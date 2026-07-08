use crate::{commands, state};

/// jq filter turning claude's raw stream-json NDJSON into readable lines:
/// 🤔 thinking, → tool calls (with args), ← tool results, 💬 assistant text,
/// ✅ final result. Long strings/blobs are truncated so image base64 data
/// doesn't flood the terminal.
const LOG_FILTER: &str = r#"
def trunc: if (type == "string" and length > 300) then .[0:300] + "…" else . end;
if .type == "assistant" then
  (.message.content[]? |
    if .type == "thinking" and (.thinking | length) > 0 then "🤔 " + (.thinking | trunc)
    elif .type == "tool_use" then "→ " + .name + " " + (.input | tostring | trunc)
    elif .type == "text" then "💬 " + .text
    else empty end)
elif .type == "user" then
  (.message.content[]? |
    if .type == "tool_result" then
      "  ← " + (if (.content | type) == "array" then "[image]" else (.content | tostring | trunc) end)
    else empty end)
elif .type == "result" then
  "\n✅ " + .result
else empty end
"#;

/// Ensures a browser session, runs `claude -p` on it, then tears the session down.
pub async fn cmd_run(query: &str) -> anyhow::Result<()> {
    if !state::session_exists() {
        commands::cmd_open(None).await?;
    }
    let mut claude = std::process::Command::new("claude")
        .arg("-p")
        .arg(query)
        .arg("--allowedTools")
        .arg("Bash,Read")
        .arg("--permission-mode")
        .arg("acceptEdits")
        // Emits one NDJSON event per step (thinking, each tool call incl.
        // the exact `cua` command, tool results, final text) instead of just
        // the final answer — this is what surfaces "what agent thinks and
        // does". Plain --verbose with the default text format shows nothing
        // extra; stream-json requires --verbose to be set.
        .arg("--output-format")
        .arg("stream-json")
        .arg("--verbose")
        .stdout(std::process::Stdio::piped())
        .spawn()?;

    // Pipe claude's raw NDJSON through jq for human-readable output instead
    // of dumping the raw event stream.
    let jq_status = std::process::Command::new("jq")
        .args(["-r", LOG_FILTER])
        .stdin(std::process::Stdio::from(claude.stdout.take().unwrap()))
        .status();

    let claude_status = claude.wait();
    let close_result = commands::cmd_close().await;

    let status = claude_status?;
    let _ = jq_status;
    close_result?;
    if !status.success() {
        anyhow::bail!("claude exited with status {status}");
    }
    Ok(())
}
