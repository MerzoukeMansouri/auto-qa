use crate::{action_entry::ActionEntry, playwright_codegen, state};
use axum::{
    body::Body, http::StatusCode, response::IntoResponse, routing::get, Json, Router,
};
use rust_embed::RustEmbed;

/// The built React review UI (`web/dist`), baked into the binary at compile
/// time — a Homebrew-shipped `cua` binary has no `web/` directory alongside
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
    // is often left running across multiple `cua run` sessions, and a
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

async fn post_validate() -> impl IntoResponse {
    let entries = state::read_actions();
    let ts = playwright_codegen::generate(&entries);
    let out = "cua-generated.spec.ts";
    match std::fs::write(out, &ts) {
        Ok(()) => Json(serde_json::json!({"path": out, "contents": ts})).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
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
        .fallback(|uri: axum::http::Uri| async move { serve_asset(uri.path()).await });

    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    let url = format!("http://127.0.0.1:{port}");
    println!("cua review UI: {url}");
    open_browser(&url);

    axum::serve(listener, app).await?;
    Ok(())
}
