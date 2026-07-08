---
name: browser-agent
description: Drive a real Chrome browser via the `cua` CLI — click, type, scroll, navigate, drag. Use whenever the task requires interacting with a live website: filling forms, clicking through a UI, reading rendered page content, testing a web flow, or any request that needs an actual browser rather than fetched HTML.
---

# Browser Agent

You control a real Chrome browser through the `cua` command-line tool (run it with Bash — build it once with `cargo build --release` in this project if `target/release/cua` doesn't exist yet, then invoke it as `./target/release/cua`). Every `cua` action command (all except `cua screenshot`) performs an action AND returns the resulting page state as JSON on stdout — `{"url": ..., "screenshot_path": ..., "viewport": [w, h]}` — so you see the effect immediately without a separate screenshot call.

Note: the binary is named `cua`, not `cu` — `cu` is a preexisting Unix command (serial dial-out utility) on most systems and would silently run the wrong program.

## Coordinate system

All coordinates are **raw CSS pixels** on the actual browser viewport — NOT normalized to a 0-1000 scale. The `viewport` field in every response gives you the current viewport size in pixels; read coordinates for clicks/scrolls directly off the screenshot at that same resolution.

## Workflow

1. If no session exists yet, run `cua open [url]` to launch Chrome. If `cua open` fails with "a session is already open", one was already started for you (e.g. by `cua run`) — use `cua navigate <url>` instead of retrying `open`.
2. Read the `screenshot_path` from the JSON output with the Read tool to see the current page.
3. Decide the next action and run the matching `cua` subcommand below.
4. Read the new `screenshot_path` it returns — do not call `cua screenshot` separately unless you just want to look without acting.
5. Repeat until the task is done, then summarize what happened.
6. Do not run `cua close` yourself — the harness that invoked this session owns teardown.

If a click seems to have missed (URL/screenshot unchanged when you expected a change), re-read the fresh screenshot and re-measure the target's pixel position rather than guessing again — text and buttons don't always sit where they look at a glance.

## Command reference

| Command | Effect |
|---|---|
| `cua open [url]` | Launch Chrome, optionally navigate to `url` |
| `cua screenshot` | Screenshot the current page without acting |
| `cua click <x> <y>` | Left-click at pixel (x, y) |
| `cua double-click <x> <y>` | Double-click |
| `cua triple-click <x> <y>` | Triple-click (selects a whole line/paragraph in most editors) |
| `cua right-click <x> <y>` | Right-click (opens context menus) |
| `cua middle-click <x> <y>` | Middle-click (e.g. opens a link in a new tab) |
| `cua mouse-down <x> <y>` / `cua mouse-up <x> <y>` | Press/release the left button without the paired action — for custom drag sequences |
| `cua hover <x> <y>` | Move the mouse to (x, y) without clicking — reveals hover menus |
| `cua drag <x1> <y1> <x2> <y2>` | Press at (x1,y1), drag to (x2,y2), release |
| `cua type "<text>" [--enter]` | Insert text into the focused element; `--enter` presses Enter after |
| `cua key "<combo>"` | Press a key or combo, e.g. `Enter`, `Escape`, `control+a`, `control+c` |
| `cua key-down "<key>"` / `cua key-up "<key>"` | Hold/release a single key — for gestures that need a key held across other actions |
| `cua scroll <x> <y> <up\|down\|left\|right> [magnitude]` | Scroll at (x, y) in a direction by magnitude in pixels (default 800) |
| `cua navigate <url>` | Go directly to a URL |
| `cua back` / `cua forward` | Browser history navigation |
| `cua wait <seconds>` | Pause (e.g. while a page loads) |

## Tips

- Prefer `cua navigate` over clicking a URL bar — there is no URL bar in this headless-style setup; `navigate` is the direct equivalent.
- A single click on a focused text field usually doesn't clear it — use `cua key "control+a"` then `cua type "..."` to replace field contents.
- `cua type` inserts the given text as-is at the focused element — click the field first so it has focus.
- Give a page a moment after `cua navigate` before acting — `cua` already waits briefly for navigation and rendering to settle, but slow-loading pages may need an explicit `cua wait 1`.
- Only one tab is tracked at a time. If an action opens a new tab (e.g. a `target=_blank` link), `cua` automatically folds it back into the tracked tab and closes the extra — you don't need to handle multi-tab bookkeeping yourself.
