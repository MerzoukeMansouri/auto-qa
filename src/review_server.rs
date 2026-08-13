use crate::{
    agent,
    block::{Block, Param, Test, TestStep},
    harness::Harness,
    playwright_codegen, state,
};
use axum::{
    body::Body, extract::Path, extract::State, http::StatusCode, response::IntoResponse,
    routing::get, Json, Router,
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

async fn get_actions() -> Json<Vec<TestStep>> {
    // Re-sync on every load, not just at server startup — the review server
    // is often left running across multiple `autoqa run` sessions, and a
    // startup-only sync would keep serving whatever was captured first.
    let _ = state::sync_actions_from_latest_mcp_session();
    Json(state::read_actions())
}

async fn put_actions(Json(entries): Json<Vec<TestStep>>) -> impl IntoResponse {
    match state::write_actions(&entries) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn get_blocks() -> Json<Vec<(String, Block)>> {
    Json(state::list_blocks().unwrap_or_default())
}

async fn put_block(Path(slug): Path<String>, Json(block): Json<Block>) -> impl IntoResponse {
    match state::write_block(&slug, &block) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn delete_block(Path(slug): Path<String>) -> impl IntoResponse {
    match state::delete_block(&slug) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn get_tests() -> Json<Vec<(String, Test)>> {
    Json(state::list_tests().unwrap_or_default())
}

/// Loads a saved test into the current working buffer (`actions.json`) —
/// same one `/api/validate`/`/api/run`/`/api/pause` already operate on —
/// and returns its steps, so the client's editor state and the server's
/// buffer end up in sync in one round trip.
async fn open_test(Path(slug): Path<String>) -> impl IntoResponse {
    let test = match state::read_test(&slug) {
        Ok(t) => t,
        Err(e) => return (StatusCode::NOT_FOUND, e.to_string()).into_response(),
    };
    match state::write_actions(&test.steps) {
        Ok(()) => Json(test.steps).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// Loads the latest `autoqa run` session into the working buffer on
/// demand — unlike `get_actions`'s auto-sync, this ignores the staleness
/// guard, so it works even if the buffer was hand-edited more recently.
async fn open_last_run() -> impl IntoResponse {
    match state::load_latest_mcp_session() {
        Ok(steps) => Json(steps).into_response(),
        Err(e) => (StatusCode::NOT_FOUND, e.to_string()).into_response(),
    }
}

async fn put_test(Path(slug): Path<String>, Json(test): Json<Test>) -> impl IntoResponse {
    match state::write_test(&slug, &test) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn delete_test(Path(slug): Path<String>) -> impl IntoResponse {
    match state::delete_test(&slug) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn get_params() -> Json<Vec<Param>> {
    Json(state::read_params())
}

async fn put_params(Json(entries): Json<Vec<Param>>) -> impl IntoResponse {
    match state::write_params(&entries) {
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
        .args([
            "install",
            "-D",
            "@playwright/test",
            "--registry",
            state::NPM_PUBLIC_REGISTRY,
        ])
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
    let ts = playwright_codegen::generate(&entries, &title)?;
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
    let ts = match playwright_codegen::generate_up_to_with_pause(&entries, index, &title) {
        Ok(ts) => ts,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };

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

/// Edits the step list via a natural-language instruction sent to the
/// selected harness — no browser/MCP involved, purely a JSON-in/JSON-out
/// text task. Only persists on a successful parse, so a malformed model
/// response can never corrupt actions.json.
async fn post_chat(
    State(state): State<AppState>,
    Json(req): Json<ChatRequest>,
) -> impl IntoResponse {
    let current = state::read_actions();
    match agent::edit_actions_via_chat(
        state.harness,
        &current,
        &req.instruction,
        state.model.as_deref(),
    )
    .await
    {
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

/// Router state — the harness/model chosen for this `autoqa review`
/// invocation, shared read-only across every request handler.
#[derive(Clone)]
struct AppState {
    harness: Harness,
    model: Option<String>,
}

pub async fn serve(port: u16, harness: Harness, model: Option<String>) -> anyhow::Result<()> {
    let app = Router::new()
        .route("/api/actions", get(get_actions).put(put_actions))
        .route("/api/validate", axum::routing::post(post_validate))
        .route("/api/run", axum::routing::post(post_run))
        .route("/api/pause/:index", axum::routing::post(post_pause))
        .route("/api/chat", axum::routing::post(post_chat))
        .route("/api/blocks", get(get_blocks))
        .route(
            "/api/blocks/:slug",
            axum::routing::put(put_block).delete(delete_block),
        )
        .route("/api/params", get(get_params).put(put_params))
        .route("/api/tests", get(get_tests))
        .route(
            "/api/tests/:slug",
            axum::routing::put(put_test).delete(delete_test),
        )
        .route("/api/tests/:slug/open", axum::routing::post(open_test))
        .route("/api/last-run/open", axum::routing::post(open_last_run))
        .fallback(|uri: axum::http::Uri| async move { serve_asset(uri.path()).await })
        .with_state(AppState { harness, model });

    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    let url = format!("http://127.0.0.1:{port}");
    println!("autoqa review UI: {url}");
    open_browser(&url);

    axum::serve(listener, app).await?;
    Ok(())
}
