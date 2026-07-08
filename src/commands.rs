use crate::{browser, cdp_actions, cli::Commands, output, output::ActionResult, state};
use chromiumoxide::{Browser, Page};
use std::time::Duration;

/// Pinned viewport size — bounds vision-model tokens per screenshot
/// regardless of the host display's actual resolution.
const VIEWPORT_WIDTH: u32 = 1024;
const VIEWPORT_HEIGHT: u32 = 768;

pub async fn cmd_open(url: Option<&str>) -> anyhow::Result<()> {
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

    let (b, page) = attach_session(port, Some(&target_id)).await?;
    browser::set_viewport(&page, VIEWPORT_WIDTH, VIEWPORT_HEIGHT).await?;
    if url.is_some() {
        page.wait_for_navigation().await.ok();
    }
    browser::close_other_pages(&b, &page).await?;

    finish(&b, &page, "open").await
}

pub async fn cmd_close() -> anyhow::Result<()> {
    if let Some(pid) = state::pid() {
        let _ = std::process::Command::new("kill")
            .arg(pid.to_string())
            .status();
    }
    if let Some(dir) = state::read("user-data-dir") {
        let _ = std::fs::remove_dir_all(dir);
    }
    state::clear();
    Ok(())
}

pub async fn cmd_action(command: Commands) -> anyhow::Result<()> {
    if !state::session_exists() {
        anyhow::bail!("no browser session — run 'cua open' first");
    }
    let port = state::port();
    let (b, page) = attach_session(port, state::target_id().as_deref()).await?;
    let action_desc = format!("{command:?}");

    if matches!(command, Commands::Screenshot) {
        return finish(&b, &page, &action_desc).await;
    }

    match command {
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
        Commands::Scroll {
            x,
            y,
            direction,
            magnitude,
        } => cdp_actions::scroll(&page, x, y, &direction, magnitude).await?,
        Commands::Navigate { url } => cdp_actions::navigate(&page, &url).await?,
        Commands::Back => cdp_actions::back(&page).await?,
        Commands::Forward => cdp_actions::forward(&page).await?,
        Commands::Wait { seconds } => cdp_actions::wait(seconds).await?,
        Commands::Screenshot | Commands::Open { .. } | Commands::Close | Commands::Run { .. } => {
            unreachable!()
        }
    }

    browser::enforce_single_tab(&b, &page).await?;
    finish(&b, &page, &action_desc).await
}

/// Connects to the running Chrome session and resolves the active page,
/// pinning it as the tracked target for subsequent invocations.
async fn attach_session(port: u16, target_id: Option<&str>) -> anyhow::Result<(Browser, Page)> {
    let b = browser::connect(port).await?;
    let page = browser::get_active_page(&b, target_id).await?;
    state::set_target_id(page.target_id().inner())?;
    Ok((b, page))
}

/// Captures post-action state, logs and prints it, then cleanly detaches —
/// the common tail of every command that ends with the page in a known state.
async fn finish(b: &Browser, page: &Page, action_desc: &str) -> anyhow::Result<()> {
    let result = after_action(page).await?;
    state::append_log(action_desc, &result.url);
    output::print(&result);
    browser::detach(b, page).await?;
    Ok(())
}

async fn after_action(page: &Page) -> anyhow::Result<ActionResult> {
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
