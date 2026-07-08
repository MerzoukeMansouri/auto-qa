use crate::action_entry::ActionEntry;
use std::path::PathBuf;

pub fn runtime_dir() -> PathBuf {
    let home = std::env::var("HOME").expect("HOME not set");
    PathBuf::from(home).join(".cu-agent")
}

fn file(name: &str) -> PathBuf {
    runtime_dir().join(name)
}

pub fn ensure_runtime_dir() -> anyhow::Result<()> {
    std::fs::create_dir_all(runtime_dir())?;
    std::fs::create_dir_all(runtime_dir().join("screenshots"))?;
    Ok(())
}

pub fn screenshot_path() -> PathBuf {
    file("screenshot.jpg")
}

/// Per-action screenshot path — `n` is the action's index in actions.json.
pub fn screenshot_path_for(n: usize) -> PathBuf {
    runtime_dir().join("screenshots").join(format!("{n}.jpg"))
}

pub fn actions_path() -> PathBuf {
    file("actions.json")
}

pub fn read_actions() -> Vec<ActionEntry> {
    std::fs::read_to_string(actions_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn write_actions(entries: &[ActionEntry]) -> anyhow::Result<()> {
    ensure_runtime_dir()?;
    let json = serde_json::to_string_pretty(entries)?;
    std::fs::write(actions_path(), json)?;
    Ok(())
}

/// Appends one action entry to actions.json — the structured, editable
/// record a session's `cua review` UI and `cua codegen` both read from.
pub fn append_action(entry: &ActionEntry) -> anyhow::Result<()> {
    let mut entries = read_actions();
    entries.push(entry.clone());
    write_actions(&entries)
}

pub fn write(name: &str, contents: &str) -> anyhow::Result<()> {
    ensure_runtime_dir()?;
    std::fs::write(file(name), contents)?;
    Ok(())
}

pub fn read(name: &str) -> Option<String> {
    std::fs::read_to_string(file(name))
        .ok()
        .map(|s| s.trim().to_string())
}

pub fn port() -> u16 {
    read("port").and_then(|p| p.parse().ok()).unwrap_or(9222)
}

pub fn target_id() -> Option<String> {
    read("target-id")
}

pub fn set_target_id(id: &str) -> anyhow::Result<()> {
    write("target-id", id)
}

pub fn pid() -> Option<u32> {
    read("chrome.pid").and_then(|p| p.parse().ok())
}

/// True if a chrome.pid file exists and that process is still alive.
pub fn session_exists() -> bool {
    let Some(pid) = pid() else { return false };
    // `kill -0` checks liveness without sending a signal.
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Wipes everything, including captured actions.json/screenshots — only for
/// `cua open` starting a genuinely new session, never for `cua close`.
pub fn clear() {
    let dir = runtime_dir();
    let _ = std::fs::remove_dir_all(&dir);
}

/// Tears down just the Chrome session bookkeeping (pid/port/target-id) so
/// `session_exists()` goes false — leaves actions.json/screenshots intact so
/// `cua review`/`cua codegen` still have something to read after `cua close`
/// (including the auto-close at the end of `cua run`).
pub fn clear_session() {
    for name in ["chrome.pid", "port", "target-id", "user-data-dir"] {
        let _ = std::fs::remove_file(file(name));
    }
}
