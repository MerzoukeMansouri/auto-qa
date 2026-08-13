use crate::state;
use std::path::PathBuf;

/// Fixed set of supported CLI harnesses. A `match` per concern (not a trait
/// object) since there's a known, fixed set of variants and no runtime
/// plugin loading requirement — the compiler flags a missing arm if a new
/// one is added.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Default,
    clap::ValueEnum,
    serde::Serialize,
    serde::Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum Harness {
    #[default]
    Claude,
    ClaudeSdk,
    Copilot,
    Opencode,
    Codex,
    Gemini,
    GeminiSdk,
}

pub const ALL: &[Harness] = &[
    Harness::Claude,
    Harness::ClaudeSdk,
    Harness::Copilot,
    Harness::Opencode,
    Harness::Codex,
    Harness::Gemini,
    Harness::GeminiSdk,
];

impl std::fmt::Display for Harness {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Harness::Claude => "claude",
            Harness::ClaudeSdk => "claude-sdk",
            Harness::Copilot => "copilot",
            Harness::Opencode => "opencode",
            Harness::Codex => "codex",
            Harness::Gemini => "gemini",
            Harness::GeminiSdk => "gemini-sdk",
        };
        f.write_str(name)
    }
}

/// One MCP server definition, harness-agnostic — the Playwright-specific
/// command/args stay owned by agent.rs, this struct just carries them
/// through to whichever config format a given harness needs.
pub struct McpServerSpec {
    pub name: &'static str,
    pub command: &'static str,
    pub args: Vec<String>,
}

/// jq filter turning claude's raw stream-json NDJSON into readable lines:
/// 🤔 thinking, → tool calls (with args), ← tool results, 💬 assistant text,
/// ✅ final result. Long strings/blobs are truncated so image base64 data
/// doesn't flood the terminal.
const CLAUDE_LOG_FILTER: &str = r#"
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

/// Best-effort filter for opencode's `--format json` stream. Only 3 fields
/// are confirmed against real output (a bug report, not official docs):
/// step_start/text/step_finish. step_finish can be dropped entirely if the
/// run loop exits early (sst/opencode#26855) — a run with no ✅ line is
/// normal for this harness, not a sign the filter is broken.
const OPENCODE_LOG_FILTER: &str = r#"
if .type == "text" then (.part.text // empty)
elif .type == "step_finish" then "\n✅ " + (.part.reason // "done")
else empty end
"#;

/// Defensive filter for gemini's `-o stream-json`. Only the top-level
/// `.type` enum (message/tool_use/tool_result/result/error) is confirmed by
/// official docs — nested field names below are best-guess, verify against
/// real `gemini -o stream-json` output before trusting this in a demo.
const GEMINI_LOG_FILTER: &str = r#"
if .type == "message" then (.text // .message // empty)
elif .type == "tool_use" then "→ " + (.name // "tool")
elif .type == "tool_result" then "  ← " + (.result // empty | tostring)
elif .type == "result" then "\n✅ " + (.result // empty | tostring)
elif .type == "error" then "\n❌ " + (.message // . | tostring)
else empty end
"#;

/// Scratch dir for a harness's ad hoc per-run config artifacts (MCP config
/// file, system-prompt file, or a full scratch config dir for $CODEX_HOME /
/// --dir). One subdir per harness+mode so concurrent/successive runs never
/// read stale config left by a different call site.
fn scratch_dir(name: &str) -> PathBuf {
    state::runtime_dir().join("harness-config").join(name)
}

/// Codex reads auth from `$CODEX_HOME/auth.json`. Since we point CODEX_HOME
/// at a scratch dir, copy the real login (ChatGPT or API key) in — otherwise
/// codex sees no auth and fails even when the user is logged in.
fn copy_codex_auth(dir: &std::path::Path) {
    if let Ok(home) = std::env::var("HOME") {
        let auth = PathBuf::from(home).join(".codex").join("auth.json");
        let _ = std::fs::copy(auth, dir.join("auth.json"));
    }
}

/// `node/gemini-sdk` (own harness against the Gemini API directly, no
/// `gemini` CLI subprocess) is embedded via `include_str!` — same reasoning
/// as `block_server_mcp_spec` in agent.rs: a Homebrew install has no `node/`
/// dir alongside the binary, so the script has to be written out at runtime.
/// `npm install` only runs once (skipped whenever `node_modules` already
/// exists), so this stays cheap on every call after the first.
fn ensure_gemini_sdk_script() -> anyhow::Result<PathBuf> {
    let dir = state::runtime_dir().join("gemini-sdk");
    std::fs::create_dir_all(&dir)?;
    std::fs::write(
        dir.join("package.json"),
        include_str!("../node/gemini-sdk/package.json"),
    )?;
    let script_path = dir.join("index.mjs");
    std::fs::write(&script_path, include_str!("../node/gemini-sdk/index.mjs"))?;

    if !dir.join("node_modules").is_dir() {
        let status = std::process::Command::new("npm")
            .args(["install"])
            .current_dir(&dir)
            .status()?;
        anyhow::ensure!(
            status.success(),
            "npm install for gemini-sdk harness failed"
        );
    }
    Ok(script_path)
}

/// `node/claude-sdk` (own harness against the Claude Messages API directly —
/// no `claude` CLI subprocess, and no Claude Agent SDK either, since that
/// bundles and spawns the `claude-code` binary internally) is embedded via
/// `include_str!`, same reasoning as `ensure_gemini_sdk_script`.
fn ensure_claude_sdk_script() -> anyhow::Result<PathBuf> {
    let dir = state::runtime_dir().join("claude-sdk");
    std::fs::create_dir_all(&dir)?;
    std::fs::write(
        dir.join("package.json"),
        include_str!("../node/claude-sdk/package.json"),
    )?;
    let script_path = dir.join("index.mjs");
    std::fs::write(&script_path, include_str!("../node/claude-sdk/index.mjs"))?;

    if !dir.join("node_modules").is_dir() {
        let status = std::process::Command::new("npm")
            .args(["install"])
            .current_dir(&dir)
            .status()?;
        anyhow::ensure!(
            status.success(),
            "npm install for claude-sdk harness failed"
        );
    }
    Ok(script_path)
}

fn mcp_config_json(servers: &[McpServerSpec]) -> serde_json::Value {
    let mut mcp_servers = serde_json::Map::new();
    for mcp in servers {
        mcp_servers.insert(
            mcp.name.to_string(),
            serde_json::json!({ "command": mcp.command, "args": mcp.args }),
        );
    }
    serde_json::json!({ "mcpServers": mcp_servers })
}

impl Harness {
    /// Built-in fallback used whenever no model is configured/passed —
    /// `None` for CLI harnesses that never took an explicit model in this
    /// codebase before model selection existed (copilot/opencode/codex/
    /// gemini), so an unconfigured run keeps behaving exactly as before:
    /// no `--model` flag at all, the CLI's own default applies.
    pub fn default_model(&self) -> Option<&'static str> {
        match self {
            Harness::Claude => Some("haiku"),
            Harness::ClaudeSdk => Some("claude-haiku-4-5"),
            Harness::GeminiSdk => Some("gemini-3.6-flash"),
            Harness::Copilot | Harness::Opencode | Harness::Codex | Harness::Gemini => None,
        }
    }

    /// A handful of doc-verified models for harnesses whose model space is
    /// small and stable enough to curate (Anthropic, Google) — used for the
    /// `autoqa config` arrow-key picker. `None` for harnesses where a
    /// scriptable/bounded list isn't available (copilot has none; opencode
    /// returns 100+ dynamic entries across providers), which fall back to a
    /// free-text prompt instead.
    pub fn model_choices(&self) -> Option<&'static [&'static str]> {
        match self {
            Harness::Claude | Harness::ClaudeSdk => Some(&[
                "claude-haiku-4-5",
                "claude-sonnet-5",
                "claude-opus-5",
                "claude-fable-5",
            ]),
            Harness::Gemini | Harness::GeminiSdk => {
                Some(&["gemini-3.6-flash", "gemini-3.5-flash-lite"])
            }
            Harness::Copilot | Harness::Opencode | Harness::Codex => None,
        }
    }

    fn resolved_model(&self, model: Option<&str>) -> Option<String> {
        model
            .map(str::to_string)
            .or_else(|| self.default_model().map(str::to_string))
    }

    pub fn build_run_command(
        &self,
        query: &str,
        mcp: &[McpServerSpec],
        system_prompt: &str,
        model: Option<&str>,
    ) -> anyhow::Result<std::process::Command> {
        let model = self.resolved_model(model);
        // Space-separated `mcp__<name>` patterns — Claude's --allowedTools
        // accepts multiple space-separated patterns in one string.
        let allowed_tools = mcp
            .iter()
            .map(|m| format!("mcp__{}", m.name))
            .collect::<Vec<_>>()
            .join(" ");
        match self {
            Harness::Claude => {
                let dir = scratch_dir("claude-run");
                std::fs::create_dir_all(&dir)?;
                let mut cmd = std::process::Command::new("claude");
                cmd.arg("-p")
                    .arg(query)
                    .arg("--model")
                    .arg(model.as_deref().expect("Claude always has a default_model"))
                    .arg("--mcp-config")
                    .arg(mcp_config_json(mcp).to_string())
                    .arg("--strict-mcp-config")
                    // No user/project/local settings.json, no skills. (Not
                    // --safe-mode: verified by hand it also kills MCP tools
                    // from --mcp-config, not just config-file ones — leaves
                    // the agent with zero browser tools. Not --bare either:
                    // that drops OAuth/keychain auth, requiring
                    // ANTHROPIC_API_KEY.) CLAUDE.md auto-discovery is
                    // cwd-based, separate from --setting-sources — closed
                    // below via current_dir instead.
                    .arg("--setting-sources")
                    .arg("")
                    .arg("--disable-slash-commands")
                    .arg("--append-system-prompt")
                    .arg(system_prompt)
                    .arg("--allowedTools")
                    .arg(allowed_tools)
                    .arg("--permission-mode")
                    .arg("acceptEdits")
                    .arg("--output-format")
                    .arg("stream-json")
                    .arg("--verbose")
                    // --setting-sources only covers settings.json — CLAUDE.md
                    // project-memory auto-discovery is cwd-based and
                    // separate, so run from an empty scratch dir instead of
                    // the caller's actual project (no filesystem tools are
                    // allowed here anyway, only the MCP browser tools).
                    .current_dir(&dir);
                Ok(cmd)
            }
            Harness::ClaudeSdk => {
                let dir = scratch_dir("claude-sdk-run");
                std::fs::create_dir_all(&dir)?;
                let system_prompt_path = dir.join("system.md");
                std::fs::write(&system_prompt_path, system_prompt)?;
                let mcp_config_path = dir.join("mcp-config.json");
                std::fs::write(&mcp_config_path, mcp_config_json(mcp).to_string())?;

                let script_path = ensure_claude_sdk_script()?;
                let mut cmd = std::process::Command::new("node");
                cmd.arg(&script_path)
                    .arg(query)
                    .arg("--system-prompt-file")
                    .arg(&system_prompt_path)
                    .arg("--mcp-config-file")
                    .arg(&mcp_config_path)
                    .arg("--model")
                    .arg(
                        model
                            .as_deref()
                            .expect("ClaudeSdk always has a default_model"),
                    );
                Ok(cmd)
            }
            Harness::Copilot => {
                let dir = scratch_dir("copilot");
                std::fs::create_dir_all(&dir)?;
                let config_path = dir.join("mcp-config.json");
                std::fs::write(&config_path, mcp_config_json(mcp).to_string())?;

                // No confirmed flag to set a system prompt per-invocation —
                // fold it into the user turn itself as a prefix.
                let prompt = format!("{system_prompt}\n\n{query}");
                let mut cmd = std::process::Command::new("copilot");
                // --additional-mcp-config takes a JSON string OR an @-prefixed
                // file path — a bare path is parsed as JSON text and fails.
                // It also *augments* ~/.copilot/mcp-config.json rather than
                // replacing it, so strip AGENTS.md-style custom instructions
                // and the built-in github-mcp-server explicitly. COPILOT_HOME
                // (undocumented but confirmed by testing) redirects the whole
                // ~/.copilot root — config, skills/, installed-plugins/,
                // mcp-config.json — so --additional-mcp-config's "augment"
                // has nothing but our own file to augment. Auth is
                // keychain-based, unaffected by the redirect.
                cmd.arg("-p")
                    .arg(prompt)
                    .arg("--yolo")
                    .arg("--additional-mcp-config")
                    .arg(format!("@{}", config_path.display()))
                    .arg("--no-custom-instructions")
                    .arg("--disable-builtin-mcps")
                    .env("COPILOT_HOME", &dir);
                if let Some(m) = &model {
                    cmd.arg("--model").arg(m);
                }
                Ok(cmd)
            }
            Harness::Opencode => {
                let dir = scratch_dir("opencode-run");
                std::fs::create_dir_all(&dir)?;
                std::fs::write(dir.join("system-prompt.md"), system_prompt)?;
                let mut mcp_servers = serde_json::Map::new();
                for m in mcp {
                    mcp_servers.insert(
                        m.name.to_string(),
                        serde_json::json!({
                            "type": "local",
                            "command": std::iter::once(m.command.to_string()).chain(m.args.clone()).collect::<Vec<_>>(),
                            "enabled": true
                        }),
                    );
                }
                let config = serde_json::json!({
                    "mcp": mcp_servers,
                    "instructions": ["system-prompt.md"]
                });
                std::fs::write(dir.join("opencode.jsonc"), config.to_string())?;

                // --pure: global plugins (user's ~/.config/opencode/opencode.json)
                // still load without it and can inject arbitrary banner/log
                // lines into stdout, breaking the jq log_filter.
                let mut cmd = std::process::Command::new("opencode");
                cmd.arg("run")
                    .arg(query)
                    .arg("--dangerously-skip-permissions")
                    .arg("--pure")
                    .arg("--format")
                    .arg("json")
                    .arg("--dir")
                    .arg(&dir);
                if let Some(m) = &model {
                    cmd.arg("--model").arg(m);
                }
                Ok(cmd)
            }
            Harness::Codex => {
                let dir = scratch_dir("codex-run");
                std::fs::create_dir_all(&dir)?;
                std::fs::write(dir.join("system-prompt.md"), system_prompt)?;
                let config = codex_config_toml(mcp);
                std::fs::write(dir.join("config.toml"), config)?;
                copy_codex_auth(&dir);

                let mut cmd = std::process::Command::new("codex");
                cmd.arg("exec")
                    .arg(query)
                    .arg("--json")
                    .arg("--dangerously-bypass-approvals-and-sandbox")
                    // CODEX_HOME isolates the global ~/.codex config, but
                    // codex also auto-discovers a project-level AGENTS.md
                    // from cwd — running from the caller's actual project
                    // dir would merge in whatever instructions/skills live
                    // there and can hijack this task (e.g. its own chrome
                    // launch instead of the CDP session we hand it).
                    .env("CODEX_HOME", &dir)
                    .current_dir(&dir);
                if let Some(m) = &model {
                    cmd.arg("--model").arg(m);
                }
                Ok(cmd)
            }
            Harness::Gemini => {
                let dir = scratch_dir("gemini-run");
                std::fs::create_dir_all(dir.join(".gemini"))?;
                let system_prompt_path = dir.join("system.md");
                std::fs::write(&system_prompt_path, system_prompt)?;
                std::fs::write(
                    dir.join(".gemini/settings.json"),
                    mcp_config_json(mcp).to_string(),
                )?;

                let mut cmd = std::process::Command::new("gemini");
                cmd.arg("-p")
                    .arg(query)
                    .arg("-o")
                    .arg("stream-json")
                    .arg("--approval-mode=yolo")
                    .arg("--skip-trust")
                    .current_dir(&dir)
                    .env("GEMINI_SYSTEM_MD", &system_prompt_path)
                    // GEMINI_CLI_HOME overrides os.homedir() for all .gemini
                    // resolution (skills, extensions, settings, MCP config)
                    // — same isolation CODEX_HOME gives codex. Combined with
                    // current_dir above (no project-local .gemini either),
                    // nothing but our own settings.json can load.
                    .env("GEMINI_CLI_HOME", &dir);
                if let Some(m) = &model {
                    cmd.arg("--model").arg(m);
                }
                // Whatever other MCP servers/extensions merge in from the
                // user's global ~/.gemini config, this locks which ones the
                // model can actually call to just ours.
                for m in mcp {
                    cmd.arg("--allowed-mcp-server-names").arg(m.name);
                }
                Ok(cmd)
            }
            Harness::GeminiSdk => {
                let dir = scratch_dir("gemini-sdk-run");
                std::fs::create_dir_all(&dir)?;
                let system_prompt_path = dir.join("system.md");
                std::fs::write(&system_prompt_path, system_prompt)?;
                let mcp_config_path = dir.join("mcp-config.json");
                std::fs::write(&mcp_config_path, mcp_config_json(mcp).to_string())?;

                let script_path = ensure_gemini_sdk_script()?;
                let mut cmd = std::process::Command::new("node");
                cmd.arg(&script_path)
                    .arg(query)
                    .arg("--system-prompt-file")
                    .arg(&system_prompt_path)
                    .arg("--mcp-config-file")
                    .arg(&mcp_config_path)
                    .arg("--model")
                    .arg(
                        model
                            .as_deref()
                            .expect("GeminiSdk always has a default_model"),
                    );
                Ok(cmd)
            }
        }
    }

    pub fn build_chat_command(
        &self,
        prompt: &str,
        system_prompt: &str,
        model: Option<&str>,
    ) -> anyhow::Result<std::process::Command> {
        let model = self.resolved_model(model);
        match self {
            Harness::Claude => {
                let mut cmd = std::process::Command::new("claude");
                cmd.arg("-p")
                    .arg(prompt)
                    .arg("--model")
                    .arg(model.as_deref().expect("Claude always has a default_model"))
                    .arg("--append-system-prompt")
                    .arg(system_prompt)
                    .arg("--allowedTools")
                    .arg("");
                Ok(cmd)
            }
            Harness::ClaudeSdk => {
                let dir = scratch_dir("claude-sdk-chat");
                std::fs::create_dir_all(&dir)?;
                let system_prompt_path = dir.join("system.md");
                std::fs::write(&system_prompt_path, system_prompt)?;

                let script_path = ensure_claude_sdk_script()?;
                let mut cmd = std::process::Command::new("node");
                // --raw: no MCP config, no decorative log lines — chat mode's
                // caller (edit_actions_via_chat) parses stdout directly as
                // the model's raw text answer.
                cmd.arg(&script_path)
                    .arg(prompt)
                    .arg("--system-prompt-file")
                    .arg(&system_prompt_path)
                    .arg("--raw")
                    .arg("--model")
                    .arg(
                        model
                            .as_deref()
                            .expect("ClaudeSdk always has a default_model"),
                    );
                Ok(cmd)
            }
            Harness::Copilot => {
                let dir = scratch_dir("copilot-chat");
                std::fs::create_dir_all(&dir)?;
                // No MCP config, no --yolo: nothing is configured to
                // approve, so there's nothing for the model to call.
                let full_prompt = format!("{system_prompt}\n\n{prompt}");
                let mut cmd = std::process::Command::new("copilot");
                cmd.arg("-p")
                    .arg(full_prompt)
                    .arg("--no-custom-instructions")
                    .arg("--disable-builtin-mcps")
                    .env("COPILOT_HOME", &dir);
                if let Some(m) = &model {
                    cmd.arg("--model").arg(m);
                }
                Ok(cmd)
            }
            Harness::Opencode => {
                let dir = scratch_dir("opencode-chat");
                std::fs::create_dir_all(&dir)?;
                std::fs::write(dir.join("system-prompt.md"), system_prompt)?;
                let config = serde_json::json!({ "instructions": ["system-prompt.md"] });
                std::fs::write(dir.join("opencode.jsonc"), config.to_string())?;

                // No --format json here: chat parses stdout directly as the
                // model's raw text answer, not an NDJSON event stream.
                // --pure: see build_run_command's comment on global plugins.
                let mut cmd = std::process::Command::new("opencode");
                cmd.arg("run")
                    .arg(prompt)
                    .arg("--pure")
                    .arg("--dir")
                    .arg(&dir);
                if let Some(m) = &model {
                    cmd.arg("--model").arg(m);
                }
                Ok(cmd)
            }
            Harness::Codex => {
                let dir = scratch_dir("codex-chat");
                std::fs::create_dir_all(&dir)?;
                std::fs::write(dir.join("system-prompt.md"), system_prompt)?;
                // No [mcp_servers] table at all: nothing configured to call.
                let config = codex_config_toml(&[]);
                std::fs::write(dir.join("config.toml"), config)?;
                copy_codex_auth(&dir);

                let mut cmd = std::process::Command::new("codex");
                cmd.arg("exec")
                    .arg(prompt)
                    .arg("--json")
                    .arg("--dangerously-bypass-approvals-and-sandbox")
                    .env("CODEX_HOME", &dir)
                    .current_dir(&dir);
                if let Some(m) = &model {
                    cmd.arg("--model").arg(m);
                }
                Ok(cmd)
            }
            Harness::Gemini => {
                let dir = scratch_dir("gemini-chat");
                std::fs::create_dir_all(&dir)?;
                let system_prompt_path = dir.join("system.md");
                std::fs::write(&system_prompt_path, system_prompt)?;
                // No .gemini/settings.json: no MCP servers registered.
                let mut cmd = std::process::Command::new("gemini");
                cmd.arg("-p")
                    .arg(prompt)
                    .arg("-o")
                    .arg("stream-json")
                    .arg("--skip-trust")
                    .current_dir(&dir)
                    .env("GEMINI_SYSTEM_MD", &system_prompt_path)
                    .env("GEMINI_CLI_HOME", &dir);
                if let Some(m) = &model {
                    cmd.arg("--model").arg(m);
                }
                Ok(cmd)
            }
            Harness::GeminiSdk => {
                let dir = scratch_dir("gemini-sdk-chat");
                std::fs::create_dir_all(&dir)?;
                let system_prompt_path = dir.join("system.md");
                std::fs::write(&system_prompt_path, system_prompt)?;

                let script_path = ensure_gemini_sdk_script()?;
                let mut cmd = std::process::Command::new("node");
                // --raw: no MCP config, no decorative log lines — chat mode's
                // caller (edit_actions_via_chat) parses stdout directly as
                // the model's raw text answer.
                cmd.arg(&script_path)
                    .arg(prompt)
                    .arg("--system-prompt-file")
                    .arg(&system_prompt_path)
                    .arg("--raw")
                    .arg("--model")
                    .arg(
                        model
                            .as_deref()
                            .expect("GeminiSdk always has a default_model"),
                    );
                Ok(cmd)
            }
        }
    }

    /// jq filter for the run's NDJSON stdout, or None for raw passthrough.
    /// None for Copilot/Codex is deliberate: their stream schemas are
    /// unconfirmed by official docs, so raw passthrough is more honest than
    /// a speculative filter that silently drops everything.
    pub fn log_filter(&self) -> Option<&'static str> {
        match self {
            Harness::Claude => Some(CLAUDE_LOG_FILTER),
            // Script prints its own final log format directly — no
            // undocumented schema to guess at, so raw passthrough.
            Harness::ClaudeSdk => None,
            Harness::Copilot => None,
            Harness::Opencode => Some(OPENCODE_LOG_FILTER),
            Harness::Codex => None,
            Harness::Gemini => Some(GEMINI_LOG_FILTER),
            // Script prints its own final log format directly — no
            // undocumented schema to guess at, so raw passthrough.
            Harness::GeminiSdk => None,
        }
    }
}

/// `developer_instructions` is chosen over `model_instructions_file` to
/// match Claude's --append-system-prompt (append, not replace) semantics —
/// unverified against real Codex behavior, check before relying on it.
fn codex_config_toml(mcp: &[McpServerSpec]) -> String {
    let mut out = String::from("developer_instructions = \"system-prompt.md\"\n");
    for m in mcp {
        out.push_str(&format!(
            "\n[mcp_servers.{}]\ncommand = \"{}\"\nargs = [{}]\n",
            m.name,
            m.command,
            m.args
                .iter()
                .map(|a| format!("\"{a}\""))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(cmd: &std::process::Command) -> Vec<String> {
        std::iter::once(cmd.get_program().to_string_lossy().into_owned())
            .chain(cmd.get_args().map(|a| a.to_string_lossy().into_owned()))
            .collect()
    }

    fn playwright_spec() -> McpServerSpec {
        McpServerSpec {
            name: "playwright",
            command: "npx",
            args: vec!["@playwright/mcp@latest".into()],
        }
    }

    #[test]
    fn claude_run_uses_default_model_when_unset() {
        let cmd = Harness::Claude
            .build_run_command("q", &[playwright_spec()], "sys", None)
            .unwrap();
        let a = argv(&cmd);
        let i = a.iter().position(|s| s == "--model").unwrap();
        assert_eq!(a[i + 1], "haiku");
    }

    #[test]
    fn claude_run_model_override_replaces_default() {
        let cmd = Harness::Claude
            .build_run_command("q", &[playwright_spec()], "sys", Some("claude-opus-5"))
            .unwrap();
        let a = argv(&cmd);
        let i = a.iter().position(|s| s == "--model").unwrap();
        assert_eq!(a[i + 1], "claude-opus-5");
    }

    #[test]
    fn copilot_run_omits_model_flag_when_unset() {
        // Copilot has no default_model — an unconfigured run must keep
        // behaving exactly as before model selection existed: no --model
        // flag at all, the CLI's own default applies.
        let cmd = Harness::Copilot
            .build_run_command("q", &[playwright_spec()], "sys", None)
            .unwrap();
        assert!(!argv(&cmd).contains(&"--model".to_string()));
    }

    #[test]
    fn copilot_run_passes_model_flag_when_set() {
        let cmd = Harness::Copilot
            .build_run_command("q", &[playwright_spec()], "sys", Some("gpt-5.4"))
            .unwrap();
        let a = argv(&cmd);
        let i = a.iter().position(|s| s == "--model").unwrap();
        assert_eq!(a[i + 1], "gpt-5.4");
    }

    #[test]
    fn claude_run_argv_is_isolated_from_user_config() {
        let cmd = Harness::Claude
            .build_run_command("do the thing", &[playwright_spec()], "sys prompt", None)
            .unwrap();
        let a = argv(&cmd);
        assert_eq!(a[0], "claude");
        assert!(a.contains(&"--permission-mode".to_string()));
        assert!(a.contains(&"acceptEdits".to_string()));
        assert!(a.contains(&"--strict-mcp-config".to_string()));
        assert!(a.contains(&"--setting-sources".to_string()));
        assert!(a.contains(&"--disable-slash-commands".to_string()));
    }

    #[test]
    fn codex_run_sets_codex_home_env() {
        let cmd = Harness::Codex
            .build_run_command("q", &[playwright_spec()], "sys", None)
            .unwrap();
        assert!(cmd
            .get_envs()
            .any(|(k, _)| k == std::ffi::OsStr::new("CODEX_HOME")));
    }

    #[test]
    fn codex_run_cwd_is_scratch_dir_not_callers_project() {
        // Codex auto-discovers a project-level AGENTS.md from cwd,
        // independent of CODEX_HOME — running from the caller's actual
        // project would merge in whatever instructions/skills live there.
        let cmd = Harness::Codex
            .build_run_command("q", &[playwright_spec()], "sys", None)
            .unwrap();
        let cwd = cmd.get_current_dir().expect("current_dir must be set");
        assert!(cwd.ends_with("codex-run"));
    }

    #[test]
    fn codex_config_toml_has_mcp_server_and_instructions() {
        let toml_str = codex_config_toml(&[playwright_spec()]);
        let parsed: toml::Value = toml::from_str(&toml_str).unwrap();
        assert_eq!(
            parsed["developer_instructions"].as_str(),
            Some("system-prompt.md")
        );
        assert_eq!(
            parsed["mcp_servers"]["playwright"]["command"].as_str(),
            Some("npx")
        );
    }

    #[test]
    fn codex_chat_config_has_no_mcp_servers_table() {
        let toml_str = codex_config_toml(&[]);
        let parsed: toml::Value = toml::from_str(&toml_str).unwrap();
        assert!(parsed.get("mcp_servers").is_none());
    }

    #[test]
    fn gemini_env_points_at_system_prompt_file_containing_prompt_text() {
        let cmd = Harness::Gemini
            .build_run_command("q", &[playwright_spec()], "unique sys prompt text", None)
            .unwrap();
        let path = cmd
            .get_envs()
            .find(|(k, _)| *k == std::ffi::OsStr::new("GEMINI_SYSTEM_MD"))
            .and_then(|(_, v)| v)
            .expect("GEMINI_SYSTEM_MD set")
            .to_owned();
        let contents = std::fs::read_to_string(path).unwrap();
        assert_eq!(contents, "unique sys prompt text");
        assert!(cmd
            .get_envs()
            .any(|(k, _)| k == std::ffi::OsStr::new("GEMINI_CLI_HOME")));
    }

    #[test]
    fn copilot_run_sets_copilot_home_env() {
        let cmd = Harness::Copilot
            .build_run_command("q", &[playwright_spec()], "sys", None)
            .unwrap();
        assert!(cmd
            .get_envs()
            .any(|(k, _)| k == std::ffi::OsStr::new("COPILOT_HOME")));
    }

    #[test]
    fn opencode_jsonc_is_valid_json_with_mcp_and_instructions_keys() {
        Harness::Opencode
            .build_run_command("q", &[playwright_spec()], "sys", None)
            .unwrap();
        let dir = scratch_dir("opencode-run");
        let contents = std::fs::read_to_string(dir.join("opencode.jsonc")).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&contents).unwrap();
        assert!(parsed["mcp"]["playwright"].is_object());
        assert!(parsed["instructions"].is_array());
    }

    #[test]
    fn harness_without_confirmed_schema_has_no_log_filter() {
        assert!(Harness::Copilot.log_filter().is_none());
        assert!(Harness::Codex.log_filter().is_none());
        // Script owns its own log format directly, no schema to guess at.
        assert!(Harness::GeminiSdk.log_filter().is_none());
        assert!(Harness::ClaudeSdk.log_filter().is_none());
    }

    #[test]
    fn harness_with_confirmed_schema_has_log_filter() {
        assert!(Harness::Claude.log_filter().is_some());
        assert!(Harness::Opencode.log_filter().is_some());
        assert!(Harness::Gemini.log_filter().is_some());
    }
}
