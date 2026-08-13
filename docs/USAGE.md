# Usage

## Running a task

```
autoqa run --harness opencode --query "go to https://demo.playwright.dev/todomvc/, verify a new todo item can be added, verify each item's completed status can be toggled, and verify an item can be deleted"
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

## Choosing a harness

```
autoqa run --harness copilot --query "..."     # this run only
autoqa config --harness opencode               # change the saved default
autoqa config                                   # interactive picker instead
```

Supported: `claude`, `copilot`, `opencode`, `codex`, `gemini` — each needs its
own CLI installed and authenticated (`claude`, `copilot`, `opencode`, `codex`,
`gemini` respectively) on `$PATH`. Also `claude-sdk` and `gemini-sdk` — own
agent loop against the Anthropic/Gemini API directly (no CLI subprocess),
authenticated via `ANTHROPIC_API_KEY`/`GEMINI_API_KEY` instead.

Resolution order for `run`/`review`, every time: an explicit `--harness` flag
wins; otherwise the harness saved in `~/.autoqa/config.json` is used; if
neither is set, you're prompted once (same picker as `autoqa config`) and the
choice is persisted for next time.

## Choosing a model

```
autoqa run --model claude-opus-5 --query "..."   # this run only
autoqa config --model gemini-3.5-flash-lite      # change the saved default for the current harness
autoqa config                                     # interactive: harness picker, then model picker
```

Each harness remembers its own model in `~/.autoqa/config.json` — switching
harness doesn't carry a model string over that means nothing there. `autoqa
config` with no flags picks from an arrow-key list for harnesses with a
small, stable model set (`claude`/`claude-sdk`: `claude-haiku-4-5` (default),
`claude-sonnet-5`, `claude-opus-5`, `claude-fable-5`; `gemini`/`gemini-sdk`:
`gemini-3.6-flash` (default), `gemini-3.5-flash-lite`) or a free-text prompt
for the rest (`copilot`, `opencode`, `codex` — no bounded list to curate, the
model string goes straight to that CLI's own `--model`/`-m` flag). Same
resolution order as harness: `--model` flag, then the saved value, then the
harness's own default.

## Reusable blocks

The review UI ([docs/REVIEW.md](REVIEW.md)) lets you save a run's steps as a
named, reusable "block" with `{{placeholder}}` bindings (e.g. a `login` block
parameterized on username/password). Any later `autoqa run`'s pre-run picker
can select one or more saved blocks to replay deterministically via a
dedicated MCP tool before the harness starts on the actual task — instead of
re-driving the same setup steps through the browser every time.

## Doctor

```
autoqa doctor                    # run the environment check on its own
autoqa doctor --harness gemini-sdk
autoqa run --recheck ...         # force a fresh check instead of trusting the cache
autoqa run --no-verification ... # skip it entirely for this run
```

`autoqa run`/`autoqa review` run this automatically before doing anything
else — a checklist screen for Node, system Chrome, the selected harness's
CLI (or, for `claude-sdk`/`gemini-sdk`, its API key), and autoqa's own npm
deps, auto-installing what it's allowed to and blocking with copy-pasteable
instructions on what it isn't (Node, Chrome, harness auth — all third-party).
The result is cached (`~/.autoqa/doctor.json`), so a repeat run with nothing
changed skips straight past it. `autoqa doctor` runs it standalone, always
showing the screen even on a cache hit; see
[Environment check](INSTALL.md#environment-check) for the full breakdown.

## See also

- [Install](INSTALL.md)
- [Environment check](INSTALL.md#environment-check)
- [Turning a session into a Playwright test](REVIEW.md)
- [Implementation notes](IMPLEMENTATION.md)
