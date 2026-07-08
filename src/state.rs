use std::path::PathBuf;

fn runtime_dir() -> PathBuf {
    let home = std::env::var("HOME").expect("HOME not set");
    PathBuf::from(home).join(".cu-agent")
}

fn file(name: &str) -> PathBuf {
    runtime_dir().join(name)
}

pub fn ensure_runtime_dir() -> anyhow::Result<()> {
    std::fs::create_dir_all(runtime_dir())?;
    Ok(())
}

pub fn screenshot_path() -> PathBuf {
    file("screenshot.jpg")
}

pub fn write(name: &str, contents: &str) -> anyhow::Result<()> {
    ensure_runtime_dir()?;
    std::fs::write(file(name), contents)?;
    Ok(())
}

pub fn read(name: &str) -> Option<String> {
    std::fs::read_to_string(file(name)).ok().map(|s| s.trim().to_string())
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

/// Appends one line to ~/.cu-agent/log.jsonl — a record of every action `cua`
/// actually executed, so a session can be replayed/audited after the fact.
pub fn append_log(action: &str, result_url: &str) {
    use std::io::Write;
    let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(file("log.jsonl")) else {
        return;
    };
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let line = serde_json::json!({"ts": ts, "action": action, "url": result_url});
    let _ = writeln!(f, "{line}");
}

pub fn clear() {
    let dir = runtime_dir();
    let _ = std::fs::remove_dir_all(&dir);
}
