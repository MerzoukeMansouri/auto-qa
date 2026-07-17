use crate::action_entry::ActionEntry;
use std::path::PathBuf;

pub fn runtime_dir() -> PathBuf {
    let home = std::env::var("HOME").expect("HOME not set");
    PathBuf::from(home).join(".cu-agent")
}

pub fn actions_path() -> PathBuf {
    runtime_dir().join("actions.json")
}

/// `session.md` from the most recent `cua run` (dirs are named
/// `session-<unix_ms>`, so a plain lexical max gives the latest).
pub fn latest_mcp_session_md() -> Option<PathBuf> {
    let dir = runtime_dir().join("pw-session");
    let latest = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .max_by_key(|p| p.file_name().map(|n| n.to_os_string()))?;
    let md = latest.join("session.md");
    md.is_file().then_some(md)
}

/// The `--query` text from the most recent `cua run`, used to title the
/// generated test instead of a generic placeholder.
pub fn latest_query() -> Option<String> {
    std::fs::read_to_string(runtime_dir().join("last-query.txt")).ok()
}

pub fn read_actions() -> Vec<ActionEntry> {
    std::fs::read_to_string(actions_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn write_actions(entries: &[ActionEntry]) -> anyhow::Result<()> {
    std::fs::create_dir_all(runtime_dir())?;
    let json = serde_json::to_string_pretty(entries)?;
    std::fs::write(actions_path(), json)?;
    Ok(())
}

/// Refreshes actions.json from the latest `cua run` (MCP) session, unless
/// actions.json was touched more recently — e.g. by hand-editing in the
/// review UI, which should win over re-importing the raw session.
pub fn sync_actions_from_latest_mcp_session() -> anyhow::Result<()> {
    let Some(session_md) = latest_mcp_session_md() else {
        return Ok(());
    };
    let session_modified = std::fs::metadata(&session_md)?.modified()?;
    if let Ok(actions_modified) = std::fs::metadata(actions_path()).and_then(|m| m.modified()) {
        if actions_modified >= session_modified {
            return Ok(());
        }
    }
    let md = std::fs::read_to_string(session_md)?;
    write_actions(&crate::playwright_codegen::parse_mcp_session(&md))
}
