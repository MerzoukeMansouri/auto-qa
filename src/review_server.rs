use crate::{action_entry::ActionEntry, agent, playwright_codegen, state};
use axum::{
    body::Body, extract::Path, http::StatusCode, response::IntoResponse, routing::get, Json, Router,
};
use rust_embed::RustEmbed;

/// The built React review UI (`web/dist`), baked into the binary at compile
/// time — a Homebrew-shipped `autoqa` binary has no `web/` directory alongside
/// it at runtime, so this must be self-contained rather than served from disk.
#[derive(RustEmbed)]
#[folder = "web/dist/"]
struct Assets;

async fn serve_asset(path: &str) -> impl IntoResponse {
    let path = path.trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };
    match Assets::get(path) {
        Some(file) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            (
                [("content-type", mime.as_ref().to_string())],
                Body::from(file.data),
            )
                .into_response()
        }
        // SPA fallback: unknown paths (client-side routes, if any get added)
        // resolve to index.html rather than 404.
        None => match Assets::get("index.html") {
            Some(file) => (
                [("content-type", "text/html".to_string())],
                Body::from(file.data),
            )
                .into_response(),
            None => StatusCode::NOT_FOUND.into_response(),
        },
    }
}

async fn get_actions() -> Json<Vec<ActionEntry>> {
    // Re-sync on every load, not just at server startup — the review server
    // is often left running across multiple `autoqa run` sessions, and a
    // startup-only sync would keep serving whatever was captured first.
    let _ = state::sync_actions_from_latest_mcp_session();
    Json(state::read_actions())
}

async fn put_actions(Json(entries): Json<Vec<ActionEntry>>) -> impl IntoResponse {
    match state::write_actions(&entries) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// Bootstraps `~/.auto-qa/playwright-tests` on first use — a fresh
/// Homebrew install has no npm project there yet, and `autoqa review` must
/// work from any cwd, not just a directory someone happened to `npm init`
/// in by hand. Cheap no-op on every call after the first (guarded by
/// `node_modules/@playwright/test` already existing).
async fn ensure_playwright_tests_stack() -> anyhow::Result<()> {
    let dir = state::playwright_tests_dir();
    if dir.join("node_modules/@playwright/test").is_dir() {
        return Ok(());
    }
    std::fs::create_dir_all(&dir)?;

    if !dir.join("package.json").is_file() {
        let status = tokio::process::Command::new("npm")
            .args(["init", "-y"])
            .current_dir(&dir)
            .status()
            .await?;
        anyhow::ensure!(status.success(), "npm init failed");
    }

    let status = tokio::process::Command::new("npm")
        .args(["install", "-D", "@playwright/test"])
        .current_dir(&dir)
        .status()
        .await?;
    anyhow::ensure!(status.success(), "npm install @playwright/test failed");

    let status = tokio::process::Command::new("npx")
        .args(["playwright", "install", "chromium"])
        .current_dir(&dir)
        .status()
        .await?;
    anyhow::ensure!(status.success(), "playwright install chromium failed");

    let config = dir.join("playwright.config.ts");
    if !config.is_file() {
        std::fs::write(
            config,
            "import { defineConfig } from '@playwright/test';\n\n\
             export default defineConfig({\n  use: {\n    headless: false,\n  },\n});\n",
        )?;
    }
    Ok(())
}

/// Regenerates `autoqa-generated.spec.ts` from the current actions.json —
/// shared by both `/api/validate` (write only) and `/api/run` (write + execute).
fn write_generated_spec() -> anyhow::Result<(std::path::PathBuf, String)> {
    let entries = state::read_actions();
    let title =
        state::latest_query().unwrap_or_else(|| "generated from autoqa session".to_string());
    let ts = playwright_codegen::generate(&entries, &title);
    let dir = state::playwright_tests_dir();
    std::fs::create_dir_all(&dir)?;
    let out = dir.join("autoqa-generated.spec.ts");
    std::fs::write(&out, &ts)?;
    Ok((out, ts))
}

async fn post_validate() -> impl IntoResponse {
    match write_generated_spec() {
        Ok((out, ts)) => {
            Json(serde_json::json!({"path": out.display().to_string(), "contents": ts}))
                .into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// Regenerates the spec, then actually runs it with `npx playwright test` so
/// "Generate" isn't the only signal — a generated test that never gets
/// executed can silently rot as the app under test changes.
async fn post_run() -> impl IntoResponse {
    if let Err(e) = write_generated_spec() {
        return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
    }
    if let Err(e) = ensure_playwright_tests_stack().await {
        return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
    }

    let output = tokio::process::Command::new("npx")
        .args(["playwright", "test", "autoqa-generated.spec.ts"])
        .current_dir(state::playwright_tests_dir())
        .output()
        .await;

    match output {
        Ok(out) => {
            let mut log = String::from_utf8_lossy(&out.stdout).into_owned();
            log.push_str(&String::from_utf8_lossy(&out.stderr));
            Json(serde_json::json!({"passed": out.status.success(), "output": log})).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// Truncates the test at `index` (inclusive), appends `page.pause()`, and
/// launches it headed with the Inspector forced open (`PWDEBUG=1`) — lets
/// the developer poke the real DOM at that exact step instead of guessing a
/// selector from a static snapshot. Fire-and-forget: the Inspector window
/// stays open until the developer closes it, so this doesn't wait for the
/// process to exit, just for it to launch.
async fn post_pause(Path(index): Path<usize>) -> impl IntoResponse {
    let entries = state::read_actions();
    if index >= entries.len() {
        return (StatusCode::BAD_REQUEST, "index out of range").into_response();
    }
    let title =
        state::latest_query().unwrap_or_else(|| "generated from autoqa session".to_string());
    let ts = playwright_codegen::generate_up_to_with_pause(&entries, index, &title);

    if let Err(e) = ensure_playwright_tests_stack().await {
        return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
    }
    let dir = state::playwright_tests_dir();
    let out = dir.join(".autoqa-pause.spec.ts");
    if let Err(e) = std::fs::write(&out, &ts) {
        return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
    }

    let spawned = tokio::process::Command::new("npx")
        .args(["playwright", "test", ".autoqa-pause.spec.ts", "--headed"])
        .env("PWDEBUG", "1")
        .current_dir(&dir)
        .spawn();

    match spawned {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[derive(serde::Deserialize)]
struct ChatRequest {
    instruction: String,
}

/// Edits the step list via a natural-language instruction sent to `claude -p`
/// — no browser/MCP involved, purely a JSON-in/JSON-out text task. Only
/// persists on a successful parse, so a malformed model response can never
/// corrupt actions.json.
async fn post_chat(Json(req): Json<ChatRequest>) -> impl IntoResponse {
    let current = state::read_actions();
    match agent::edit_actions_via_chat(&current, &req.instruction).await {
        Ok(updated) => {
            if let Err(e) = state::write_actions(&updated) {
                return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
            }
            Json(updated).into_response()
        }
        Err(e) => (StatusCode::UNPROCESSABLE_ENTITY, e.to_string()).into_response(),
    }
}

fn open_browser(url: &str) {
    let cmd = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let _ = std::process::Command::new(cmd).arg(url).spawn();
}

pub async fn serve(port: u16) -> anyhow::Result<()> {
    let app = Router::new()
        .route("/api/actions", get(get_actions).put(put_actions))
        .route("/api/validate", axum::routing::post(post_validate))
        .route("/api/run", axum::routing::post(post_run))
        .route("/api/pause/:index", axum::routing::post(post_pause))
        .route("/api/chat", axum::routing::post(post_chat))
        .fallback(|uri: axum::http::Uri| async move { serve_asset(uri.path()).await });

    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    let url = format!("http://127.0.0.1:{port}");
    println!("autoqa review UI: {url}");
    open_browser(&url);

    axum::serve(listener, app).await?;
    Ok(())
}
