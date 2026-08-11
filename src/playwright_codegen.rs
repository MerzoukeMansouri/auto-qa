use crate::action_entry::ActionEntry;
use crate::block::TestStep;
use crate::state;

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
/// editable `ActionEntry`s, each paired with the byte offset in
/// `session_md` marking the end of the tool-call block it came from — a
/// precise, clock-free position marker. `autoqa-blocks`'s `run_block` log
/// (see `node/block-server`) stamps each call with `session.md`'s byte
/// length *at that moment* (read live, during the run) instead of a
/// timestamp, for exactly this reason: most tool-call entries here carry no
/// usable timestamp of their own (only ones with a console-log `events`
/// reference do, which in practice is a small minority), so a byte-offset
/// boundary — "this many bytes of session.md existed when run_block was
/// called" — is the only reliable ordering signal available. See
/// `state::sync_actions_from_latest_mcp_session` for the merge.
///
/// Each tool call's Result JSON carries a ready-made `code` field (e.g.
/// `await page.goto(...)` or, for a `browser_verify_*` call, `await
/// expect(...)`). An `expect(...)` statement is paired onto the action
/// immediately before it as that step's assertion, rather than becoming its
/// own entry — matching how `autoqa run` is prompted to verify right after
/// each action.
pub fn parse_mcp_session_with_offset(session_md: &str) -> Vec<(ActionEntry, usize)> {
    let mut entries: Vec<(ActionEntry, usize)> = Vec::new();
    let mut pending: Option<(ActionEntry, usize)> = None;
    let mut rest = session_md;
    while let Some(start) = rest.find("```json") {
        let after_fence = &rest[start + "```json".len()..];
        let Some(end) = after_fence.find("```") else {
            break;
        };
        let block = &after_fence[..end];
        rest = &after_fence[end + "```".len()..];
        let offset = session_md.len() - rest.len();
        if let Ok(serde_json::Value::Object(obj)) = serde_json::from_str(block) {
            if let Some(serde_json::Value::String(code)) = obj.get("code") {
                for stmt in split_statements(code) {
                    if stmt.starts_with("await expect(") {
                        match pending.take() {
                            Some((mut item, item_offset)) => {
                                item.assertion = stmt;
                                entries.push((item, item_offset));
                            }
                            None => entries.push((
                                ActionEntry {
                                    assertion: stmt,
                                    ..Default::default()
                                },
                                offset,
                            )),
                        }
                    } else {
                        if let Some(item) = pending.take() {
                            entries.push(item);
                        }
                        pending = Some((
                            ActionEntry {
                                action: stmt,
                                ..Default::default()
                            },
                            offset,
                        ));
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

fn push_entry_lines(lines: &mut Vec<String>, action: &str, assertion: &str) {
    if !action.is_empty() {
        lines.push(format!("  {action}"));
    }
    if !assertion.is_empty() {
        lines.push(format!("  {assertion}"));
    }
}

/// Substitutes every `{{placeholder}}` token in `text` with its bound param's
/// value, hard-erroring on a token with no binding or a binding pointing at a
/// missing param.
fn resolve_placeholders(
    text: &str,
    slug: &str,
    bindings: &std::collections::HashMap<String, String>,
    params: &[crate::block::Param],
) -> anyhow::Result<String> {
    let mut out = text.to_string();
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            break;
        };
        let placeholder = after[..end].trim();
        let param_name = bindings.get(placeholder).ok_or_else(|| {
            anyhow::anyhow!(
                "block '{slug}' has placeholder '{{{{{placeholder}}}}}' with no binding"
            )
        })?;
        let value = params
            .iter()
            .find(|p| &p.name == param_name)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "block '{slug}' placeholder '{{{{{placeholder}}}}}' is bound to param \
                     '{param_name}', which does not exist in params.json"
                )
            })?;
        out = out.replace(&format!("{{{{{placeholder}}}}}"), &value.value);
        rest = &after[end + 2..];
    }
    Ok(out)
}

fn step_lines(entries: &[TestStep]) -> anyhow::Result<Vec<String>> {
    let mut lines = Vec::new();
    for e in entries {
        match e {
            TestStep::Step { action, assertion } => push_entry_lines(&mut lines, action, assertion),
            TestStep::Block { slug, bindings } => {
                let block = state::read_block(slug).map_err(|_| {
                    anyhow::anyhow!("block '{slug}' referenced by this test no longer exists")
                })?;
                let params = state::read_params();
                for step in &block.steps {
                    let action = resolve_placeholders(&step.action, slug, bindings, &params)?;
                    let assertion = resolve_placeholders(&step.assertion, slug, bindings, &params)?;
                    push_entry_lines(&mut lines, &action, &assertion);
                }
            }
        }
    }
    Ok(lines)
}

/// Escapes a title for embedding in a single-quoted JS string literal —
/// backslash/quote escaping alone isn't enough, an unescaped newline breaks
/// a single-quoted string just as badly (observed: a multi-line title
/// produced an unparseable `.spec.ts`), so newlines are collapsed to spaces
/// too.
fn escape_js_string_literal(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('\'', "\\'")
        .replace(['\n', '\r'], " ")
}

fn wrap_test(title: &str, lines: &[String]) -> String {
    format!(
        "import {{ test, expect }} from '@playwright/test';\n\ntest('{}', async ({{ page }}) => {{\n{}\n}});\n",
        escape_js_string_literal(title),
        lines.join("\n")
    )
}

/// Generates a self-contained Playwright TS test from a captured/edited
/// action array. `title` is typically the original `autoqa run --query` text —
/// falls back to a generic name when unavailable (e.g. actions.json edited
/// by hand with no run behind it).
pub fn generate(entries: &[TestStep], title: &str) -> anyhow::Result<String> {
    Ok(wrap_test(title, &step_lines(entries)?))
}

/// Generates a test truncated after `index` (inclusive) with a trailing
/// `page.pause()` — running this headed drops the developer into Playwright
/// Inspector with the real DOM at exactly that step, live, to fix a selector
/// or verify what's actually on the page instead of guessing from a snapshot.
pub fn generate_up_to_with_pause(
    entries: &[TestStep],
    index: usize,
    title: &str,
) -> anyhow::Result<String> {
    let mut lines = step_lines(&entries[..=index.min(entries.len().saturating_sub(1))])?;
    lines.push("  await page.pause();".to_string());
    Ok(wrap_test(title, &lines))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multiline_title_does_not_break_the_js_string_literal() {
        let ts = generate(&[], "line one\nline two").unwrap();
        // The title itself must collapse onto one line inside the single
        // quotes — a raw newline there would unterminate the string, same
        // as the real bug this locks in (a multi-line query text broke the
        // generated .spec.ts). The rest of the file legitimately has
        // newlines (imports, function body), so this checks the title's
        // own line, not the whole file.
        assert!(ts.contains("test('line one line two'"));
        assert!(!ts.contains("test('line one\n"));
    }

    #[test]
    fn session_offsets_are_strictly_increasing_in_call_order() {
        let md = "\
### Tool call: a
- Result
```json
{\"code\": \"await page.goto('x');\"}
```

### Tool call: b
- Result
```json
{\"code\": \"await page.click('y');\"}
```
";
        let entries = parse_mcp_session_with_offset(md);
        assert_eq!(entries.len(), 2);
        assert!(
            entries[0].1 < entries[1].1,
            "later tool call must have a strictly larger byte offset"
        );
    }
}
