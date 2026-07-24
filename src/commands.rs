use crate::state;

/// Pure file transform, no browser session needed — reads actions.json
/// (importing the latest `autoqa run` MCP session into it first, unless it was
/// already hand-edited more recently via `autoqa review`), writes a Playwright
/// .spec.ts. Kept separate from `autoqa review` for scripting/CI use.
pub async fn cmd_codegen(out: &str) -> anyhow::Result<()> {
    state::sync_actions_from_latest_mcp_session()?;
    let title =
        state::latest_query().unwrap_or_else(|| "generated from autoqa session".to_string());
    let ts = crate::playwright_codegen::generate(&state::read_actions(), &title);
    if let Some(parent) = std::path::Path::new(out).parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(out, ts)?;
    println!("wrote {out}");
    Ok(())
}
