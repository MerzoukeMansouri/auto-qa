# Running autoqa in CI

## Setup action

`.github/actions/setup` downloads the `autoqa` release binary for the
runner's OS/arch and adds it to `PATH` — no Homebrew tap/trust dance, which
is macOS/Linux-oriented and slow to bootstrap on `ubuntu-latest`. It installs
the binary only: Chrome/Node/harness CLI are still yours to provide (GitHub's
`ubuntu-latest`/`macos-latest` images ship both Chrome and Node already).

```yaml
- uses: MerzoukeMansouri/auto-qa/.github/actions/setup@main
  with:
    version: latest   # optional — pin to a release tag, e.g. "v0.3.0"
```

## Two flags for non-interactive environments

`autoqa run` is built around an interactive terminal — a live ratatui pane
streaming the harness's reasoning, and a pre-run picker for reusable blocks.
Neither works on a CI runner (no controlling terminal, no display), so two
flags swap them out:

- `--headless` — launches Chrome with `--headless=new`, plus `--no-sandbox`
  (CI containers usually lack the setuid sandbox helpers Chrome's sandbox
  needs, and `--headless` is specifically the CI-in-a-container case).
- `--no-tui` — streams plain log lines to stdout instead of the ratatui pane
  (which needs a real terminal and fails with `ENXIO` otherwise), and skips
  the pre-run block picker.

Pair both with `--no-verification` to skip the environment checklist screen
(also ratatui-based) — safe here since the setup action + CI image already
guarantee Node/Chrome are present.

```
autoqa run --query "..." --harness claude-sdk --headless --no-tui --no-verification
```

`claude-sdk`/`gemini-sdk` are the harnesses to reach for in CI: they call the
Anthropic/Gemini API directly with an API-key secret, unlike `claude`/`gemini`
which expect an interactive OAuth/keychain login that doesn't work headless.

## Persisting the generated test

`autoqa run` only records the session to `~/.autoqa` on the runner — it
doesn't leave the runner unless a later step ships it out. `autoqa codegen`
turns that session into a `.spec.ts` (see
[Review & codegen](REVIEW.md)); upload it as a workflow artifact to get it
off the ephemeral runner:

```yaml
- run: autoqa codegen --out playwright-tests/autoqa-generated.spec.ts

- uses: actions/upload-artifact@v4
  with:
    name: autoqa-generated-test
    path: playwright-tests/autoqa-generated.spec.ts
    retention-days: 7
```

## Reference workflow

[`.github/workflows/demo_run.yml`](../.github/workflows/demo_run.yml) is a
working, `workflow_dispatch`-triggered example wiring all of the above
together end to end (setup action → `autoqa run` → `autoqa codegen` →
artifact upload) against a public demo site.
