use crate::action_entry::ActionEntry;

/// Splits a `code` blob into top-level JS statements. Not a plain
/// `.lines()` split — a single statement can itself span multiple lines
/// (e.g. `toMatchAriaSnapshot`'s backtick template literal), so this tracks
/// paren depth and template-literal state and only splits on a `;` that's
/// at depth 0 and outside a template literal.
fn split_statements(code: &str) -> Vec<String> {
    let mut statements = Vec::new();
    let mut buf = String::new();
    let mut depth = 0i32;
    let mut in_template = false;
    for c in code.chars() {
        buf.push(c);
        match c {
            '`' => in_template = !in_template,
            '(' if !in_template => depth += 1,
            ')' if !in_template => depth -= 1,
            ';' if !in_template && depth == 0 => {
                let stmt = buf.trim().to_string();
                if !stmt.is_empty() {
                    statements.push(stmt);
                }
                buf.clear();
            }
            _ => {}
        }
    }
    let rest = buf.trim();
    if !rest.is_empty() {
        statements.push(rest.to_string());
    }
    statements
}

/// Parses a Playwright MCP `session.md` (written by `--save-session`) into
/// editable `ActionEntry`s — each tool call's Result JSON carries a
/// ready-made `code` field (e.g. `await page.goto(...)` or, for a
/// `browser_verify_*` call, `await expect(...)`). An `expect(...)` statement
/// is paired onto the action immediately before it as that step's
/// assertion, rather than becoming its own entry — matching how `autoqa run`
/// is prompted to verify right after each action.
pub fn parse_mcp_session(session_md: &str) -> Vec<ActionEntry> {
    let mut entries: Vec<ActionEntry> = Vec::new();
    let mut pending: Option<ActionEntry> = None;
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
                for stmt in split_statements(code) {
                    if stmt.starts_with("await expect(") {
                        match pending.take() {
                            Some(mut item) => {
                                item.assertion = stmt;
                                entries.push(item);
                            }
                            None => entries.push(ActionEntry {
                                assertion: stmt,
                                ..Default::default()
                            }),
                        }
                    } else {
                        if let Some(item) = pending.take() {
                            entries.push(item);
                        }
                        pending = Some(ActionEntry {
                            action: stmt,
                            ..Default::default()
                        });
                    }
                }
            }
        }
    }
    if let Some(item) = pending {
        entries.push(item);
    }
    entries
}

fn step_lines(entries: &[ActionEntry]) -> Vec<String> {
    let mut lines = Vec::new();
    for e in entries {
        if !e.action.is_empty() {
            lines.push(format!("  {}", e.action));
        }
        if !e.assertion.is_empty() {
            lines.push(format!("  {}", e.assertion));
        }
    }
    lines
}

fn wrap_test(title: &str, lines: &[String]) -> String {
    format!(
        "import {{ test, expect }} from '@playwright/test';\n\ntest('{}', async ({{ page }}) => {{\n{}\n}});\n",
        title.replace('\\', "\\\\").replace('\'', "\\'"),
        lines.join("\n")
    )
}

/// Generates a self-contained Playwright TS test from a captured/edited
/// action array. `title` is typically the original `autoqa run --query` text —
/// falls back to a generic name when unavailable (e.g. actions.json edited
/// by hand with no run behind it).
pub fn generate(entries: &[ActionEntry], title: &str) -> String {
    wrap_test(title, &step_lines(entries))
}

/// Generates a test truncated after `index` (inclusive) with a trailing
/// `page.pause()` — running this headed drops the developer into Playwright
/// Inspector with the real DOM at exactly that step, live, to fix a selector
/// or verify what's actually on the page instead of guessing from a snapshot.
pub fn generate_up_to_with_pause(entries: &[ActionEntry], index: usize, title: &str) -> String {
    let mut lines = step_lines(&entries[..=index.min(entries.len().saturating_sub(1))]);
    lines.push("  await page.pause();".to_string());
    wrap_test(title, &lines)
}
