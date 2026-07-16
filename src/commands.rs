use crate::state;

/// Pure file transform, no browser session needed — reads actions.json
/// (importing the latest `cua run` MCP session into it first, unless it was
/// already hand-edited more recently via `cua review`), writes a Playwright
/// .spec.ts. Kept separate from `cua review` for scripting/CI use.
pub async fn cmd_codegen(out: &str) -> anyhow::Result<()> {
    state::sync_actions_from_latest_mcp_session()?;
    let ts = crate::playwright_codegen::generate(&state::read_actions());
    std::fs::write(out, ts)?;
    println!("wrote {out}");
    Ok(())
}
