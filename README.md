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
Codex, or Gemini CLI (or `claude-sdk`/`gemini-sdk`, which call the
Anthropic/Gemini API directly, no CLI needed) — wired to
[Playwright MCP](https://github.com/microsoft/playwright-mcp), records the
session, and turns it into a real Playwright test. Reasoning/orchestration is
delegated entirely to whichever harness you pick — this project has no agent
loop of its own for the CLI harnesses, and no browser-automation code of its
own either; Playwright MCP owns the browser.

## Install

**Manually** — see [docs/INSTALL.md](docs/INSTALL.md) for Homebrew, a manual
binary download (macOS/Linux/Windows), and dependencies.

**Via an AI coding agent** — paste this into your harness (Claude Code,
Copilot CLI, Codex, etc.):

> Fetch https://raw.githubusercontent.com/MerzoukeMansouri/auto-qa/main/docs/INSTALL.md
> and follow the instructions in its "For AI agents" section to install the
> `autoqa` CLI.

## Quick start

```
autoqa run --query "go to example.com and tell me the page title"
autoqa review    # turn the recorded session into a Playwright test
```

## Documentation

| | |
|---|---|
| [Install](docs/INSTALL.md) | Homebrew, manual binary, dependencies, troubleshooting |
| [Usage](docs/USAGE.md) | Running a task, choosing a harness/model, reusable blocks |
| [Review & codegen](docs/REVIEW.md) | Turning a recorded session into a Playwright test |
| [Implementation notes](docs/IMPLEMENTATION.md) | Selector strategy, session parsing, harness-specific quirks |
