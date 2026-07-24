use crate::{action_entry::ActionEntry, state};

/// MCP server config for `claude -p`: launches Playwright's own MCP server
/// (via `npx`), which drives and owns its own browser instance.
/// `--save-session` + `--codegen typescript` (the default) makes Playwright
/// MCP itself write a replayable .spec.ts of the run to `--output-dir` —
/// that's the source for `autoqa codegen` post-run instead of our actions.json,
/// which MCP-driven runs never populate.
fn mcp_config() -> String {
    let out_dir = state::runtime_dir().join("pw-session");
    serde_json::json!({
        "mcpServers": {
            "playwright": {
                "command": "npx",
                "args": [
                    "@playwright/mcp@latest",
                    "--image-responses",
                    "omit",
                    "--save-session",
                    "--output-dir",
                    out_dir.display().to_string(),
                    // "testing" caps add browser_verify_* tools — same as
                    // other tool calls, their results carry a ready-made
                    // `code` field (an `await expect(...)`), so
                    // parse_mcp_session picks them up for free.
                    "--caps",
                    "testing",
                    // Without this, MCP persists the browser profile to disk
                    // and reuses it across separate `autoqa run` invocations —
                    // cookies/localStorage from an earlier run leak into the
                    // next one, so a generated test can silently depend on
                    // state a fresh Playwright run will never have (observed:
                    // a todo item counted as "already present" by
                    // browser_verify_list_visible with no action in this
                    // session that added it). `--isolated` keeps the profile
                    // in memory, fresh every run.
                    "--isolated"
                ]
            }
        }
    })
    .to_string()
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
4. Tool choice for step 3: prefer browser_verify_element_visible over browser_verify_text_visible whenever the target has an identifiable role/name — element-based assertions survive copy/wording changes that break text matches. Reach for browser_verify_text_visible only when there's no meaningful element to target (e.g. verifying a sentence of body text). Use browser_verify_value for form field contents and browser_verify_list_visible for a set of list items.";

/// jq filter turning claude's raw stream-json NDJSON into readable lines:
/// 🤔 thinking, → tool calls (with args), ← tool results, 💬 assistant text,
/// ✅ final result. Long strings/blobs are truncated so image base64 data
/// doesn't flood the terminal.
const LOG_FILTER: &str = r#"
def trunc: if (type == "string" and length > 300) then .[0:300] + "…" else . end;
if .type == "assistant" then
  (.message.content[]? |
    if .type == "thinking" and (.thinking | length) > 0 then "🤔 " + (.thinking | trunc)
    elif .type == "tool_use" then "→ " + .name + " " + (.input | tostring | trunc)
    elif .type == "text" then "💬 " + .text
    else empty end)
elif .type == "user" then
  (.message.content[]? |
    if .type == "tool_result" then
      "  ← " + (if (.content | type) == "array" then "[image]" else (.content | tostring | trunc) end)
    else empty end)
elif .type == "result" then
  "\n✅ " + .result
else empty end
"#;

/// Runs `claude -p` wired to the Playwright MCP server.
pub async fn cmd_run(query: &str) -> anyhow::Result<()> {
    // Persisted so `autoqa codegen`/`autoqa review` (run later, in a separate
    // invocation) can title the generated test after the actual task
    // instead of a generic placeholder.
    std::fs::create_dir_all(state::runtime_dir())?;
    std::fs::write(state::runtime_dir().join("last-query.txt"), query)?;

    let mut claude = std::process::Command::new("claude")
        .arg("-p")
        .arg(query)
        .arg("--mcp-config")
        .arg(mcp_config())
        .arg("--strict-mcp-config")
        .arg("--append-system-prompt")
        .arg(SYSTEM_PROMPT)
        .arg("--allowedTools")
        .arg("mcp__playwright")
        .arg("--permission-mode")
        .arg("acceptEdits")
        // Emits one NDJSON event per step (thinking, each tool call incl.
        // the exact Playwright MCP call, tool results, final text) instead of
        // just the final answer — this is what surfaces "what agent thinks
        // and does". Plain --verbose with the default text format shows
        // nothing extra; stream-json requires --verbose to be set.
        .arg("--output-format")
        .arg("stream-json")
        .arg("--verbose")
        .stdout(std::process::Stdio::piped())
        .spawn()?;

    // Pipe claude's raw NDJSON through jq for human-readable output instead
    // of dumping the raw event stream.
    let jq_status = std::process::Command::new("jq")
        .args(["-r", LOG_FILTER])
        .stdin(std::process::Stdio::from(claude.stdout.take().unwrap()))
        .status();

    let status = claude.wait()?;
    let _ = jq_status;
    if !status.success() {
        anyhow::bail!("claude exited with status {status}");
    }
    Ok(())
}

/// Sends the current actions.json array + a natural-language instruction to
/// `claude -p` and asks it to return the edited array as raw JSON — no MCP
/// tools, no browser, this is a pure text transformation. Does not write
/// actions.json itself; the caller only persists on successful parse, so a
/// bad model response can never corrupt state.
pub async fn edit_actions_via_chat(
    current: &[ActionEntry],
    instruction: &str,
) -> anyhow::Result<Vec<ActionEntry>> {
    let current_json = serde_json::to_string_pretty(current)?;
    let prompt = format!(
        "Current steps (JSON array of {{action, assertion}} — each is a raw \
         Playwright JS statement string, assertion may be empty):\n{current_json}\n\n\
         Instruction: {instruction}\n\n\
         Return the FULL updated array reflecting the instruction. Output ONLY the raw \
         JSON array, no markdown code fences, no prose before or after, no explanation."
    );

    let output = tokio::process::Command::new("claude")
        .arg("-p")
        .arg(&prompt)
        .arg("--append-system-prompt")
        .arg(
            "You output strictly valid JSON and nothing else. Never wrap output in \
             markdown fences. Never include commentary.",
        )
        // No tool access needed for a pure text→JSON task — disallow tool use
        // outright so there's no chance of a permission prompt hanging this
        // headless call, and no chance the model wanders into Bash/WebSearch.
        .arg("--allowedTools")
        .arg("")
        .output()
        .await?;

    if !output.status.success() {
        anyhow::bail!(
            "claude exited with status {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let raw = String::from_utf8_lossy(&output.stdout);
    let cleaned = strip_markdown_fence(raw.trim());
    let updated: Vec<ActionEntry> = serde_json::from_str(cleaned)
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
}
