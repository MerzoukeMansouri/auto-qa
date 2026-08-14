use crate::{
    block::TestStep,
    harness::{Harness, McpServerSpec},
    state, tui,
};
use tokio::io::{AsyncBufReadExt, BufReader};

/// autoqa launches Chrome itself (rather than letting Playwright MCP launch
/// its own) so a second MCP server — `block_server_mcp_spec` — can attach to
/// the *same* browser via CDP and replay a block's steps deterministically
/// alongside whatever the agent is doing through Playwright MCP. Common
/// per-OS install locations for Chrome/Chromium; first match wins.
pub(crate) fn find_chrome_executable() -> anyhow::Result<std::path::PathBuf> {
    let candidates: &[&str] = if cfg!(target_os = "macos") {
        &[
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
            "/Applications/Chromium.app/Contents/MacOS/Chromium",
        ]
    } else if cfg!(target_os = "windows") {
        &[
            r"C:\Program Files\Google\Chrome\Application\chrome.exe",
            r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
        ]
    } else {
        &[
            "/usr/bin/google-chrome",
            "/usr/bin/chromium",
            "/usr/bin/chromium-browser",
        ]
    };
    candidates
        .iter()
        .map(std::path::PathBuf::from)
        .find(|p| p.is_file())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "no Chrome/Chromium executable found in the usual install locations — \
                 install Google Chrome (or Chromium) to run `autoqa run`"
            )
        })
}

/// Spawns Chrome with a CDP debug port and returns the child (kept alive for
/// the run's lifetime — dropping/killing it ends the browser) plus its
/// `ws://` CDP endpoint, parsed off Chrome's own stderr announcement
/// ("DevTools listening on ws://..."). `--remote-debugging-port=0` picks a
/// free port so concurrent `autoqa run` invocations never collide.
async fn launch_chrome_with_cdp(headless: bool) -> anyhow::Result<(tokio::process::Child, String)> {
    let exe = find_chrome_executable()?;
    // A fresh profile dir per run (keyed by pid, not a fixed path) — matches
    // the isolation `playwright_mcp_spec`'s old `--isolated` flag used to
    // guarantee: no cookies/localStorage leaking from a prior `autoqa run`.
    let profile_dir = std::env::temp_dir().join(format!("autoqa-chrome-{}", std::process::id()));
    let mut cmd = tokio::process::Command::new(exe);
    cmd.arg("--remote-debugging-port=0")
        .arg(format!("--user-data-dir={}", profile_dir.display()))
        .arg("--no-first-run")
        .arg("--no-default-browser-check");
    if headless {
        // --no-sandbox: Chrome's sandbox needs setuid helpers most CI
        // containers don't have set up — without it headless Chrome just
        // fails to start under the root/container user CI runs as.
        cmd.arg("--headless=new").arg("--no-sandbox");
    }
    let mut child = cmd.stderr(std::process::Stdio::piped()).spawn()?;

    let stderr = child.stderr.take().expect("stderr was piped");
    let mut lines = BufReader::new(stderr).lines();
    let endpoint = loop {
        let Some(line) = lines.next_line().await? else {
            anyhow::bail!("Chrome exited before announcing its CDP endpoint");
        };
        if let Some(idx) = line.find("DevTools listening on ") {
            break line[idx + "DevTools listening on ".len()..]
                .trim()
                .to_string();
        }
    };

    // Chrome keeps writing to stderr for its whole lifetime — drain it in
    // the background so the pipe never fills up and blocks the browser.
    tokio::spawn(async move { while lines.next_line().await.ok().flatten().is_some() {} });

    Ok((child, endpoint))
}

/// Writes `node/block-server`'s script + package.json out to the runtime
/// dir — embedded in the binary (a Homebrew install has no `node/` dir
/// alongside it), so this has to run before anything can `npm install` or
/// `node server.mjs` there. Shared with `doctor.rs`'s auto-install step,
/// which used to skip this and run `npm install` in a directory that didn't
/// exist yet on a machine with no prior `autoqa` run — "No such file or
/// directory".
pub(crate) fn write_block_server_files() -> anyhow::Result<std::path::PathBuf> {
    let dir = state::runtime_dir().join("block-server");
    std::fs::create_dir_all(&dir)?;
    std::fs::write(
        dir.join("package.json"),
        include_str!("../node/block-server/package.json"),
    )?;
    std::fs::write(
        dir.join("server.mjs"),
        include_str!("../node/block-server/server.mjs"),
    )?;
    Ok(dir)
}

/// autoqa's own MCP server (node/block-server) — exposes `list_blocks` and
/// `run_block` to the agent, replaying a block's steps via `connectOverCDP`
/// against the same browser Playwright MCP is driving (see
/// `launch_chrome_with_cdp`).
async fn block_server_mcp_spec(
    cdp_endpoint: &str,
    pw_session_baseline: &str,
) -> anyhow::Result<McpServerSpec> {
    let dir = write_block_server_files()?;

    if !dir.join("node_modules").is_dir() {
        let status = tokio::process::Command::new("npm")
            .args(["install", "--registry", state::NPM_PUBLIC_REGISTRY])
            .current_dir(&dir)
            .status()
            .await?;
        anyhow::ensure!(
            status.success(),
            "npm install for autoqa-blocks server failed"
        );
    }

    Ok(McpServerSpec {
        name: "autoqa-blocks",
        command: "node",
        args: vec![
            dir.join("server.mjs").display().to_string(),
            "--cdp-endpoint".to_string(),
            cdp_endpoint.to_string(),
            "--blocks-dir".to_string(),
            state::blocks_dir().display().to_string(),
            "--params-file".to_string(),
            state::params_path().display().to_string(),
            "--run-log".to_string(),
            state::run_block_log_path().display().to_string(),
            "--pw-session-dir".to_string(),
            state::runtime_dir()
                .join("pw-session")
                .display()
                .to_string(),
            "--pw-session-baseline".to_string(),
            pw_session_baseline.to_string(),
        ],
        env: vec![],
    })
}

/// The Playwright MCP server every harness wires up for `cmd_run`: launches
/// Playwright's own MCP server (via `npx`), which drives and owns its own
/// browser instance. `--save-session` + `--codegen typescript` (the default)
/// makes Playwright MCP itself write a replayable .spec.ts of the run to
/// `--output-dir` — that's the source for `autoqa codegen` post-run instead
/// of our actions.json, which MCP-driven runs never populate.
fn playwright_mcp_spec(locale: &str, cdp_endpoint: &str) -> anyhow::Result<McpServerSpec> {
    let out_dir = state::runtime_dir().join("pw-session");
    let mut args = vec![
        "@playwright/mcp@latest".to_string(),
        "--cdp-endpoint".to_string(),
        cdp_endpoint.to_string(),
        "--image-responses".to_string(),
        "omit".to_string(),
        "--save-session".to_string(),
        "--output-dir".to_string(),
        out_dir.display().to_string(),
    ];

    // `--locale` isn't a Playwright MCP CLI flag; it only takes locale via
    // a JSON `--config` file's `contextOptions.locale`. Without this, MCP's
    // browser context defaults to en-US regardless of what a later
    // `playwright test` run is configured for (playwright-tests/playwright.config.ts),
    // so a recorded session and the generated test can silently disagree on
    // date formats / form input.
    let config_path = state::runtime_dir().join("pw-mcp-config.json");
    std::fs::create_dir_all(state::runtime_dir())?;
    std::fs::write(
        &config_path,
        serde_json::json!({ "contextOptions": { "locale": locale } }).to_string(),
    )?;
    args.push("--config".to_string());
    args.push(config_path.display().to_string());

    // "testing" caps add browser_verify_* tools — same as other tool calls,
    // their results carry a ready-made `code` field (an `await expect(...)`),
    // so parse_mcp_session picks them up for free.
    args.push("--caps".to_string());
    args.push("testing".to_string());

    // No `--isolated` here: with `--cdp-endpoint`, MCP attaches to the
    // browser `launch_chrome_with_cdp` already launched instead of starting
    // its own — that Chrome process owns a fresh-per-run profile dir
    // (see `launch_chrome_with_cdp`), which is what `--isolated` used to
    // guarantee for MCP's own launch.

    Ok(McpServerSpec {
        name: "playwright",
        command: "npx",
        args,
        // `npx` inherits whatever registry the caller's .npmrc points at —
        // a corporate mirror without @playwright/mcp 404s and the MCP server
        // silently never starts, leaving the agent with no browser tools.
        // Force the public registry, same as block_server_mcp_spec's own
        // `npm install` above.
        env: vec![(
            "NPM_CONFIG_REGISTRY",
            state::NPM_PUBLIC_REGISTRY.to_string(),
        )],
    })
}

/// Belt-and-suspenders: --allowedTools mcp__playwright already blocks other
/// tools, but the model has still been observed answering from training
/// knowledge instead of acting when a task looks answerable without a
/// browser. Force it to actually drive the page.
///
/// Four distinct directives, kept structurally separate rather than one
/// run-on paragraph — numbered rules hold up better than prose when a model
/// is optimizing for "answer the question" over "follow every constraint".
const SYSTEM_PROMPT: &str = "You must complete this task by driving a real browser through the playwright MCP tools (mcp__playwright__*). Follow these rules:\n\
\n\
1. Never answer from memory or prior knowledge. Navigate, click, type, and read the actual rendered page for every fact you report.\n\
\n\
2. This session is recorded to generate a replayable Playwright test afterward. Perform only the actions strictly necessary to complete the task, in a clean, deliberate, linear sequence — no exploratory clicks, no backtracking, no dead-end navigation you don't use.\n\
\n\
3. After EVERY action that changes or reveals page state (navigation, click, form fill, submit) — not just once at the end — call the single most relevant mcp__playwright__browser_verify_* tool for what that specific action was supposed to achieve, before moving to the next action. This applies regardless of whether the task's wording asks for verification: the task never mentioning 'verify' or 'check' is the default case, not a signal to skip this. A click on a 'Sign In' button gets verified by confirming the post-login element/text appears; typing into a field gets verified with browser_verify_value; a search gets verified by confirming a result is visible. Each of these becomes a real `expect(...)` assertion in the generated test — an action with no matching verify is a step the generated test cannot catch a regression on.\n\
\n\
4. Tool choice for step 3: prefer browser_verify_element_visible over browser_verify_text_visible whenever the target has an identifiable role/name — element-based assertions survive copy/wording changes that break text matches. Reach for browser_verify_text_visible only when there's no meaningful element to target (e.g. verifying a sentence of body text). Use browser_verify_value for form field contents and browser_verify_list_visible for a set of list items.\n\
\n\
5. Before starting, call mcp__autoqa-blocks__list_blocks to see what reusable step blocks already exist. If the task matches one (e.g. a known 'login' block for a task that starts with logging in), call mcp__autoqa-blocks__run_block with its slug and a binding for every placeholder it lists, instead of re-driving those steps yourself through mcp__playwright__* — it replays the exact recorded steps deterministically. Only drive the browser directly for the parts no existing block covers.\n\
\n\
6. After any mcp__autoqa-blocks__run_block call, its own steps are invisible to you — you were not shown them. Never assume what it left the page in (already navigated, an item already added, already logged in); call mcp__playwright__browser_snapshot immediately after and act only on what it actually shows. Repeating a setup action the block already performed (a duplicate navigation, a duplicate item add) is a bug, not a safe default.";

/// Runs the selected harness wired to the Playwright MCP server (attached,
/// via CDP, to a Chrome instance autoqa itself launches and owns) and to
/// autoqa's own `run_block` MCP server, sharing that same CDP endpoint.
pub async fn cmd_run(
    harness: Harness,
    query: &str,
    locale: &str,
    model: Option<&str>,
    headless: bool,
    no_tui: bool,
) -> anyhow::Result<()> {
    // Fresh per run — a stale log from a prior run would otherwise get
    // merged into this run's session (see `state::read_run_block_log`).
    state::clear_run_block_log()?;

    // Pre-run TUI: pick which saved blocks to replay, and in what order,
    // before the agent starts — not a mid-run pause, a plan built up front.
    // --no-tui (no controlling terminal, e.g. CI) skips the picker outright:
    // there's no one to pick, so the run proceeds with no blocks chained in.
    let plan = if no_tui {
        Vec::new()
    } else {
        let available_blocks = state::list_blocks().unwrap_or_default();
        let params = state::read_params();
        tui::pick_blocks(&available_blocks, &params)?
    };

    // Persisted so `autoqa codegen`/`autoqa review` (run later, in a separate
    // invocation) can title the generated test after the actual task
    // instead of a generic placeholder — the *original* query, not the
    // block-plan prefix appended below (that's internal instruction text
    // for the agent, not a fit test title, and generate() only escapes `'`
    // and `\` in the title so multi-line text with unescaped quotes broke
    // the emitted `.spec.ts`'s syntax).
    std::fs::create_dir_all(state::runtime_dir())?;
    std::fs::write(state::runtime_dir().join("last-query.txt"), query)?;

    let query = format!("{}{query}", tui::render_plan_prefix(&plan));

    // Snapshotted before the harness (and therefore Playwright MCP) ever
    // starts — see `state::max_pw_session_dir_name`.
    let pw_session_baseline = state::max_pw_session_dir_name().unwrap_or_default();

    let (mut chrome, cdp_endpoint) = launch_chrome_with_cdp(headless).await?;
    let run_result = run_harness(
        harness,
        &query,
        locale,
        &cdp_endpoint,
        &pw_session_baseline,
        model,
        no_tui,
    )
    .await;

    // Chrome is autoqa's own child, not the harness's — always tear it down
    // on the way out, run success or failure, or it leaks past this process.
    let _ = chrome.kill().await;
    run_result
}

async fn run_harness(
    harness: Harness,
    query: &str,
    locale: &str,
    cdp_endpoint: &str,
    pw_session_baseline: &str,
    model: Option<&str>,
    no_tui: bool,
) -> anyhow::Result<()> {
    let mcp_specs = vec![
        playwright_mcp_spec(locale, cdp_endpoint)?,
        block_server_mcp_spec(cdp_endpoint, pw_session_baseline).await?,
    ];
    let mut cmd = harness.build_run_command(query, &mcp_specs, SYSTEM_PROMPT, model)?;
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    let child = cmd.spawn()?;

    // Takes over the terminal for the run's duration, streaming the
    // harness's (jq-filtered, or raw) output into a live-progress pane —
    // replaces the old direct-to-stdout print loop.
    let log_filter = harness.log_filter();
    let status =
        tokio::task::spawn_blocking(move || tui::run_live(child, log_filter, no_tui)).await??;

    if !status.success() {
        anyhow::bail!("harness exited with status {status}");
    }
    Ok(())
}

/// Sends the current actions.json array + a natural-language instruction to
/// the selected harness and asks it to return the edited array as raw JSON —
/// no MCP tools, no browser, this is a pure text transformation. Does not
/// write actions.json itself; the caller only persists on successful parse,
/// so a bad model response can never corrupt state.
pub async fn edit_actions_via_chat(
    harness: Harness,
    current: &[TestStep],
    instruction: &str,
    model: Option<&str>,
) -> anyhow::Result<Vec<TestStep>> {
    let current_json = serde_json::to_string_pretty(current)?;
    let prompt = format!(
        "Current steps (JSON array of tagged step objects). Each item is either \
         {{\"kind\": \"step\", \"action\": ..., \"assertion\": ...}} (a raw Playwright JS \
         statement string, assertion may be empty) or {{\"kind\": \"block\", \"slug\": ..., \
         \"bindings\": {{...}}}} (a reference to a reusable named block, with a map of \
         placeholder name to param name):\n{current_json}\n\n\
         Instruction: {instruction}\n\n\
         Return the FULL updated array reflecting the instruction. Output ONLY the raw \
         JSON array, no markdown code fences, no prose before or after, no explanation."
    );
    let system_prompt = "You output strictly valid JSON and nothing else. Never wrap output in \
             markdown fences. Never include commentary.";

    let cmd = harness.build_chat_command(&prompt, system_prompt, model)?;
    let mut cmd = tokio::process::Command::from(cmd);
    let output = cmd.output().await?;

    if !output.status.success() {
        anyhow::bail!(
            "harness exited with status {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let raw = String::from_utf8_lossy(&output.stdout);
    let cleaned = strip_markdown_fence(raw.trim());
    let updated: Vec<TestStep> = serde_json::from_str(cleaned)
        .map_err(|e| anyhow::anyhow!("model did not return valid JSON: {e}\nraw output: {raw}"))?;
    Ok(updated)
}

/// Best-effort: some models wrap JSON in ```json ... ``` fences even when
/// told not to. Strip a single leading/trailing fence if present rather than
/// failing the whole request over formatting.
fn strip_markdown_fence(s: &str) -> &str {
    let s = s
        .strip_prefix("```json")
        .or_else(|| s.strip_prefix("```"))
        .unwrap_or(s);
    s.strip_suffix("```").unwrap_or(s).trim()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_json_fence() {
        assert_eq!(strip_markdown_fence("```json\n[1,2]\n```"), "[1,2]");
        assert_eq!(strip_markdown_fence("[1,2]"), "[1,2]");
    }

    #[tokio::test]
    #[ignore]
    async fn live_copilot_chat_edits_json() {
        let current = vec![TestStep::Step {
            action: "click button".into(),
            assertion: String::new(),
        }];
        let updated = edit_actions_via_chat(
            crate::harness::Harness::Copilot,
            &current,
            "no-op: return unchanged",
            None,
        )
        .await
        .unwrap();
        assert_eq!(updated.len(), 1);
    }

    #[tokio::test]
    #[ignore]
    async fn live_opencode_chat_edits_json() {
        let current = vec![TestStep::Step {
            action: "click button".into(),
            assertion: String::new(),
        }];
        let updated = edit_actions_via_chat(
            crate::harness::Harness::Opencode,
            &current,
            "no-op: return unchanged",
            None,
        )
        .await
        .unwrap();
        assert_eq!(updated.len(), 1);
    }
}
