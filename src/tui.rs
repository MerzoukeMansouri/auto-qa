use crate::block::{Block, Param};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block as UiBlock, Borders, List, ListItem, Paragraph, Wrap};
use ratatui::Terminal;
use std::collections::HashMap;
use std::io::Write;
use std::time::Duration;

/// Harness picker screen, used both for the first-run prompt (`resolve_harness`)
/// and `autoqa config` with no `--harness`. `Esc`/`q` aborts (returns Err) —
/// same convention as `pick_blocks`.
pub fn pick_harness(
    current: Option<crate::harness::Harness>,
) -> anyhow::Result<crate::harness::Harness> {
    let mut term = enter_tui()?;
    let mut cursor = current
        .and_then(|c| crate::harness::ALL.iter().position(|h| *h == c))
        .unwrap_or(0);

    let result = loop {
        term.draw(|f| {
            let area = f.area();
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(3), Constraint::Length(3)])
                .split(area);

            let items: Vec<ListItem> = crate::harness::ALL
                .iter()
                .enumerate()
                .map(|(i, h)| {
                    let style = if i == cursor {
                        Style::default().add_modifier(Modifier::REVERSED)
                    } else {
                        Style::default()
                    };
                    let marker = if current == Some(*h) {
                        " (current)"
                    } else {
                        ""
                    };
                    ListItem::new(format!("{h}{marker}")).style(style)
                })
                .collect();
            f.render_widget(
                List::new(items).block(
                    UiBlock::default()
                        .borders(Borders::ALL)
                        .title("Pick a harness"),
                ),
                rows[0],
            );

            let help = "↑/↓: move  Enter: select  q/Esc: cancel";
            f.render_widget(
                Paragraph::new(help)
                    .wrap(Wrap { trim: true })
                    .block(UiBlock::default().borders(Borders::ALL)),
                rows[1],
            );
        })?;

        if !event::poll(Duration::from_millis(200))? {
            continue;
        }
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        match key.code {
            KeyCode::Up => {
                cursor = (cursor + crate::harness::ALL.len() - 1) % crate::harness::ALL.len()
            }
            KeyCode::Down => cursor = (cursor + 1) % crate::harness::ALL.len(),
            KeyCode::Enter => break Ok(crate::harness::ALL[cursor]),
            KeyCode::Esc | KeyCode::Char('q') => {
                break Err(anyhow::anyhow!("harness selection cancelled"))
            }
            _ => {}
        }
    };

    leave_tui()?;
    result
}

/// Model picker for `autoqa config`. Dispatches per harness: an arrow-key
/// list for harnesses with a small, doc-curated model set
/// (`Harness::model_choices`), a free-text prompt otherwise (copilot has no
/// scriptable model list; opencode's is 100+ dynamic entries — neither is
/// worth curating by hand).
pub fn pick_model(
    harness: crate::harness::Harness,
    current: Option<String>,
) -> anyhow::Result<String> {
    match harness.model_choices() {
        Some(choices) => pick_model_from_list(choices, current),
        None => prompt_text("Enter model", current),
    }
}

fn pick_model_from_list(choices: &[&str], current: Option<String>) -> anyhow::Result<String> {
    let mut term = enter_tui()?;
    let mut cursor = current
        .as_deref()
        .and_then(|c| choices.iter().position(|m| *m == c))
        .unwrap_or(0);

    let result = loop {
        term.draw(|f| {
            let area = f.area();
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(3), Constraint::Length(3)])
                .split(area);

            let items: Vec<ListItem> = choices
                .iter()
                .enumerate()
                .map(|(i, m)| {
                    let style = if i == cursor {
                        Style::default().add_modifier(Modifier::REVERSED)
                    } else {
                        Style::default()
                    };
                    let marker = if current.as_deref() == Some(*m) {
                        " (current)"
                    } else {
                        ""
                    };
                    ListItem::new(format!("{m}{marker}")).style(style)
                })
                .collect();
            f.render_widget(
                List::new(items).block(
                    UiBlock::default()
                        .borders(Borders::ALL)
                        .title("Pick a model"),
                ),
                rows[0],
            );

            let help = "↑/↓: move  Enter: select  q/Esc: cancel";
            f.render_widget(
                Paragraph::new(help)
                    .wrap(Wrap { trim: true })
                    .block(UiBlock::default().borders(Borders::ALL)),
                rows[1],
            );
        })?;

        if !event::poll(Duration::from_millis(200))? {
            continue;
        }
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        match key.code {
            KeyCode::Up => cursor = (cursor + choices.len() - 1) % choices.len(),
            KeyCode::Down => cursor = (cursor + 1) % choices.len(),
            KeyCode::Enter => break Ok(choices[cursor].to_string()),
            KeyCode::Esc | KeyCode::Char('q') => {
                break Err(anyhow::anyhow!("model selection cancelled"))
            }
            _ => {}
        }
    };

    leave_tui()?;
    result
}

/// Single-line free-text input screen — Enter submits (empty input keeps
/// `current` if any, otherwise re-prompts since a blank model is never
/// useful), Esc/q cancels.
fn prompt_text(title: &str, current: Option<String>) -> anyhow::Result<String> {
    let mut term = enter_tui()?;
    let mut buf = current.clone().unwrap_or_default();

    let result = loop {
        term.draw(|f| {
            let area = f.area();
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(3), Constraint::Length(3)])
                .split(area);

            f.render_widget(
                Paragraph::new(buf.as_str())
                    .block(UiBlock::default().borders(Borders::ALL).title(title)),
                rows[0],
            );
            let help = "Enter: confirm  q/Esc: cancel";
            f.render_widget(
                Paragraph::new(help)
                    .wrap(Wrap { trim: true })
                    .block(UiBlock::default().borders(Borders::ALL)),
                rows[1],
            );
        })?;

        if !event::poll(Duration::from_millis(200))? {
            continue;
        }
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        match key.code {
            KeyCode::Char(c) => buf.push(c),
            KeyCode::Backspace => {
                buf.pop();
            }
            KeyCode::Enter if !buf.trim().is_empty() => break Ok(buf.trim().to_string()),
            KeyCode::Esc => break Err(anyhow::anyhow!("model entry cancelled")),
            _ => {}
        }
    };

    leave_tui()?;
    result
}

/// A block chosen in the pre-run picker, in run order, with every
/// `{{placeholder}}` it needs bound to a param name — the agent is
/// instructed to replay these via `run_block` before touching the task.
pub struct PlannedBlock {
    pub slug: String,
    pub name: String,
    pub bindings: HashMap<String, String>,
}

fn placeholders_in(block: &Block) -> Vec<String> {
    let mut names = Vec::new();
    for step in &block.steps {
        for text in [&step.action, &step.assertion] {
            let mut rest = text.as_str();
            while let Some(start) = rest.find("{{") {
                let after = &rest[start + 2..];
                let Some(end) = after.find("}}") else { break };
                let name = after[..end].to_string();
                if !names.contains(&name) {
                    names.push(name);
                }
                rest = &after[end + 2..];
            }
        }
    }
    names
}

fn enter_tui() -> anyhow::Result<Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>> {
    enable_raw_mode()?;
    std::io::stdout().execute(EnterAlternateScreen)?;
    Ok(Terminal::new(ratatui::backend::CrosstermBackend::new(
        std::io::stdout(),
    ))?)
}

fn leave_tui() -> anyhow::Result<()> {
    disable_raw_mode()?;
    std::io::stdout().execute(LeaveAlternateScreen)?;
    Ok(())
}

enum Focus {
    Available,
    Plan,
}

struct EditingBindings {
    plan_index: usize,
    placeholders: Vec<String>,
    cursor: usize,
}

/// Pre-run screen: pick blocks + order them before the agent starts.
/// Returns an empty plan if the user starts the run with nothing selected
/// (`g`) — that's a normal, not a cancelled, run. `Esc`/`q` aborts the run
/// entirely (returns Err).
pub fn pick_blocks(
    available: &[(String, Block)],
    params: &[Param],
) -> anyhow::Result<Vec<PlannedBlock>> {
    if available.is_empty() {
        // Nothing to pick from — don't force the user through an empty
        // screen just to press 'g'.
        return Ok(Vec::new());
    }

    let mut term = enter_tui()?;
    let mut available_cursor = 0usize;
    let mut plan_cursor = 0usize;
    let mut plan: Vec<PlannedBlock> = Vec::new();
    let mut focus = Focus::Available;
    let mut editing: Option<EditingBindings> = None;

    let result = loop {
        term.draw(|f| {
            let area = f.area();
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(3), Constraint::Length(3)])
                .split(area);
            let cols = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(rows[0]);

            let available_items: Vec<ListItem> = available
                .iter()
                .enumerate()
                .map(|(i, (slug, b))| {
                    let style = if matches!(focus, Focus::Available) && i == available_cursor {
                        Style::default().add_modifier(Modifier::REVERSED)
                    } else {
                        Style::default()
                    };
                    ListItem::new(format!("{} ({slug})", b.name)).style(style)
                })
                .collect();
            f.render_widget(
                List::new(available_items)
                    .block(UiBlock::default().borders(Borders::ALL).title("Available blocks — Enter to add")),
                cols[0],
            );

            let plan_items: Vec<ListItem> = plan
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    let style = if matches!(focus, Focus::Plan) && i == plan_cursor {
                        Style::default().add_modifier(Modifier::REVERSED)
                    } else {
                        Style::default()
                    };
                    let unbound = p.bindings.len() < placeholder_count(available, &p.slug);
                    let marker = if unbound { " [needs bindings]" } else { "" };
                    ListItem::new(format!("{}. {}{}", i + 1, p.name, marker)).style(style)
                })
                .collect();
            f.render_widget(
                List::new(plan_items).block(
                    UiBlock::default()
                        .borders(Borders::ALL)
                        .title("Run plan (order) — J/K reorder, Enter to bind, x remove"),
                ),
                cols[1],
            );

            let help = "Tab: switch panel  ↑/↓: move  Enter: add/bind  J/K: reorder  x: remove  g: start run  q/Esc: cancel";
            f.render_widget(
                Paragraph::new(help).wrap(Wrap { trim: true }).block(UiBlock::default().borders(Borders::ALL)),
                rows[1],
            );

            if let Some(edit) = &editing {
                render_binding_popup(f, area, &plan[edit.plan_index], edit, params);
            }
        })?;

        if !event::poll(Duration::from_millis(200))? {
            continue;
        }
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }

        if let Some(edit) = &mut editing {
            match key.code {
                KeyCode::Esc => editing = None,
                KeyCode::Tab | KeyCode::Down => {
                    if !edit.placeholders.is_empty() {
                        edit.cursor = (edit.cursor + 1) % edit.placeholders.len();
                    }
                }
                KeyCode::Up => {
                    if !edit.placeholders.is_empty() {
                        edit.cursor =
                            (edit.cursor + edit.placeholders.len() - 1) % edit.placeholders.len();
                    }
                }
                KeyCode::Left | KeyCode::Right
                    if !edit.placeholders.is_empty() && !params.is_empty() =>
                {
                    let placeholder = edit.placeholders[edit.cursor].clone();
                    let current = plan[edit.plan_index].bindings.get(&placeholder).cloned();
                    let cur_idx = current
                        .as_ref()
                        .and_then(|name| params.iter().position(|p| &p.name == name));
                    let next_idx = match (key.code, cur_idx) {
                        (KeyCode::Right, None) => 0,
                        (KeyCode::Right, Some(i)) => (i + 1) % params.len(),
                        (KeyCode::Left, None) => params.len() - 1,
                        (KeyCode::Left, Some(i)) => (i + params.len() - 1) % params.len(),
                        _ => unreachable!(),
                    };
                    plan[edit.plan_index]
                        .bindings
                        .insert(placeholder, params[next_idx].name.clone());
                }
                _ => {}
            }
            continue;
        }

        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => break Err(anyhow::anyhow!("run cancelled")),
            KeyCode::Char('g') => break Ok(()),
            KeyCode::Tab => {
                focus = match focus {
                    Focus::Available => Focus::Plan,
                    Focus::Plan => Focus::Available,
                };
            }
            KeyCode::Up => match focus {
                Focus::Available if available_cursor > 0 => available_cursor -= 1,
                Focus::Plan if plan_cursor > 0 => plan_cursor -= 1,
                _ => {}
            },
            KeyCode::Down => match focus {
                Focus::Available if available_cursor + 1 < available.len() => available_cursor += 1,
                Focus::Plan if plan_cursor + 1 < plan.len() => plan_cursor += 1,
                _ => {}
            },
            KeyCode::Enter => match focus {
                Focus::Available => {
                    let (slug, block) = &available[available_cursor];
                    plan.push(PlannedBlock {
                        slug: slug.clone(),
                        name: block.name.clone(),
                        bindings: HashMap::new(),
                    });
                    plan_cursor = plan.len() - 1;
                }
                Focus::Plan => {
                    if let Some(p) = plan.get(plan_cursor) {
                        let placeholders = available
                            .iter()
                            .find(|(slug, _)| *slug == p.slug)
                            .map(|(_, b)| placeholders_in(b))
                            .unwrap_or_default();
                        if !placeholders.is_empty() {
                            editing = Some(EditingBindings {
                                plan_index: plan_cursor,
                                placeholders,
                                cursor: 0,
                            });
                        }
                    }
                }
            },
            KeyCode::Char('J') if matches!(focus, Focus::Plan) && plan_cursor + 1 < plan.len() => {
                plan.swap(plan_cursor, plan_cursor + 1);
                plan_cursor += 1;
            }
            KeyCode::Char('K') if matches!(focus, Focus::Plan) && plan_cursor > 0 => {
                plan.swap(plan_cursor, plan_cursor - 1);
                plan_cursor -= 1;
            }
            KeyCode::Char('x') if matches!(focus, Focus::Plan) && !plan.is_empty() => {
                plan.remove(plan_cursor);
                plan_cursor = plan_cursor.min(plan.len().saturating_sub(1));
            }
            _ => {}
        }
    };

    leave_tui()?;
    result.map(|()| plan)
}

fn placeholder_count(available: &[(String, Block)], slug: &str) -> usize {
    available
        .iter()
        .find(|(s, _)| s == slug)
        .map(|(_, b)| placeholders_in(b).len())
        .unwrap_or(0)
}

fn render_binding_popup(
    f: &mut ratatui::Frame,
    area: Rect,
    plan_item: &PlannedBlock,
    edit: &EditingBindings,
    params: &[Param],
) {
    let popup = Rect {
        x: area.width / 6,
        y: area.height / 3,
        width: area.width * 2 / 3,
        height: (edit.placeholders.len() as u16 + 3).min(area.height / 2),
    };
    let lines: Vec<Line> = edit
        .placeholders
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let bound = plan_item
                .bindings
                .get(name)
                .cloned()
                .unwrap_or_else(|| "—".to_string());
            let style = if i == edit.cursor {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default()
            };
            Line::from(Span::styled(format!("{{{{{name}}}}}  →  {bound}"), style))
        })
        .collect();
    let title = format!(
        "Bind placeholders for '{}' — ←/→ pick param, Esc done",
        plan_item.name
    );
    f.render_widget(ratatui::widgets::Clear, popup);
    f.render_widget(
        Paragraph::new(lines).block(UiBlock::default().borders(Borders::ALL).title(title)),
        popup,
    );
    let _ = params; // param names shown via the bound-value lookup above
}

/// Text prepended to the run query, instructing the agent to replay the
/// picked blocks (in order, with their resolved bindings) via `run_block`
/// before starting on the task itself.
pub fn render_plan_prefix(plan: &[PlannedBlock]) -> String {
    if plan.is_empty() {
        return String::new();
    }
    let mut out = String::from(
        "Before starting the task below, replay these blocks in this exact order via \
         mcp__autoqa-blocks__run_block, each with the bindings given:\n",
    );
    for (i, p) in plan.iter().enumerate() {
        let bindings = serde_json::to_string(&p.bindings).unwrap_or_else(|_| "{}".to_string());
        out.push_str(&format!(
            "{}. run_block(slug=\"{}\", bindings={})\n",
            i + 1,
            p.slug,
            bindings
        ));
    }
    out.push_str(
        "\nEach block's own steps are not shown to you — do not assume you know exactly what \
         it did. After a run_block call returns, take a browser_snapshot before doing anything \
         else, and treat the result as ground truth for what the page already has (already on \
         the right URL, items already added, etc.). Never blindly repeat a setup action \
         (navigation, an item add, a login) on the assumption it wasn't done yet — only act on \
         what the snapshot actually shows.\n\
         \nThen continue with the task:\n\n",
    );
    out
}

/// Live-progress screen: takes over the terminal for the duration of the
/// harness run, streaming its (already jq-filtered, or raw) output lines
/// into a scrolling pane instead of the plain stdout print `cmd_run` used
/// to do. Blocks until the child exits, then waits for a keypress before
/// restoring the terminal.
pub fn run_live(
    mut child: std::process::Child,
    log_filter: Option<&'static str>,
) -> anyhow::Result<std::process::ExitStatus> {
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let child_stdout = child.stdout.take().expect("stdout was piped");
    let child_stderr = child.stderr.take().expect("stderr was piped");

    // Otherwise inherited straight to the real terminal, invisibly, since
    // the alternate screen covers it — any failure (npx fetch, MCP
    // handshake, harness auth) would silently vanish instead of explaining
    // why the pane never shows anything.
    let tx_err = tx.clone();
    let stderr_reader = std::thread::spawn(move || {
        use std::io::BufRead;
        for line in std::io::BufReader::new(child_stderr)
            .lines()
            .map_while(Result::ok)
        {
            let _ = tx_err.send(format!("[stderr] {line}"));
        }
    });

    let tx_reader_err = tx.clone();
    let reader = std::thread::spawn(move || {
        if let Err(e) = read_stdout(child_stdout, log_filter, &tx) {
            let _ = tx_reader_err.send(format!("[reader error] {e}"));
        }
    });
    fn read_stdout(
        child_stdout: std::process::ChildStdout,
        log_filter: Option<&'static str>,
        tx: &std::sync::mpsc::Sender<String>,
    ) -> anyhow::Result<()> {
        use std::io::BufRead;
        match log_filter {
            Some(filter) => {
                // --unbuffered: jq block-buffers stdout by default when it's
                // not a TTY (which it isn't here — we pipe it ourselves to
                // feed the live pane) — without this, nothing shows up until
                // jq's buffer flushes at process exit.
                let mut jq = std::process::Command::new("jq")
                    .args(["--unbuffered", "-r", filter])
                    .stdin(std::process::Stdio::piped())
                    .stdout(std::process::Stdio::piped())
                    .spawn()?;
                let mut jq_stdin = jq.stdin.take().unwrap();
                let jq_stdout = jq.stdout.take().unwrap();
                let tx_jq = tx.clone();
                let pump = std::thread::spawn(move || {
                    for line in std::io::BufReader::new(jq_stdout)
                        .lines()
                        .map_while(Result::ok)
                    {
                        let _ = tx_jq.send(line);
                    }
                });
                for line in std::io::BufReader::new(child_stdout)
                    .lines()
                    .map_while(Result::ok)
                {
                    if line.trim_start().starts_with('{') {
                        let _ = writeln!(jq_stdin, "{line}");
                    }
                }
                drop(jq_stdin);
                let _ = pump.join();
                let _ = jq.wait();
            }
            None => {
                for line in std::io::BufReader::new(child_stdout)
                    .lines()
                    .map_while(Result::ok)
                {
                    let _ = tx.send(line);
                }
            }
        }
        Ok(())
    }

    let mut term = enter_tui()?;
    let mut lines: Vec<String> = Vec::new();
    let status = loop {
        while let Ok(line) = rx.try_recv() {
            lines.push(line);
        }
        term.draw(|f| {
            let area = f.area();
            let visible = area.height.saturating_sub(2) as usize;
            let start = lines.len().saturating_sub(visible);
            let text = lines[start..].join("\n");
            f.render_widget(
                Paragraph::new(text).wrap(Wrap { trim: false }).block(
                    UiBlock::default()
                        .borders(Borders::ALL)
                        .title("autoqa run — live progress"),
                ),
                area,
            );
        })?;

        if let Ok(Some(status)) = child.try_wait() {
            break status;
        }
        let _ = event::poll(Duration::from_millis(100))?;
    };
    let _ = reader.join();
    let _ = stderr_reader.join();

    // Drain anything still buffered after the child exited, then leave the
    // final output on screen until the user acknowledges it.
    while let Ok(line) = rx.try_recv() {
        lines.push(line);
    }
    term.draw(|f| {
        let area = f.area();
        let footer = if status.success() {
            "✅ done — press any key to exit"
        } else {
            "❌ harness failed — press any key to exit"
        };
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(3), Constraint::Length(1)])
            .split(area);
        let visible = rows[0].height.saturating_sub(2) as usize;
        let start = lines.len().saturating_sub(visible);
        let text = lines[start..].join("\n");
        f.render_widget(
            Paragraph::new(text).wrap(Wrap { trim: false }).block(
                UiBlock::default()
                    .borders(Borders::ALL)
                    .title("autoqa run — done"),
            ),
            rows[0],
        );
        f.render_widget(Paragraph::new(footer), rows[1]);
    })?;
    loop {
        if let Event::Key(k) = event::read()? {
            if k.kind == KeyEventKind::Press {
                break;
            }
        }
    }

    leave_tui()?;
    Ok(status)
}
