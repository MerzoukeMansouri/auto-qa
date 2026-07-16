use crate::state;

/// MCP server config for `claude -p`: launches Playwright's own MCP server
/// (via `npx`), which drives and owns its own browser instance.
/// `--save-session` + `--codegen typescript` (the default) makes Playwright
/// MCP itself write a replayable .spec.ts of the run to `--output-dir` —
/// that's the source for `cua codegen` post-run instead of our actions.json,
/// which MCP-driven runs never populate.
fn mcp_config() -> String {
    let out_dir = state::runtime_dir().join("pw-session");
    serde_json::json!({
        "mcpServers": {
            "playwright": {
                "command": "npx",
                "args": [
                    "@playwright/mcp@latest",
                    "--image-responses",
                    "omit",
                    "--save-session",
                    "--output-dir",
                    out_dir.display().to_string()
                ]
            }
        }
    })
    .to_string()
}

/// Belt-and-suspenders: --allowedTools mcp__playwright already blocks other
/// tools, but the model has still been observed answering from training
/// knowledge instead of acting when a task looks answerable without a
/// browser. Force it to actually drive the page.
const SYSTEM_PROMPT: &str = "You must complete this task by driving a real browser through the playwright MCP tools (mcp__playwright__*). Do not answer from memory or prior knowledge — navigate, click, type, and read the actual rendered page for every fact you report. This session is being recorded to generate a replayable Playwright test afterward: perform only the actions strictly necessary to complete the task, in a clean, deliberate, linear sequence — no exploratory clicks, no backtracking, no dead-end navigation you don't use.";

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

/// Runs `claude -p` wired to the Playwright MCP server.
pub async fn cmd_run(query: &str) -> anyhow::Result<()> {
    let mut claude = std::process::Command::new("claude")
        .arg("-p")
        .arg(query)
        .arg("--mcp-config")
        .arg(mcp_config())
        .arg("--strict-mcp-config")
        .arg("--append-system-prompt")
        .arg(SYSTEM_PROMPT)
        .arg("--allowedTools")
        .arg("mcp__playwright")
        .arg("--permission-mode")
        .arg("acceptEdits")
        // Emits one NDJSON event per step (thinking, each tool call incl.
        // the exact Playwright MCP call, tool results, final text) instead of
        // just the final answer — this is what surfaces "what agent thinks
        // and does". Plain --verbose with the default text format shows
        // nothing extra; stream-json requires --verbose to be set.
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

    let status = claude.wait()?;
    let _ = jq_status;
    if !status.success() {
        anyhow::bail!("claude exited with status {status}");
    }
    Ok(())
}
