<p align="center">
  <a href="https://github.com/MerzoukeMansouri/homebrew">
    <img src="https://cdn.simpleicons.org/rust/000000/FFFFFF" width="48" height="48" alt="Rust">
  </a>
  &nbsp;&nbsp;
  <a href="https://github.com/MerzoukeMansouri/homebrew">
    <img src="https://cdn.simpleicons.org/react/61DAFB" width="48" height="48" alt="React">
  </a>
  &nbsp;&nbsp;
  <a href="https://github.com/MerzoukeMansouri/homebrew">
    <img src="https://cdn.simpleicons.org/homebrew/FBB040" width="48" height="48" alt="Homebrew">
  </a>
</p>

# cu-agent

A Rust CLI (`cua`) that drives a real Chrome browser via CDP, plus a Claude Code
Skill that teaches Claude how to use it. Reasoning/orchestration is delegated
entirely to `claude -p` (headless Claude Code) — this project has no agent
loop of its own.

## Usage

Install via Homebrew:

```
brew install MerzoukeMansouri/homebrew/cua
```

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

### Playwright test generation

Every action `cua` executes (whether run by hand or via `cua run`) is
captured with a DOM selector and a per-action screenshot into
`~/.cu-agent/actions.json`. Review and edit that session, then generate a
Playwright test from it:

```
cua review              # opens a local dark-themed UI at localhost:4321
                         # — edit/delete actions, add assertions, click Validate
```

Or skip the UI entirely for scripting/CI:

```
cua codegen --out my-test.spec.ts   # reads actions.json directly, no UI/browser needed
```

Selectors are picked `#id` > `[data-testid=...]` (also `data-test`/`data-cy`/
`data-qa`) > `.class` > a CSS `nth-child` path — never XPath — each tier
verified unique against the live DOM at capture time. Assertions: an
`await expect(page).toHaveURL(...)` is auto-inserted after every navigation;
add your own `visible`/`text`/`value` checks per-element from the review UI.

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
