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

# auto-qa

A Rust CLI (`autoqa`) that drives a real Chrome browser through a headless
coding-agent CLI of your choice — Claude Code, GitHub Copilot CLI, opencode,
Codex, or Gemini CLI — wired to [Playwright MCP](https://github.com/microsoft/playwright-mcp),
records the session, and turns it into a real Playwright test.
Reasoning/orchestration is delegated entirely to whichever harness you pick —
this project has no agent loop of its own, and no browser-automation code of
its own either; Playwright MCP owns the browser.

## Usage

Install via Homebrew:

```
brew tap MerzoukeMansouri/homebrew
brew install autoqa
```

Drive a browser end-to-end from a natural-language query:

```
autoqa run --query "go to example.com and tell me the page title"
```

`autoqa run` launches its own Chrome (CDP debug port, fresh profile dir per
run — no leftover cookies/localStorage between runs) and wires the selected
harness to Playwright MCP attached to it over CDP. It shows the harness's
reasoning and every tool call live (piped through `jq` — `brew install jq` —
for Claude/opencode/Gemini's confirmed stream schemas; raw passthrough for
Copilot/Codex, whose stream formats aren't officially documented) instead of
a wall of raw NDJSON. The harness is instructed to actually drive the page
(never answer from memory) and to verify the outcome of every action — not
just the ones you happen to ask it to check — so the recorded session has
real assertions, not just a chain of clicks.

`--locale` (BCP 47, default `en-US`) pins the MCP browser context's locale —
without it, a recorded session and a later `playwright test` run of the
generated spec can silently disagree on date formats/form input.

Before the run starts, a pre-run picker (`↑`/`↓`, `Enter` to add/bind, `g` to
start) lets you select and order any reusable "blocks" — named, recorded step
sequences with placeholder bindings — to replay first; see
[Reusable blocks](#reusable-blocks) below.

### Choosing a harness

```
autoqa run --harness copilot --query "..."     # this run only
autoqa config --harness opencode               # change the saved default
autoqa config                                   # interactive picker instead
```

Supported: `claude`, `copilot`, `opencode`, `codex`, `gemini` — each needs its
own CLI installed and authenticated (`claude`, `copilot`, `opencode`, `codex`,
`gemini` respectively) on `$PATH`.

Resolution order for `run`/`review`, every time: an explicit `--harness` flag
wins; otherwise the harness saved in `~/.autoqa/config.json` is used; if
neither is set, you're prompted once (same picker as `autoqa config`) and the
choice is persisted for next time.

### Reusable blocks

The review UI (below) lets you save a run's steps as a named, reusable
"block" with `{{placeholder}}` bindings (e.g. a `login` block parameterized on
username/password). Any later `autoqa run`'s pre-run picker can select one or
more saved blocks to replay deterministically via a dedicated MCP tool before
the harness starts on the actual task — instead of re-driving the same setup
steps through the browser every time.

### Turning a session into a Playwright test

Playwright MCP's `--save-session` records every tool call's ready-made JS
statement (`await page.goto(...)`, `await expect(...).toBeVisible()`, ...).
`autoqa` parses that into an editable step list — one `{action, assertion}` pair
per step — and can turn it into a real `.spec.ts`:

```
autoqa review                    # opens a local dark-themed UI at localhost:4321
                                  # — edit/reorder/insert/delete steps, chat with
                                  # a harness to add or fix a step in plain English,
                                  # save/replay reusable blocks, pause at any step to
                                  # inspect the live DOM, generate and run the test
autoqa review --port 8080        # different port
autoqa review --harness copilot  # harness used for the chat-based step editor
```

Or skip the UI entirely for scripting/CI:

```
autoqa codegen --out my-test.spec.ts   # reads the recorded session, no UI/browser needed
```

Generated tests live under `playwright-tests/` (gitignored — regenerate
anytime with `autoqa codegen`/`autoqa review`). That directory needs its own
`@playwright/test` install once (`npm init -y && npm i -D @playwright/test`
inside `playwright-tests/`) to actually run the generated spec.

## Notes from implementation

- Playwright MCP's own locator-generation is accessibility-first
  (`getByRole`/`getByTestId`), not our own selector logic — `autoqa` never
  builds a CSS selector itself. Tag your app's elements with `data-testid`
  and the generated test picks that up automatically (Playwright prefers a
  test id over a role+name match when one exists), which also makes tests
  independent of on-page text/locale.
- A statement in the recorded session can itself span multiple lines (e.g.
  `toMatchAriaSnapshot`'s backtick template literal) — parsing splits on
  top-level `;` while tracking paren depth and template-literal state, not a
  naive line-by-line split, so a multi-line assertion doesn't get shredded
  into bogus separate steps.
- `autoqa run` launches Chrome itself with a profile dir keyed to the run's
  pid, then attaches Playwright MCP to it via `--cdp-endpoint` instead of
  letting MCP launch its own browser — so a second MCP server (the block
  replayer) can drive the exact same browser over CDP alongside the harness.
  A fresh profile dir per run gives the same no-leftover-state guarantee
  `--isolated` used to.
- Copilot's `--additional-mcp-config` takes a JSON string or an `@`-prefixed
  file path — a bare path is parsed as JSON text and fails. Opencode's
  `--pure` flag is passed on every invocation so your global
  `~/.config/opencode/opencode.json` plugins can't leak stdout noise into
  the harness's output parsing.
- The review UI's "pause here" button doesn't just open a debugger — it
  regenerates a truncated test ending in `page.pause()` and runs it headed
  with `PWDEBUG=1`, so you're inspecting the *real* DOM at that exact step
  through Playwright Inspector, not a stale snapshot.
