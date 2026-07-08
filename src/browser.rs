use chromiumoxide::cdp::browser_protocol::page::CaptureScreenshotFormat;
use chromiumoxide::{Browser, Page};
use futures::StreamExt;
use std::path::Path;
use std::time::Duration;

/// Connect to an already-running Chrome at the given CDP HTTP endpoint.
/// Chrome itself holds all session state across separate `cu` invocations —
/// this attaches to it rather than launching a new managed browser.
pub async fn connect(port: u16) -> anyhow::Result<Browser> {
    let (browser, mut handler) = Browser::connect(format!("http://localhost:{port}")).await?;
    // chromiumoxide requires the handler event loop polled continuously in the
    // background for the Browser handle to function at all — unpolled, calls hang forever.
    tokio::spawn(async move { while handler.next().await.is_some() {} });
    Ok(browser)
}

pub async fn get_active_page(browser: &Browser, target_id: Option<&str>) -> anyhow::Result<Page> {
    // Target discovery arrives asynchronously over the handler's event stream
    // right after connect() — retry briefly rather than assuming it's already landed.
    let mut pages = Vec::new();
    for _ in 0..25 {
        pages = browser.pages().await?;
        if !pages.is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    if let Some(tid) = target_id {
        if let Some(p) = pages.iter().find(|p| p.target_id().inner() == tid) {
            return Ok(p.clone());
        }
    }
    pages
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("no open pages — run 'cua open' first"))
}

/// JPEG at moderate quality instead of lossless PNG — screenshots are mostly
/// flat UI/text, which compresses well, and this cuts file size + the vision
/// model's per-image ingest time without hurting coordinate-picking accuracy.
pub async fn take_screenshot(page: &Page, out_path: &Path) -> anyhow::Result<()> {
    let params = chromiumoxide::page::ScreenshotParams::builder()
        .format(CaptureScreenshotFormat::Jpeg)
        .quality(75)
        .full_page(false)
        .build();
    page.save_screenshot(params, out_path).await?;
    Ok(())
}

/// Pins the viewport to a fixed, moderate size right after a page is created.
/// Vision-model token cost scales with image pixel dimensions, not file size —
/// this bounds it regardless of the host display's actual resolution.
pub async fn set_viewport(page: &Page, width: u32, height: u32) -> anyhow::Result<()> {
    use chromiumoxide::cdp::browser_protocol::emulation::SetDeviceMetricsOverrideParams;
    page.execute(
        SetDeviceMetricsOverrideParams::builder()
            .width(width as i64)
            .height(height as i64)
            .device_scale_factor(1.0)
            .mobile(false)
            .build()
            .map_err(|e| anyhow::anyhow!(e))?,
    )
    .await?;
    Ok(())
}

/// Sends an explicit `Target.detachFromTarget` for this page's session before
/// the process exits. Without this, the process just drops the WebSocket
/// abruptly, and Chrome appears to (inconsistently, after a few such abrupt
/// disconnects accumulate) reset or fully close tabs whose attached debugger
/// session vanished without a clean detach — observed as tabs reverting to
/// chrome://newtab/ or disappearing entirely between separate `cua` invocations.
pub async fn detach(browser: &Browser, page: &Page) -> anyhow::Result<()> {
    use chromiumoxide::cdp::browser_protocol::target::DetachFromTargetParams;
    let _ = browser
        .execute(DetachFromTargetParams::builder().session_id(page.session_id().clone()).build())
        .await;
    Ok(())
}

/// Reference's `PlaywrightComputer._handle_new_page`: the agent only ever
/// sees one tab, so if an action (e.g. a link with target=_blank) opened a
/// new one, fold it back into the tracked tab and close the extra.
pub async fn enforce_single_tab(browser: &Browser, primary: &Page) -> anyhow::Result<()> {
    let pages = browser.pages().await?;
    let primary_id = primary.target_id().inner().to_string();
    for p in pages {
        if *p.target_id().inner() != primary_id {
            if let Ok(Some(url)) = p.url().await {
                if !url.is_empty() && url != "about:blank" {
                    let _ = primary.goto(url).await;
                }
            }
            let _ = p.close().await;
        }
    }
    Ok(())
}

/// Closes every page except `keep` — used right after 'cua open' to get rid
/// of Chrome's own default New Tab Page tab that appears on a fresh launch,
/// so later `enforce_single_tab` calls only ever see tabs the agent itself opened.
pub async fn close_other_pages(browser: &Browser, keep: &Page) -> anyhow::Result<()> {
    let keep_id = keep.target_id().inner().to_string();
    for p in browser.pages().await? {
        if *p.target_id().inner() != keep_id {
            let _ = p.close().await;
        }
    }
    Ok(())
}

fn find_chrome_binary() -> anyhow::Result<std::path::PathBuf> {
    if let Ok(p) = std::env::var("CHROME_PATH") {
        return Ok(p.into());
    }
    let candidates = [
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/usr/bin/google-chrome",
        "/usr/bin/chromium",
        "/usr/bin/chromium-browser",
    ];
    for c in candidates {
        if Path::new(c).exists() {
            return Ok(c.into());
        }
    }
    anyhow::bail!("Chrome not found — set CHROME_PATH to the binary")
}

/// Launches a detached Chrome with a fresh profile, listening on `port` for CDP.
/// Returns the process id.
pub fn launch_chrome(port: u16, user_data_dir: &Path) -> anyhow::Result<u32> {
    let chrome = find_chrome_binary()?;
    let child = std::process::Command::new(chrome)
        .arg(format!("--remote-debugging-port={port}"))
        .arg(format!("--user-data-dir={}", user_data_dir.display()))
        .arg("--no-first-run")
        .arg("--no-default-browser-check")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    Ok(child.id())
}

/// Polls the CDP HTTP endpoint (via `curl`, not a chromiumoxide connect —
/// see `create_page_via_http` for why no WebSocket session touches page
/// creation) until Chrome is ready to accept requests.
pub async fn wait_for_http_ready(port: u16, timeout: Duration) -> anyhow::Result<()> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        let ok = std::process::Command::new("curl")
            .args(["-s", "-o", "/dev/null", "-w", "%{http_code}", &format!("http://localhost:{port}/json/version")])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "200")
            .unwrap_or(false);
        if ok {
            return Ok(());
        }
        if std::time::Instant::now() > deadline {
            anyhow::bail!("Chrome did not become ready on port {port} in time");
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

/// Creates a new tab via the plain HTTP /json/new endpoint and returns its
/// target id. Deliberately NOT chromiumoxide's `Browser::new_page()`, which
/// creates the tab over the WebSocket CDP session: Chrome resets that tab's
/// content back to chrome://newtab/ once the session that created it fully
/// disconnects — which happens the instant this short-lived `cua` process
/// exits. A tab created over plain HTTP has no such owning session and its
/// content persists across separate `cua` invocations, same as a tab a human
/// opened by hand.
pub fn create_page_via_http(port: u16, url: &str) -> anyhow::Result<String> {
    let output = std::process::Command::new("curl")
        .args(["-s", "-X", "PUT", &format!("http://localhost:{port}/json/new?{url}")])
        .output()?;
    let json: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    json["id"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| anyhow::anyhow!("no target id in /json/new response: {}", String::from_utf8_lossy(&output.stdout)))
}
