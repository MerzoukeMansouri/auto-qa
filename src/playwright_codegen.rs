use crate::action_entry::ActionEntry;

fn esc(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\'', "\\'")
}

fn scroll_delta(direction: &str, magnitude: f64) -> (f64, f64) {
    match direction {
        "up" => (0.0, -magnitude),
        "down" => (0.0, magnitude),
        "left" => (-magnitude, 0.0),
        "right" => (magnitude, 0.0),
        _ => (0.0, 0.0),
    }
}

/// Generates a self-contained Playwright TS test from a captured/edited
/// action array. Coordinate-only fallbacks (no selector captured) are
/// emitted with a trailing "edit me" comment rather than silently dropped.
pub fn generate(entries: &[ActionEntry]) -> String {
    let mut lines: Vec<String> = Vec::new();

    for e in entries {
        if let Some(shot) = &e.screenshot {
            lines.push(format!("  // screenshot: {shot}"));
        }
        match e.kind.as_str() {
            "navigate" => {
                if let Some(url) = &e.url {
                    lines.push(format!("  await page.goto('{}');", esc(url)));
                    lines.push(format!("  await expect(page).toHaveURL('{}');", esc(url)));
                }
            }
            "click" | "double_click" | "triple_click" | "right_click" | "middle_click" => {
                if let Some(sel) = &e.selector {
                    let call = match e.kind.as_str() {
                        "double_click" => format!("page.dblclick('{}')", esc(sel)),
                        "triple_click" => {
                            format!("page.click('{}', {{ clickCount: 3 }})", esc(sel))
                        }
                        "right_click" => {
                            format!("page.click('{}', {{ button: 'right' }})", esc(sel))
                        }
                        "middle_click" => {
                            lines.push(format!(
                                "  await page.hover('{}'); // middle-click has no Playwright locator method",
                                esc(sel)
                            ));
                            format!(
                                "page.mouse.click({}, {}, {{ button: 'middle' }})",
                                e.x.unwrap_or(0.0),
                                e.y.unwrap_or(0.0)
                            )
                        }
                        _ => format!("page.click('{}')", esc(sel)),
                    };
                    lines.push(format!("  await {call};"));
                } else {
                    lines.push(format!(
                        "  await page.mouse.click({}, {}); // no selector captured, edit me",
                        e.x.unwrap_or(0.0),
                        e.y.unwrap_or(0.0)
                    ));
                }
            }
            "hover" => {
                if let Some(sel) = &e.selector {
                    lines.push(format!("  await page.hover('{}');", esc(sel)));
                } else {
                    lines.push(format!(
                        "  await page.mouse.move({}, {}); // no selector captured, edit me",
                        e.x.unwrap_or(0.0),
                        e.y.unwrap_or(0.0)
                    ));
                }
            }
            "drag" => {
                // Only the drag-start element is captured (single `selector`
                // field); coordinate-based drag is always correct and simpler
                // than a half-supported dragAndDrop path.
                lines.push(format!(
                    "  await page.mouse.move({}, {});",
                    e.x.unwrap_or(0.0),
                    e.y.unwrap_or(0.0)
                ));
                lines.push("  await page.mouse.down();".to_string());
                lines.push(format!(
                    "  await page.mouse.move({}, {});",
                    e.x2.unwrap_or(0.0),
                    e.y2.unwrap_or(0.0)
                ));
                lines.push("  await page.mouse.up();".to_string());
            }
            "type" => {
                let value = e.value.clone().unwrap_or_default();
                if let Some(sel) = &e.selector {
                    lines.push(format!(
                        "  await page.fill('{}', '{}');",
                        esc(sel),
                        esc(&value)
                    ));
                } else {
                    lines.push(format!(
                        "  await page.keyboard.type('{}'); // no selector captured, edit me",
                        esc(&value)
                    ));
                }
                if e.enter.unwrap_or(false) {
                    lines.push("  await page.keyboard.press('Enter');".to_string());
                }
            }
            "key" => {
                if let Some(combo) = &e.combo {
                    lines.push(format!("  await page.keyboard.press('{}');", esc(combo)));
                }
            }
            "key_down" => {
                if let Some(combo) = &e.combo {
                    lines.push(format!("  await page.keyboard.down('{}');", esc(combo)));
                }
            }
            "key_up" => {
                if let Some(combo) = &e.combo {
                    lines.push(format!("  await page.keyboard.up('{}');", esc(combo)));
                }
            }
            "scroll" => {
                let (dx, dy) = scroll_delta(
                    e.direction.as_deref().unwrap_or(""),
                    e.magnitude.unwrap_or(800.0),
                );
                lines.push(format!("  await page.mouse.wheel({dx}, {dy});"));
            }
            "back" => lines.push("  await page.goBack();".to_string()),
            "forward" => lines.push("  await page.goForward();".to_string()),
            "wait" => {
                let ms = e.seconds.unwrap_or(1) * 1000;
                lines.push(format!("  await page.waitForTimeout({ms});"));
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
