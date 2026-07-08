# cu-agent

A Rust CLI (`cua`) that drives a real Chrome browser via CDP, plus a Claude Code
Skill that teaches Claude how to use it. Reasoning/orchestration is delegated
entirely to `claude -p` (headless Claude Code) — this project has no agent
loop of its own.

Rebuilt from the Gemini-based Python reference at
`../computer-use-preview` (`agent.py` + `computers/`), keeping the same
action set but replacing the hand-rolled retry/dispatch loop with Claude
Code's own agentic tool-use loop.

## Setup

Requires Google Chrome installed locally (macOS default path is used
automatically; set `CHROME_PATH` to override, or on Linux for
`google-chrome`/`chromium`/`chromium-browser`).

```
cargo build --release
```

The binary is named **`cua`**, not `cu` — `cu` is a preexisting Unix command
(serial dial-out utility, `/usr/bin/cu` on macOS/Linux) and using that name
would silently invoke the wrong program.

## Usage

Standalone CLI, one Chrome session at a time (state lives in `~/.cu-agent/`):

```
cua open https://example.com
cua click 280 243
cua type "hello" --enter
cua screenshot
cua close
```

Full end-to-end, letting Claude Code drive it via the skill:

```
cua run --query "go to example.com and tell me the page title"
```

`cua run` must be invoked from inside this project directory (or a directory
with its own `.claude/skills/browser-agent/`) so Claude Code discovers the
skill. Requires `jq` (`brew install jq`) — used to turn Claude's raw NDJSON
event stream into readable step-by-step output (thinking, each tool call,
tool results, final answer) instead of a wall of raw JSON.

## Explicitly descoped (vs. the Python reference)

Documented here rather than silently dropped:

- **Legacy Gemini-model action names** (`open_web_browser`, `click_at`,
  `scroll_document`, `wait_5_seconds`, `search`) — reference-only for older
  Gemini model compatibility; `cua`'s action set targets one interface.
- **`safety_decision` interactive confirmation** — a Gemini-API-specific
  safety-service concept. Claude Code has its own permission-prompt system
  (`--permission-mode`) covering the analogous concern.
- **`BrowserbaseComputer` (remote managed browser)** — `cua` launches/connects
  to a local Chrome only. `Browser::connect` already supports arbitrary CDP
  URLs, so a `--connect-url` mode is a small future extension, not a redesign.
- **`highlight_mouse` visual debug circle** — cosmetic, skip for v1.
- **Screenshot-pruning in conversation history** — a Gemini-context-management
  concern; irrelevant here since Claude Code manages its own session context.
- **`multiply_numbers` custom function example** — reference-only demo of
  custom tool wiring, not part of the browser feature set.

## Screenshot cost

Two optimizations keep vision-model cost/latency down per loop iteration:

- **JPEG, quality 75** instead of lossless PNG (`browser::take_screenshot`) —
  screenshots are mostly flat UI/text, which compresses well; smaller file,
  faster to write/read/base64.
- **Viewport pinned to 1024×768** at `cua open` (`browser::set_viewport`, via
  `Emulation.setDeviceMetricsOverride`) regardless of the host display's real
  resolution — vision-model token cost scales with image pixel dimensions,
  not file size, so this is what actually bounds tokens per screenshot.

## Observability

- Every action `cua` executes is appended to `~/.cu-agent/log.jsonl`
  (timestamp, action + args, resulting url) — an audit trail independent of
  whatever drove it.
- `cua run` shows Claude's reasoning and every tool call live (see Usage
  above), via `--output-format stream-json --verbose` piped through `jq`.
  Plain `--verbose` with the default text output shows nothing extra —
  `stream-json` is what actually emits per-step events, `--verbose` is just
  required alongside it.

## Notes from implementation

- Chrome's CDP target-discovery events arrive asynchronously right after
  `Browser::connect()` — actions retry briefly rather than assuming the page
  list is populated immediately.
- A fresh Chrome launch opens its own default New Tab Page tab in addition to
  the one `cua open` creates; it's closed immediately so later single-tab
  enforcement doesn't mistake it for a tab an agent action opened.
- Every mutating action does a bounded wait-for-navigation before
  screenshotting, since a click can trigger an async page load that isn't
  finished yet when the CDP call returns.
- Pages must be created via the plain HTTP `/json/new` endpoint
  (`browser::create_page_via_http`), not chromiumoxide's WebSocket-session
  `new_page()` — Chrome resets/closes a tab once the CDP session that
  attached to it fully disconnects, which happens every time a short-lived
  `cua` process exits. A tab created over HTTP has no such owning session.
- Every `cua` process sends an explicit `Target.detachFromTarget`
  (`browser::detach`) before exiting, instead of letting the WebSocket drop
  abruptly — an abrupt disconnect is what appeared to trigger Chrome's
  tab-reset behavior in the first place.
