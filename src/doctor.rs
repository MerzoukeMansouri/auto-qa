use crate::{harness::Harness, state};
use crossterm::event::{self, Event, KeyEventKind};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block as UiBlock, Borders, List, ListItem, Paragraph, Wrap};
use ratatui::Terminal;
use std::time::Duration;

/// `*Sdk` harness variants run a bundled node script directly (no CLI binary
/// on PATH), authenticating via an API key env var instead of a login flow —
/// every place that needs to tell the two families apart routes through
/// this single predicate rather than re-listing the variants.
fn is_sdk_harness(h: Harness) -> bool {
    matches!(h, Harness::ClaudeSdk | Harness::GeminiSdk)
}

/// Snapshot of what was detected on a prior `doctor::ensure` pass, scoped to
/// whichever harness was selected at the time. Compared field-for-field
/// against a fresh (install-free) detection on every run — equal ⇒ skip the
/// checklist screen entirely. Any drift (a `brew upgrade`, a `nvm use` swap,
/// an npm dir wiped by hand, an API key exported/unset, or a different
/// harness picked) naturally invalidates it without tracking timestamps.
#[derive(serde::Serialize, serde::Deserialize, PartialEq, Clone, Debug, Default)]
struct Snapshot {
    node_version: Option<String>,
    chrome_path: Option<String>,
    harness: Option<String>,
    /// CLI `--version` output for a CLI harness, or `Some("set")` for an
    /// `*Sdk` harness with its API key present — `None` either way means
    /// the harness isn't usable yet.
    harness_status: Option<String>,
    block_server_deps: bool,
    sdk_deps: bool,
    chromium_installed: bool,
}

#[derive(Clone, Copy, PartialEq)]
enum Status {
    Pending,
    Running,
    Ok,
    Failed,
}

struct Row {
    label: String,
    status: Status,
    detail: String,
    /// Detect-only rows that came back missing/too-old block the run —
    /// distinct from a `Failed` auto-install, which is also blocking but
    /// came from a command we ran ourselves rather than the environment.
    blocking: bool,
}

/// Entry point, called right after the harness is resolved and before
/// `cmd_run`/`review_server::serve` spawn anything. Fast-paths straight
/// through on a cache hit; otherwise renders the checklist TUI, running the
/// safe auto-installs live and blocking on anything it can't install itself
/// (system Chrome, Node, the selected harness's CLI or API key — all
/// third-party, all needing separate setup/auth we can't do on the user's
/// behalf).
pub fn ensure(harness: Harness, recheck: bool) -> anyhow::Result<()> {
    let fresh = detect(harness);
    let cached = (!recheck).then(read_cache).flatten();

    if cached.as_ref() == Some(&fresh) && all_ok(&fresh) {
        return Ok(());
    }

    run_checklist(harness)
}

fn all_ok(s: &Snapshot) -> bool {
    s.node_version.is_some()
        && s.chrome_path.is_some()
        && s.harness_status.is_some()
        && s.block_server_deps
        && s.sdk_deps
        && s.chromium_installed
}

fn read_cache() -> Option<Snapshot> {
    let raw = std::fs::read_to_string(state::doctor_path()).ok()?;
    serde_json::from_str(&raw).ok()
}

fn write_cache(s: &Snapshot) -> anyhow::Result<()> {
    std::fs::create_dir_all(state::runtime_dir())?;
    std::fs::write(state::doctor_path(), serde_json::to_string_pretty(s)?)?;
    Ok(())
}

/// Cheap, install-free detection pass — no network, no subprocess spawns
/// beyond one `--version`/API-key probe for the selected harness, safe to
/// run on every single invocation.
fn detect(harness: Harness) -> Snapshot {
    let node_version = detect_node();
    let chrome_path = crate::agent::find_chrome_executable()
        .ok()
        .map(|p| p.display().to_string());
    let harness_status = if is_sdk_harness(harness) {
        api_key_present(harness).then(|| "set".to_string())
    } else {
        detect_harness_cli(harness)
    };
    let block_server_deps = state::runtime_dir()
        .join("block-server")
        .join("node_modules")
        .is_dir();
    let sdk_deps = sdk_deps_ok(harness);
    let chromium_installed = playwright_chromium_installed();

    Snapshot {
        node_version,
        chrome_path,
        harness: Some(harness.to_string()),
        harness_status,
        block_server_deps,
        sdk_deps,
        chromium_installed,
    }
}

/// `node --version` (e.g. "v20.11.0"), only kept if major >= 20 — an older
/// Node still "detects" but doesn't satisfy the requirement, so callers see
/// it as missing (`None`) rather than silently accepting it.
fn detect_node() -> Option<String> {
    let out = std::process::Command::new("node")
        .arg("--version")
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let version = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let major: u32 = version
        .trim_start_matches('v')
        .split('.')
        .next()?
        .parse()
        .ok()?;
    (major >= 20).then_some(version)
}

fn detect_harness_cli(harness: Harness) -> Option<String> {
    let name = harness.to_string();
    let out = std::process::Command::new(&name)
        .arg("--version")
        .output()
        .ok()?;
    (out.status.success() || !out.stdout.is_empty())
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// `*Sdk` harness variants run a bundled node script directly (no CLI binary
/// on PATH), authenticating via an API key env var instead of a login flow.
fn api_key_env_var(harness: Harness) -> &'static str {
    match harness {
        Harness::ClaudeSdk => "ANTHROPIC_API_KEY",
        Harness::GeminiSdk => "GEMINI_API_KEY",
        _ => unreachable!("api key check only applies to SDK harnesses"),
    }
}

fn api_key_present(harness: Harness) -> bool {
    std::env::var(api_key_env_var(harness)).is_ok_and(|v| !v.trim().is_empty())
}

fn sdk_deps_ok(harness: Harness) -> bool {
    if !is_sdk_harness(harness) {
        return true;
    }
    // `harness.to_string()` doubles as the runtime dir name for `*Sdk`
    // variants — matches `ensure_claude_sdk_script`/`ensure_gemini_sdk_script`
    // in harness.rs, which extract each script into that same-named dir.
    state::runtime_dir()
        .join(harness.to_string())
        .join("node_modules")
        .is_dir()
}

/// Playwright's own browser cache dir, per OS — matches Playwright's
/// documented default (overridable by users via `PLAYWRIGHT_BROWSERS_PATH`,
/// which takes precedence here too since that's what `npx playwright
/// install` itself would honor).
fn playwright_cache_dir() -> Option<std::path::PathBuf> {
    if let Ok(dir) = std::env::var("PLAYWRIGHT_BROWSERS_PATH") {
        return Some(std::path::PathBuf::from(dir));
    }
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .ok()?;
    let home = std::path::PathBuf::from(home);
    Some(if cfg!(target_os = "macos") {
        home.join("Library/Caches/ms-playwright")
    } else if cfg!(target_os = "windows") {
        std::env::var("LOCALAPPDATA")
            .map(std::path::PathBuf::from)
            .unwrap_or(home)
            .join("ms-playwright")
    } else {
        home.join(".cache/ms-playwright")
    })
}

fn playwright_chromium_installed() -> bool {
    let Some(dir) = playwright_cache_dir() else {
        return false;
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    entries.filter_map(|e| e.ok()).any(|e| {
        e.file_name()
            .to_str()
            .is_some_and(|n| n.starts_with("chromium-"))
    })
}

fn enter_tui() -> anyhow::Result<Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>> {
    enable_raw_mode()?;
    std::io::stdout().execute(EnterAlternateScreen)?;
    Ok(Terminal::new(ratatui::backend::CrosstermBackend::new(
        std::io::stdout(),
    ))?)
}

fn leave_tui() -> anyhow::Result<()> {
    disable_raw_mode()?;
    std::io::stdout().execute(LeaveAlternateScreen)?;
    Ok(())
}

fn set_running(rows: &mut [Row], label: &str) {
    if let Some(row) = rows.iter_mut().find(|r| r.label == label) {
        row.status = Status::Running;
    }
}

/// Runs one detect-only check whose result is "found this value, or not" —
/// Node/Chrome versions, the selected harness's CLI version or API key
/// presence. Flips the row Pending -> Running -> Ok/Failed with a redraw in
/// between so it's visible as it happens, not just as a final static state.
fn run_detect_opt(
    term: &mut Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    rows: &mut [Row],
    log: &[String],
    label: &str,
    check: impl FnOnce() -> Option<String>,
    missing_detail: impl FnOnce() -> String,
) -> anyhow::Result<Option<String>> {
    set_running(rows, label);
    draw(term, rows, log)?;
    let value = check();
    if let Some(row) = rows.iter_mut().find(|r| r.label == label) {
        row.status = if value.is_some() {
            Status::Ok
        } else {
            Status::Failed
        };
        row.detail = value.clone().unwrap_or_else(missing_detail);
        row.blocking = value.is_none();
    }
    draw(term, rows, log)?;
    Ok(value)
}

/// Same live Pending -> Running -> ... progression as `run_detect_opt`, but
/// for the three deps/browser rows that auto-install themselves right after
/// — missing here means Pending (an install is about to happen), never
/// Failed, and never blocking.
fn run_detect_bool(
    term: &mut Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    rows: &mut [Row],
    log: &[String],
    label: &str,
    check: impl FnOnce() -> bool,
) -> anyhow::Result<bool> {
    set_running(rows, label);
    draw(term, rows, log)?;
    let ok = check();
    if let Some(row) = rows.iter_mut().find(|r| r.label == label) {
        row.status = if ok { Status::Ok } else { Status::Pending };
    }
    draw(term, rows, log)?;
    Ok(ok)
}

/// Renders the checklist + live install log. Every row — including the
/// cheap detect-only ones — flips Pending -> Running -> Ok/Failed with a
/// redraw in between, so a first run (nothing cached yet) reads as "actively
/// checking your system" instead of a screen that appears already resolved.
/// Only ever checks the selected harness — no rows for the other 6, so
/// there's nothing unrelated that could ever block the run. Streams each
/// auto-install's combined stdout/stderr into the log pane as it happens.
/// Never attempts to install Node, a browser, a harness CLI, or set an API
/// key itself.
fn run_checklist(harness: Harness) -> anyhow::Result<()> {
    let mut term = enter_tui()?;
    let mut log: Vec<String> = vec!["Checking your system...".to_string()];

    let harness_label = if is_sdk_harness(harness) {
        format!("{harness} API key ({})", api_key_env_var(harness))
    } else {
        format!("Harness CLI ({harness})")
    };

    let mut rows = vec![
        Row {
            label: "Node.js >= 20".to_string(),
            status: Status::Pending,
            detail: String::new(),
            blocking: false,
        },
        Row {
            label: "System Chrome/Chromium".to_string(),
            status: Status::Pending,
            detail: String::new(),
            blocking: false,
        },
        Row {
            label: harness_label.clone(),
            status: Status::Pending,
            detail: String::new(),
            blocking: false,
        },
    ];
    rows.push(Row {
        label: "autoqa-blocks server deps".to_string(),
        status: Status::Pending,
        detail: String::new(),
        blocking: false,
    });
    let sdk_deps_label = is_sdk_harness(harness).then(|| format!("{harness} deps"));
    if let Some(label) = &sdk_deps_label {
        rows.push(Row {
            label: label.clone(),
            status: Status::Pending,
            detail: String::new(),
            blocking: false,
        });
    }
    rows.push(Row {
        label: "Playwright chromium browser".to_string(),
        status: Status::Pending,
        detail: String::new(),
        blocking: false,
    });
    draw(&mut term, &rows, &log)?;

    let node_version = run_detect_opt(
        &mut term,
        &mut rows,
        &log,
        "Node.js >= 20",
        detect_node,
        || "not found, or older than 20 — install from https://nodejs.org".to_string(),
    )?;
    let chrome_path = run_detect_opt(
        &mut term,
        &mut rows,
        &log,
        "System Chrome/Chromium",
        || {
            crate::agent::find_chrome_executable()
                .ok()
                .map(|p| p.display().to_string())
        },
        || "not found in the usual install locations — install Google Chrome".to_string(),
    )?;

    let harness_status = if is_sdk_harness(harness) {
        let env_var = api_key_env_var(harness);
        run_detect_opt(
            &mut term,
            &mut rows,
            &log,
            &harness_label,
            || api_key_present(harness).then(|| "set".to_string()),
            || format!("{env_var} not set — export it in your shell before running"),
        )?
    } else {
        let name = harness.to_string();
        run_detect_opt(
            &mut term,
            &mut rows,
            &log,
            &harness_label,
            || detect_harness_cli(harness),
            || format!("`{name}` not found on PATH — install and authenticate it first"),
        )?
    };

    let block_server_deps = run_detect_bool(
        &mut term,
        &mut rows,
        &log,
        "autoqa-blocks server deps",
        || {
            state::runtime_dir()
                .join("block-server")
                .join("node_modules")
                .is_dir()
        },
    )?;
    let mut sdk_deps = true;
    if let Some(label) = &sdk_deps_label {
        sdk_deps = run_detect_bool(&mut term, &mut rows, &log, label, || sdk_deps_ok(harness))?;
    }
    let chromium_installed = run_detect_bool(
        &mut term,
        &mut rows,
        &log,
        "Playwright chromium browser",
        playwright_chromium_installed,
    )?;

    let fresh = Snapshot {
        node_version,
        chrome_path,
        harness: Some(harness.to_string()),
        harness_status,
        block_server_deps,
        sdk_deps,
        chromium_installed,
    };

    // Auto-installs, one at a time so the log pane reads as one coherent
    // stream instead of interleaved output from parallel npm/npx calls.
    if !fresh.block_server_deps {
        // Embed the script + package.json first — `npm install`'s
        // current_dir fails with "No such file or directory" if this
        // hasn't run yet (e.g. `autoqa doctor` on a machine with no prior
        // `autoqa run`).
        crate::agent::write_block_server_files()?;
        run_install(
            &mut term,
            &mut rows,
            &mut log,
            "autoqa-blocks server deps",
            npm_install_cmd(&state::runtime_dir().join("block-server")),
        )?;
    }
    if is_sdk_harness(harness) && !fresh.sdk_deps {
        run_install(
            &mut term,
            &mut rows,
            &mut log,
            &format!("{harness} deps"),
            npm_install_cmd(&state::runtime_dir().join(harness.to_string())),
        )?;
    }
    if !fresh.chromium_installed {
        let mut cmd = std::process::Command::new("npx");
        cmd.args(["playwright", "install", "chromium"]);
        run_install(
            &mut term,
            &mut rows,
            &mut log,
            "Playwright chromium browser",
            cmd,
        )?;
    }

    let blocked: Vec<&Row> = rows.iter().filter(|r| r.blocking).collect();
    let final_snapshot = detect(harness);
    if blocked.is_empty() {
        let _ = write_cache(&final_snapshot);
    }

    log.push(String::new());
    log.push(if blocked.is_empty() {
        "All checks passed. Press any key to continue.".to_string()
    } else {
        "Blocked — fix the item(s) above, then rerun. Press any key to exit.".to_string()
    });
    draw(&mut term, &rows, &log)?;
    wait_for_keypress()?;
    leave_tui()?;

    if blocked.is_empty() {
        Ok(())
    } else {
        let names: Vec<&str> = blocked.iter().map(|r| r.label.as_str()).collect();
        anyhow::bail!("environment check failed: {}", names.join(", "))
    }
}

fn npm_install_cmd(dir: &std::path::Path) -> std::process::Command {
    let mut cmd = std::process::Command::new("npm");
    cmd.args(["install", "--registry", state::NPM_PUBLIC_REGISTRY])
        .current_dir(dir);
    cmd
}

/// Runs one auto-install command, streaming its combined stdout+stderr into
/// the shared log pane line-by-line as it happens (mirrors `tui::run_live`'s
/// channel pattern) and updating that row's status when it exits.
fn run_install(
    term: &mut Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    rows: &mut [Row],
    log: &mut Vec<String>,
    label: &str,
    mut cmd: std::process::Command,
) -> anyhow::Result<()> {
    if let Some(row) = rows.iter_mut().find(|r| r.label == label) {
        row.status = Status::Running;
    }
    draw(term, rows, log)?;

    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    let mut child = cmd.spawn()?;
    let stdout = child.stdout.take().expect("stdout was piped");
    let stderr = child.stderr.take().expect("stderr was piped");

    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let tx_err = tx.clone();
    let out_reader = std::thread::spawn(move || {
        use std::io::BufRead;
        for line in std::io::BufReader::new(stdout)
            .lines()
            .map_while(Result::ok)
        {
            let _ = tx.send(line);
        }
    });
    let err_reader = std::thread::spawn(move || {
        use std::io::BufRead;
        for line in std::io::BufReader::new(stderr)
            .lines()
            .map_while(Result::ok)
        {
            let _ = tx_err.send(line);
        }
    });

    loop {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(line) => {
                log.push(line);
                draw(term, rows, log)?;
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                if let Ok(Some(_)) = child.try_wait() {
                    break;
                }
                draw(term, rows, log)?;
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    let _ = out_reader.join();
    let _ = err_reader.join();
    let status = child.wait()?;

    if let Some(row) = rows.iter_mut().find(|r| r.label == label) {
        row.status = if status.success() {
            Status::Ok
        } else {
            Status::Failed
        };
        // These three rows (block-server/sdk deps, Playwright chromium) are
        // always-needed, not per-harness alternatives — unlike the selected
        // harness's own CLI/API-key check, a failed install here always
        // blocks.
        row.blocking = !status.success();
        if !status.success() {
            row.detail = format!("exited with {status}");
        }
    }
    draw(term, rows, log)?;
    Ok(())
}

fn wait_for_keypress() -> anyhow::Result<()> {
    loop {
        if event::poll(Duration::from_millis(200))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    return Ok(());
                }
            }
        }
    }
}

fn draw(
    term: &mut Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    rows: &[Row],
    log: &[String],
) -> anyhow::Result<()> {
    term.draw(|f| {
        let area = f.area();
        let split = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(rows.len() as u16 + 2),
                Constraint::Min(3),
            ])
            .split(area);

        let items: Vec<ListItem> = rows
            .iter()
            .map(|r| {
                let (icon, color) = match r.status {
                    Status::Pending => ("○", Color::DarkGray),
                    Status::Running => ("◐", Color::Yellow),
                    Status::Ok => ("✓", Color::Green),
                    Status::Failed => ("✗", Color::Red),
                };
                let mut spans = vec![
                    Span::styled(format!("{icon} "), Style::default().fg(color)),
                    Span::raw(r.label.clone()),
                ];
                if !r.detail.is_empty() {
                    spans.push(Span::styled(
                        format!("  — {}", r.detail),
                        Style::default().fg(Color::DarkGray),
                    ));
                }
                ListItem::new(Line::from(spans))
            })
            .collect();
        f.render_widget(
            List::new(items).block(
                UiBlock::default()
                    .borders(Borders::ALL)
                    .title("autoqa environment check"),
            ),
            split[0],
        );

        let tail_start = log
            .len()
            .saturating_sub(split[1].height.saturating_sub(2) as usize);
        let text = log[tail_start..].join("\n");
        f.render_widget(
            Paragraph::new(text)
                .wrap(Wrap { trim: false })
                .block(UiBlock::default().borders(Borders::ALL).title("Log")),
            split[1],
        );
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_snapshot() -> Snapshot {
        Snapshot {
            node_version: Some("v20.0.0".to_string()),
            chrome_path: Some("/Applications/Google Chrome.app".to_string()),
            harness: Some("claude".to_string()),
            harness_status: Some("1.0.0".to_string()),
            block_server_deps: true,
            sdk_deps: true,
            chromium_installed: true,
        }
    }

    #[test]
    fn all_ok_true_when_everything_present() {
        assert!(all_ok(&base_snapshot()));
    }

    #[test]
    fn all_ok_false_when_harness_status_missing() {
        let mut s = base_snapshot();
        s.harness_status = None;
        assert!(!all_ok(&s));
    }

    #[test]
    fn all_ok_false_when_a_universal_check_fails() {
        let mut s = base_snapshot();
        s.chrome_path = None;
        assert!(!all_ok(&s));
    }

    #[test]
    fn all_ok_false_when_deps_not_installed() {
        let mut s = base_snapshot();
        s.block_server_deps = false;
        assert!(!all_ok(&s));
    }
}
