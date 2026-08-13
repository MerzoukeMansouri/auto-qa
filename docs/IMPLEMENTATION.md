# Notes from implementation

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
- `claude-sdk`/`gemini-sdk` run their own agent loop against the Anthropic/
  Gemini Messages API directly instead of shelling out to a CLI: they spawn
  the same MCP servers (Playwright, autoqa's own block server) themselves via
  `@modelcontextprotocol/sdk`, drive the call-model → run-tool →
  feed-result-back loop by hand (capped at 50 iterations), and print their
  own log format directly — no undocumented CLI stream schema to reverse
  engineer. Gemini 3's function-call parts carry a `thoughtSignature` that
  must be echoed back verbatim on the next turn or the API rejects the
  request; the loop keeps each streamed part object as-is for that reason,
  rather than reconstructing a bare `{functionCall}` from just the name/args.

## See also

- [Usage](USAGE.md)
- [Install](INSTALL.md)
