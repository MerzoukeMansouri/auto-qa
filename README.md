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

A Rust CLI (`cua`) that drives a real Chrome browser through `claude -p` +
[Playwright MCP](https://github.com/microsoft/playwright-mcp), records the
session, and turns it into a real Playwright test. Reasoning/orchestration is
delegated entirely to `claude -p` (headless Claude Code) — this project has no
agent loop of its own, and no browser-automation code of its own either;
Playwright MCP owns the browser.

## Usage

Install via Homebrew:

```
brew tap MerzoukeMansouri/homebrew
brew install cua
```

Drive a browser end-to-end from a natural-language query:

```
cua run --query "go to example.com and tell me the page title"
```

`cua run` spawns `claude -p` wired to Playwright MCP (`npx @playwright/mcp@latest`,
launched fresh and isolated per run — no leftover cookies/localStorage between
runs). It shows Claude's reasoning and every tool call live, piped through
`jq` (`brew install jq`) for readable step-by-step output instead of a wall of
raw NDJSON. The model is instructed to actually drive the page (never answer
from memory) and to verify the outcome of every action — not just the ones
you happen to ask it to check — so the recorded session has real assertions,
not just a chain of clicks.

### Turning a session into a Playwright test

Playwright MCP's `--save-session` records every tool call's ready-made JS
statement (`await page.goto(...)`, `await expect(...).toBeVisible()`, ...).
`cua` parses that into an editable step list — one `{action, assertion}` pair
per step — and can turn it into a real `.spec.ts`:

```
cua review              # opens a local dark-themed UI at localhost:4321
                         # — edit/reorder/insert/delete steps, chat with an
                         # LLM to add or fix a step in plain English, pause
                         # at any step to inspect the live DOM, generate
                         # and run the test, all from the browser
```

Or skip the UI entirely for scripting/CI:

```
cua codegen --out my-test.spec.ts   # reads the recorded session, no UI/browser needed
```

Generated tests live under `playwright-tests/` (gitignored — regenerate
anytime with `cua codegen`/`cua review`). That directory needs its own
`@playwright/test` install once (`npm init -y && npm i -D @playwright/test`
inside `playwright-tests/`) to actually run the generated spec.

## Notes from implementation

- Playwright MCP's own locator-generation is accessibility-first
  (`getByRole`/`getByTestId`), not our own selector logic — `cua` never
  builds a CSS selector itself. Tag your app's elements with `data-testid`
  and the generated test picks that up automatically (Playwright prefers a
  test id over a role+name match when one exists), which also makes tests
  independent of on-page text/locale.
- A statement in the recorded session can itself span multiple lines (e.g.
  `toMatchAriaSnapshot`'s backtick template literal) — parsing splits on
  top-level `;` while tracking paren depth and template-literal state, not a
  naive line-by-line split, so a multi-line assertion doesn't get shredded
  into bogus separate steps.
- `--isolated` is load-bearing: without it, Playwright MCP persists the
  browser profile to disk and reuses it across separate `cua run`
  invocations, so a page's leftover state (e.g. an item already in
  localStorage) can silently make a step look verified during recording but
  fail on a clean replay.
- The review UI's "pause here" button doesn't just open a debugger — it
  regenerates a truncated test ending in `page.pause()` and runs it headed
  with `PWDEBUG=1`, so you're inspecting the *real* DOM at that exact step
  through Playwright Inspector, not a stale snapshot.
