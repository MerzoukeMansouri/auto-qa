use crate::action_entry::ActionEntry;

/// Parses a Playwright MCP `session.md` (written by `--save-session`) into
/// editable `ActionEntry`s — each tool call's Result JSON carries a
/// ready-made `code` field (e.g. `await page.goto(...)`), one entry per JS
/// statement line, `kind: "code"`, the statement itself in `value` so the
/// review UI can show/edit it like any other action before codegen.
pub fn parse_mcp_session(session_md: &str) -> Vec<ActionEntry> {
    let mut entries = Vec::new();
    let mut rest = session_md;
    while let Some(start) = rest.find("```json") {
        let after_fence = &rest[start + "```json".len()..];
        let Some(end) = after_fence.find("```") else {
            break;
        };
        let block = &after_fence[..end];
        rest = &after_fence[end + "```".len()..];
        if let Ok(serde_json::Value::Object(obj)) = serde_json::from_str(block) {
            if let Some(serde_json::Value::String(code)) = obj.get("code") {
                for line in code.lines() {
                    entries.push(ActionEntry {
                        value: Some(line.to_string()),
                        ..ActionEntry::new("code")
                    });
                }
            }
        }
    }
    entries
}

fn esc(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\'', "\\'")
}

/// Generates a self-contained Playwright TS test from a captured/edited
/// action array. Entries come from `parse_mcp_session` (`kind: "code"`,
/// ready-made statements) plus any manual assertions added in `cua review`.
pub fn generate(entries: &[ActionEntry]) -> String {
    let mut lines: Vec<String> = Vec::new();

    for e in entries {
        match e.kind.as_str() {
            "code" => {
                if let Some(code) = &e.value {
                    lines.push(format!("  {code}"));
                }
            }
            "assert" => {
                if let Some(sel) = &e.selector {
                    let locator = format!("page.locator('{}')", esc(sel));
                    match e.assert_kind.as_deref() {
                        Some("text") => lines.push(format!(
                            "  await expect({locator}).toHaveText('{}');",
                            esc(e.value.as_deref().unwrap_or(""))
                        )),
                        Some("value") => lines.push(format!(
                            "  await expect({locator}).toHaveValue('{}');",
                            esc(e.value.as_deref().unwrap_or(""))
                        )),
                        _ => lines.push(format!("  await expect({locator}).toBeVisible();")),
                    }
                }
            }
            _ => {}
        }
    }

    format!(
        "import {{ test, expect }} from '@playwright/test';\n\ntest('generated from cua session', async ({{ page }}) => {{\n{}\n}});\n",
        lines.join("\n")
    )
}
