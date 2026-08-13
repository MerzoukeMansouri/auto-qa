# Turning a session into a Playwright test

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

## See also

- [Usage](USAGE.md)
- [Install](INSTALL.md)
