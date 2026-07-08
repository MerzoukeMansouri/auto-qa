mod browser;
mod cdp_actions;
mod cli;
mod output;
mod state;

use clap::Parser;
use cli::{Cli, Commands};
use output::ActionResult;
use std::time::Duration;

/// Pinned viewport size — bounds vision-model tokens per screenshot
/// regardless of the host display's actual resolution.
const VIEWPORT_WIDTH: u32 = 1024;
const VIEWPORT_HEIGHT: u32 = 768;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Open { url } => cmd_open(url.as_deref()).await,
        Commands::Close => cmd_close().await,
        Commands::Run { query } => cmd_run(&query).await,
        other => cmd_action(other).await,
    }
}

async fn cmd_open(url: Option<&str>) -> anyhow::Result<()> {
    if state::session_exists() {
        anyhow::bail!("a session is already open — run 'cua close' first");
    }
    state::clear();
    state::ensure_runtime_dir()?;

    let port = state::port();
    let user_data_dir = std::env::temp_dir().join(format!("cu-agent-{}", std::process::id()));
    std::fs::create_dir_all(&user_data_dir)?;

    let pid = browser::launch_chrome(port, &user_data_dir)?;
    state::write("chrome.pid", &pid.to_string())?;
    state::write("port", &port.to_string())?;
    state::write("user-data-dir", &user_data_dir.display().to_string())?;

    browser::wait_for_http_ready(port, Duration::from_secs(10)).await?;
    // Created via the plain HTTP /json/new endpoint, not chromiumoxide's
    // WebSocket-session new_page() — a tab created over a CDP session gets
    // reset to chrome://newtab/ by Chrome once that creating session fully
    // disconnects, which happens the moment this short-lived process exits.
    // A tab created over HTTP has no such owning session and persists fine.
    let target_id = browser::create_page_via_http(port, url.unwrap_or("about:blank"))?;

    let b = browser::connect(port).await?;
    let page = browser::get_active_page(&b, Some(&target_id)).await?;
    browser::set_viewport(&page, VIEWPORT_WIDTH, VIEWPORT_HEIGHT).await?;
    if url.is_some() {
        page.wait_for_navigation().await.ok();
    }
    browser::close_other_pages(&b, &page).await?;
    state::set_target_id(page.target_id().inner())?;

    let result = after_action(&page).await?;
    state::append_log("open", &result.url);
    output::print(&result);
    browser::detach(&b, &page).await?;
    Ok(())
}

async fn cmd_close() -> anyhow::Result<()> {
    if let Some(pid) = state::pid() {
        let _ = std::process::Command::new("kill").arg(pid.to_string()).status();
    }
    if let Some(dir) = state::read("user-data-dir") {
        let _ = std::fs::remove_dir_all(dir);
    }
    state::clear();
    Ok(())
}

async fn cmd_action(command: Commands) -> anyhow::Result<()> {
    if !state::session_exists() {
        anyhow::bail!("no browser session — run 'cua open' first");
    }
    let port = state::port();
    let b = browser::connect(port).await?;
    let page = browser::get_active_page(&b, state::target_id().as_deref()).await?;
    state::set_target_id(page.target_id().inner())?;

    let action_desc = format!("{command:?}");

    match command {
        Commands::Screenshot => {
            let result = after_action(&page).await?;
            state::append_log(&action_desc, &result.url);
            output::print(&result);
            browser::detach(&b, &page).await?;
            return Ok(());
        }
        Commands::Click { x, y } => cdp_actions::click(&page, x, y).await?,
        Commands::DoubleClick { x, y } => cdp_actions::double_click(&page, x, y).await?,
        Commands::TripleClick { x, y } => cdp_actions::triple_click(&page, x, y).await?,
        Commands::RightClick { x, y } => cdp_actions::right_click(&page, x, y).await?,
        Commands::MiddleClick { x, y } => cdp_actions::middle_click(&page, x, y).await?,
        Commands::MouseDown { x, y } => cdp_actions::mouse_down(&page, x, y).await?,
        Commands::MouseUp { x, y } => cdp_actions::mouse_up(&page, x, y).await?,
        Commands::Hover { x, y } => cdp_actions::hover(&page, x, y).await?,
        Commands::Drag { x1, y1, x2, y2 } => cdp_actions::drag(&page, x1, y1, x2, y2).await?,
        Commands::Type { text, enter } => cdp_actions::type_text(&page, &text, enter).await?,
        Commands::Key { combo } => cdp_actions::key(&page, &combo).await?,
        Commands::KeyDown { key } => cdp_actions::key_down(&page, &key).await?,
        Commands::KeyUp { key } => cdp_actions::key_up(&page, &key).await?,
        Commands::Scroll { x, y, direction, magnitude } => {
            cdp_actions::scroll(&page, x, y, &direction, magnitude).await?
        }
        Commands::Navigate { url } => cdp_actions::navigate(&page, &url).await?,
        Commands::Back => cdp_actions::back(&page).await?,
        Commands::Forward => cdp_actions::forward(&page).await?,
        Commands::Wait { seconds } => cdp_actions::wait(seconds).await?,
        Commands::Open { .. } | Commands::Close | Commands::Run { .. } => unreachable!(),
    }

    browser::enforce_single_tab(&b, &page).await?;
    let result = after_action(&page).await?;
    state::append_log(&action_desc, &result.url);
    output::print(&result);
    browser::detach(&b, &page).await?;
    Ok(())
}

async fn after_action(page: &chromiumoxide::Page) -> anyhow::Result<ActionResult> {
    // An action may have triggered a navigation (e.g. clicking a link) that's
    // still in flight — wait briefly for it, then let rendering settle, before
    // capturing the screenshot. Mirrors the Python reference's
    // wait_for_load_state() + sleep(0.5) after every action.
    let _ = tokio::time::timeout(Duration::from_millis(1500), page.wait_for_navigation()).await;
    tokio::time::sleep(Duration::from_millis(300)).await;

    let out_path = state::screenshot_path();
    browser::take_screenshot(page, &out_path).await?;
    let url = page.url().await?.unwrap_or_default();
    let viewport = page
        .evaluate("[window.innerWidth, window.innerHeight]")
        .await
        .ok()
        .and_then(|r| r.into_value::<(u32, u32)>().ok());
    Ok(ActionResult {
        url,
        screenshot_path: out_path.display().to_string(),
        viewport,
    })
}

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

async fn cmd_run(query: &str) -> anyhow::Result<()> {
    if !state::session_exists() {
        cmd_open(None).await?;
    }
    let mut claude = std::process::Command::new("claude")
        .arg("-p")
        .arg(query)
        .arg("--allowedTools")
        .arg("Bash,Read")
        .arg("--permission-mode")
        .arg("acceptEdits")
        // Emits one NDJSON event per step (thinking, each tool call incl.
        // the exact `cua` command, tool results, final text) instead of just
        // the final answer — this is what surfaces "what agent thinks and
        // does". Plain --verbose with the default text format shows nothing
        // extra; stream-json requires --verbose to be set.
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

    let claude_status = claude.wait();
    let close_result = cmd_close().await;

    let status = claude_status?;
    let _ = jq_status;
    close_result?;
    if !status.success() {
        anyhow::bail!("claude exited with status {status}");
    }
    Ok(())
}
