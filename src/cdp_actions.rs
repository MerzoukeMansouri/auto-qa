use chromiumoxide::cdp::browser_protocol::input::{
    DispatchKeyEventParams, DispatchKeyEventType, DispatchMouseEventParams, DispatchMouseEventType,
    InsertTextParams, MouseButton,
};
use chromiumoxide::keys::get_key_definition;
use chromiumoxide::Page;
use serde::Deserialize;

#[derive(Deserialize, Clone)]
pub struct ElementInfo {
    pub selector: Option<String>,
    pub tag: String,
    pub text: String,
}

/// Builds a selector for `el`: id > data-testid family > class > nth-child
/// CSS path. Every tier is uniqueness-checked with an actual
/// `querySelectorAll(...).length === 1` before being accepted — never a guess,
/// never XPath. The nth-child path fallback always succeeds.
const SELECTOR_BUILDER_JS: &str = r#"
function cuaBuildSelector(el) {
  function unique(sel) {
    try { return document.querySelectorAll(sel).length === 1; } catch (e) { return false; }
  }
  if (el.id) {
    const sel = '#' + CSS.escape(el.id);
    if (unique(sel)) return sel;
  }
  for (const attr of ['data-testid', 'data-test', 'data-cy', 'data-qa']) {
    const v = el.getAttribute(attr);
    if (v) {
      const sel = '[' + attr + '="' + CSS.escape(v) + '"]';
      if (unique(sel)) return sel;
    }
  }
  if (el.className && typeof el.className === 'string') {
    const hashLike = /^[a-z0-9_-]{6,}$/;
    const cls = el.className.split(/\s+/).filter(c => c && !hashLike.test(c));
    if (cls.length) {
      const sel = el.tagName.toLowerCase() + '.' + cls.map(c => CSS.escape(c)).join('.');
      if (unique(sel)) return sel;
    }
  }
  let path = [];
  let node = el;
  while (node && node.nodeType === 1 && node.tagName !== 'BODY') {
    const parent = node.parentElement;
    if (!parent) break;
    const idx = Array.from(parent.children).indexOf(node) + 1;
    path.unshift(node.tagName.toLowerCase() + ':nth-child(' + idx + ')');
    node = parent;
    if (unique(path.join(' > '))) break;
  }
  return path.length ? path.join(' > ') : null;
}
function cuaDescribe(el) {
  if (!el) return null;
  return {
    selector: cuaBuildSelector(el),
    tag: el.tagName.toLowerCase(),
    text: (el.textContent || '').trim().slice(0, 80),
  };
}
"#;

pub async fn describe_element_at(
    page: &Page,
    x: f64,
    y: f64,
) -> anyhow::Result<Option<ElementInfo>> {
    // Wrapped in an IIFE starting with `(` — chromiumoxide's `evaluate` peeks
    // at the string to guess expression-vs-function, and a snippet starting
    // with the bare `function` keyword gets misdetected as a callable and
    // wrapped again, producing a syntax error. `(function(){...})()` reads
    // as a plain expression.
    let js = format!(
        "(function(){{ {SELECTOR_BUILDER_JS}\nreturn cuaDescribe(document.elementFromPoint({x}, {y})); }})()"
    );
    let info = page
        .evaluate(js)
        .await?
        .into_value::<Option<ElementInfo>>()?;
    Ok(info)
}

pub async fn describe_active_element(page: &Page) -> anyhow::Result<Option<ElementInfo>> {
    let js = format!(
        "(function(){{ {SELECTOR_BUILDER_JS}\nreturn cuaDescribe(document.activeElement); }})()"
    );
    let info = page
        .evaluate(js)
        .await?
        .into_value::<Option<ElementInfo>>()?;
    Ok(info)
}

/// chromiumoxide's key table is looked up by exact-case `KeyboardEvent.key`
/// name (e.g. "Control", "Enter") — normalize common lowercase aliases to it.
fn canonical_key_name(key: &str) -> String {
    match key.to_lowercase().as_str() {
        "control" | "ctrl" => "Control".into(),
        "alt" | "option" => "Alt".into(),
        "meta" | "command" | "cmd" | "super" | "windows" | "win" => "Meta".into(),
        "shift" => "Shift".into(),
        "enter" | "return" => "Enter".into(),
        "escape" | "esc" => "Escape".into(),
        "backspace" => "Backspace".into(),
        "delete" | "del" => "Delete".into(),
        "tab" => "Tab".into(),
        "space" => " ".into(),
        "up" | "arrowup" => "ArrowUp".into(),
        "down" | "arrowdown" => "ArrowDown".into(),
        "left" | "arrowleft" => "ArrowLeft".into(),
        "right" | "arrowright" => "ArrowRight".into(),
        "home" => "Home".into(),
        "end" => "End".into(),
        "pageup" => "PageUp".into(),
        "pagedown" => "PageDown".into(),
        // Single characters and anything else already in canonical form pass through as-is.
        _ => key.to_string(),
    }
}

fn modifier_bit(name: &str) -> i64 {
    match name.to_lowercase().as_str() {
        "alt" => 1,
        "control" | "ctrl" => 2,
        "meta" | "command" | "cmd" | "super" => 4,
        "shift" => 8,
        _ => 0,
    }
}

async fn mouse_event(
    page: &Page,
    x: f64,
    y: f64,
    kind: DispatchMouseEventType,
    button: MouseButton,
    click_count: i64,
) -> anyhow::Result<()> {
    page.execute(
        DispatchMouseEventParams::builder()
            .r#type(kind)
            .x(x)
            .y(y)
            .button(button)
            .click_count(click_count)
            .build()
            .map_err(|e| anyhow::anyhow!(e))?,
    )
    .await?;
    Ok(())
}

async fn click_n(
    page: &Page,
    x: f64,
    y: f64,
    button: MouseButton,
    count: i64,
) -> anyhow::Result<()> {
    mouse_event(
        page,
        x,
        y,
        DispatchMouseEventType::MousePressed,
        button.clone(),
        count,
    )
    .await?;
    mouse_event(
        page,
        x,
        y,
        DispatchMouseEventType::MouseReleased,
        button,
        count,
    )
    .await?;
    Ok(())
}

pub async fn click(page: &Page, x: f64, y: f64) -> anyhow::Result<()> {
    click_n(page, x, y, MouseButton::Left, 1).await
}

pub async fn double_click(page: &Page, x: f64, y: f64) -> anyhow::Result<()> {
    click_n(page, x, y, MouseButton::Left, 2).await
}

pub async fn triple_click(page: &Page, x: f64, y: f64) -> anyhow::Result<()> {
    click_n(page, x, y, MouseButton::Left, 3).await
}

pub async fn right_click(page: &Page, x: f64, y: f64) -> anyhow::Result<()> {
    click_n(page, x, y, MouseButton::Right, 1).await
}

pub async fn middle_click(page: &Page, x: f64, y: f64) -> anyhow::Result<()> {
    click_n(page, x, y, MouseButton::Middle, 1).await
}

pub async fn mouse_down(page: &Page, x: f64, y: f64) -> anyhow::Result<()> {
    mouse_event(
        page,
        x,
        y,
        DispatchMouseEventType::MousePressed,
        MouseButton::Left,
        1,
    )
    .await
}

pub async fn mouse_up(page: &Page, x: f64, y: f64) -> anyhow::Result<()> {
    mouse_event(
        page,
        x,
        y,
        DispatchMouseEventType::MouseReleased,
        MouseButton::Left,
        1,
    )
    .await
}

pub async fn hover(page: &Page, x: f64, y: f64) -> anyhow::Result<()> {
    mouse_event(
        page,
        x,
        y,
        DispatchMouseEventType::MouseMoved,
        MouseButton::None,
        0,
    )
    .await
}

pub async fn drag(page: &Page, x1: f64, y1: f64, x2: f64, y2: f64) -> anyhow::Result<()> {
    mouse_event(
        page,
        x1,
        y1,
        DispatchMouseEventType::MousePressed,
        MouseButton::Left,
        1,
    )
    .await?;
    // A couple of intermediate moves for drag-sensitive UI that ignores a single jump.
    let steps = 5;
    for i in 1..=steps {
        let t = i as f64 / steps as f64;
        let x = x1 + (x2 - x1) * t;
        let y = y1 + (y2 - y1) * t;
        mouse_event(
            page,
            x,
            y,
            DispatchMouseEventType::MouseMoved,
            MouseButton::Left,
            0,
        )
        .await?;
    }
    mouse_event(
        page,
        x2,
        y2,
        DispatchMouseEventType::MouseReleased,
        MouseButton::Left,
        1,
    )
    .await
}

pub async fn scroll(
    page: &Page,
    x: f64,
    y: f64,
    direction: &str,
    magnitude: f64,
) -> anyhow::Result<()> {
    let (delta_x, delta_y) = match direction {
        "up" => (0.0, -magnitude),
        "down" => (0.0, magnitude),
        "left" => (-magnitude, 0.0),
        "right" => (magnitude, 0.0),
        other => anyhow::bail!("unknown scroll direction: {other}"),
    };
    page.execute(
        DispatchMouseEventParams::builder()
            .r#type(DispatchMouseEventType::MouseWheel)
            .x(x)
            .y(y)
            .delta_x(delta_x)
            .delta_y(delta_y)
            .build()
            .map_err(|e| anyhow::anyhow!(e))?,
    )
    .await?;
    Ok(())
}

/// Dispatch a single key press (down+up), optionally with modifier keys held for the duration.
async fn press_key_with_modifiers(page: &Page, key: &str, modifiers: i64) -> anyhow::Result<()> {
    let canon = canonical_key_name(key);
    let def = get_key_definition(&canon).ok_or_else(|| anyhow::anyhow!("unknown key: {key}"))?;

    let mut builder = DispatchKeyEventParams::builder()
        .key(def.key)
        .code(def.code)
        .windows_virtual_key_code(def.key_code)
        .native_virtual_key_code(def.key_code)
        .modifiers(modifiers);

    let key_down_type = if let Some(txt) = def.text {
        builder = builder.clone().text(txt);
        DispatchKeyEventType::KeyDown
    } else if def.key.len() == 1 {
        builder = builder.clone().text(def.key);
        DispatchKeyEventType::KeyDown
    } else {
        DispatchKeyEventType::RawKeyDown
    };

    page.execute(
        builder
            .clone()
            .r#type(key_down_type)
            .build()
            .map_err(|e| anyhow::anyhow!(e))?,
    )
    .await?;
    page.execute(
        builder
            .r#type(DispatchKeyEventType::KeyUp)
            .build()
            .map_err(|e| anyhow::anyhow!(e))?,
    )
    .await?;
    Ok(())
}

/// Presses a single key or a `+`-joined combo (e.g. "control+a", "Enter").
pub async fn key(page: &Page, combo: &str) -> anyhow::Result<()> {
    let parts: Vec<&str> = combo.split('+').map(str::trim).collect();
    let (main_key, modifier_keys) = parts
        .split_last()
        .ok_or_else(|| anyhow::anyhow!("empty key combo"))?;
    let modifiers: i64 = modifier_keys.iter().map(|m| modifier_bit(m)).sum();

    for m in modifier_keys {
        key_down(page, m).await?;
    }
    press_key_with_modifiers(page, main_key, modifiers).await?;
    for m in modifier_keys.iter().rev() {
        key_up(page, m).await?;
    }
    Ok(())
}

pub async fn key_down(page: &Page, key: &str) -> anyhow::Result<()> {
    let canon = canonical_key_name(key);
    let def = get_key_definition(&canon).ok_or_else(|| anyhow::anyhow!("unknown key: {key}"))?;
    let mut builder = DispatchKeyEventParams::builder()
        .r#type(DispatchKeyEventType::KeyDown)
        .key(def.key)
        .code(def.code)
        .windows_virtual_key_code(def.key_code)
        .native_virtual_key_code(def.key_code);
    if let Some(txt) = def.text {
        builder = builder.text(txt);
    }
    page.execute(builder.build().map_err(|e| anyhow::anyhow!(e))?)
        .await?;
    Ok(())
}

pub async fn key_up(page: &Page, key: &str) -> anyhow::Result<()> {
    let canon = canonical_key_name(key);
    let def = get_key_definition(&canon).ok_or_else(|| anyhow::anyhow!("unknown key: {key}"))?;
    let builder = DispatchKeyEventParams::builder()
        .r#type(DispatchKeyEventType::KeyUp)
        .key(def.key)
        .code(def.code)
        .windows_virtual_key_code(def.key_code)
        .native_virtual_key_code(def.key_code);
    page.execute(builder.build().map_err(|e| anyhow::anyhow!(e))?)
        .await?;
    Ok(())
}

pub async fn type_text(page: &Page, text: &str, press_enter: bool) -> anyhow::Result<()> {
    // Input.insertText handles arbitrary unicode in one call — no per-key lookup table needed.
    page.execute(InsertTextParams::new(text)).await?;
    if press_enter {
        key(page, "Enter").await?;
    }
    Ok(())
}

pub async fn navigate(page: &Page, url: &str) -> anyhow::Result<()> {
    let url = if url.starts_with("http://") || url.starts_with("https://") {
        url.to_string()
    } else {
        format!("https://{url}")
    };
    page.goto(url).await?;
    page.wait_for_navigation().await?;
    Ok(())
}

pub async fn back(page: &Page) -> anyhow::Result<()> {
    page.evaluate("history.back()").await?;
    Ok(())
}

pub async fn forward(page: &Page) -> anyhow::Result<()> {
    page.evaluate("history.forward()").await?;
    Ok(())
}

pub async fn wait(seconds: u64) -> anyhow::Result<()> {
    tokio::time::sleep(std::time::Duration::from_secs(seconds)).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_key_name_normalizes_aliases() {
        assert_eq!(canonical_key_name("ctrl"), "Control");
        assert_eq!(canonical_key_name("CMD"), "Meta");
        assert_eq!(canonical_key_name("esc"), "Escape");
        assert_eq!(canonical_key_name("arrowup"), "ArrowUp");
        assert_eq!(canonical_key_name("space"), " ");
        assert_eq!(canonical_key_name("a"), "a");
    }

    #[test]
    fn modifier_bit_matches_dispatchkeyevent_bitmask() {
        assert_eq!(modifier_bit("alt"), 1);
        assert_eq!(modifier_bit("ctrl"), 2);
        assert_eq!(modifier_bit("cmd"), 4);
        assert_eq!(modifier_bit("shift"), 8);
        assert_eq!(modifier_bit("unknown"), 0);
    }
}
