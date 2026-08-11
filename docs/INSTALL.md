# Installing autoqa

> **For AI agents**: pick the block below matching the user's OS, run it
> verbatim, top to bottom. Each ends with `autoqa --help` — if that doesn't
> print the usage text, stop and check [Troubleshooting](#troubleshooting)
> before retrying or asking the user for help. Never retry with `--force`,
> `sudo`, or by editing Homebrew's own files.
>
> **macOS / Linux with Homebrew:**
> ```sh
> brew tap MerzoukeMansouri/homebrew https://github.com/MerzoukeMansouri/homebrew.git
> brew trust merzoukemansouri/homebrew
> brew install autoqa
> autoqa --help
> ```
> Do not substitute the bare `brew tap MerzoukeMansouri/homebrew` shorthand
> (no URL) — it resolves to a nonexistent repo and 404s. Do not run
> `brew trust` before `brew tap` — it silently no-ops.
>
> **macOS / Linux without Homebrew (manual binary):**
> ```sh
> case "$(uname -s)-$(uname -m)" in
>   Darwin-arm64)  TARGET=aarch64-apple-darwin ;;
>   Darwin-x86_64) TARGET=x86_64-apple-darwin ;;
>   Linux-x86_64)  TARGET=x86_64-unknown-linux-gnu ;;
>   *) echo "unsupported: $(uname -s)-$(uname -m)" >&2; exit 1 ;;
> esac
> URL=$(curl -fsSL https://api.github.com/repos/MerzoukeMansouri/auto-qa/releases/latest \
>   | grep -o "https://[^\"]*${TARGET}\.tar\.gz")
> mkdir -p "$HOME/.local/bin"
> curl -fsSL "$URL" | tar xz -C "$HOME/.local/bin" autoqa
> case ":$PATH:" in
>   *":$HOME/.local/bin:"*) ;;
>   *) echo "export PATH=\"\$HOME/.local/bin:\$PATH\"" >> "$HOME/.$(basename "$SHELL")rc" ;;
> esac
> export PATH="$HOME/.local/bin:$PATH"
> autoqa --help
> ```
>
> **Windows (PowerShell):**
> ```powershell
> $dest = "$env:USERPROFILE\bin"
> New-Item -ItemType Directory -Force -Path $dest | Out-Null
> $release = Invoke-RestMethod -Uri "https://api.github.com/repos/MerzoukeMansouri/auto-qa/releases/latest"
> $asset = $release.assets | Where-Object { $_.name -like "*x86_64-pc-windows-msvc.tar.gz" }
> Invoke-WebRequest -Uri $asset.browser_download_url -OutFile "$env:TEMP\autoqa.tar.gz"
> tar -xzf "$env:TEMP\autoqa.tar.gz" -C $dest autoqa.exe
> if ($env:Path -notlike "*$dest*") {
>   [Environment]::SetEnvironmentVariable("Path", "$([Environment]::GetEnvironmentVariable('Path','User'));$dest", "User")
>   $env:Path += ";$dest"
> }
> autoqa --help
> ```
>
> Then install [dependencies](#dependencies) below — autoqa drives an
> external Chrome and a harness CLI, neither of which it bundles.

## Recommended: Homebrew (macOS / Linux)

autoqa ships through a third-party Homebrew tap. Because it's not on
`homebrew-core`, two things differ from a normal `brew install`:

1. The tap must be added with an **explicit repo URL** — the bare
   `brew tap MerzoukeMansouri/homebrew` shorthand expands to looking for a
   repo named `homebrew-homebrew`, which doesn't exist. The actual repo is
   named `homebrew`, so the URL must be given explicitly.
2. Homebrew requires an explicit **trust** step for third-party taps, and it
   must run *after* tapping — trusting a tap before it's been added is a
   no-op.

```sh
brew tap MerzoukeMansouri/homebrew https://github.com/MerzoukeMansouri/homebrew.git
brew trust merzoukemansouri/homebrew
brew cat autoqa   # optional: review the formula source first
brew install autoqa
```

## Manual binary install (any OS, no Homebrew)

Every [release](https://github.com/MerzoukeMansouri/auto-qa/releases/latest)
ships a `.tar.gz` per platform:

| OS      | Arch         | Asset                                    |
|---------|--------------|-------------------------------------------|
| macOS   | Apple Silicon| `autoqa-<version>-aarch64-apple-darwin.tar.gz` |
| macOS   | Intel        | `autoqa-<version>-x86_64-apple-darwin.tar.gz`  |
| Linux   | x86_64       | `autoqa-<version>-x86_64-unknown-linux-gnu.tar.gz` |
| Windows | x86_64       | `autoqa-<version>-x86_64-pc-windows-msvc.tar.gz`   |

Download the one matching your OS/arch, extract the `autoqa`(`.exe`) binary,
drop it somewhere on `PATH`, and make sure that directory is actually on
`PATH`:

- **macOS/Linux**: `~/.local/bin/autoqa` (`chmod +x` it), with
  `export PATH="$HOME/.local/bin:$PATH"` in your shell rc file.
- **Windows**: `%USERPROFILE%\bin\autoqa.exe`, added to the *User* `Path`
  environment variable (Settings → Environment Variables, or
  `[Environment]::SetEnvironmentVariable` in PowerShell as shown above).

The agent script blocks above automate this end to end, including the
GitHub API lookup for the current release's download URL.

## Dependencies

autoqa doesn't bundle a browser or a coding-agent CLI — it drives ones
already on the machine.

- **Chrome or Chromium**, at one of these exact paths (`autoqa run` looks
  here and nowhere else):
  - macOS: `/Applications/Google Chrome.app` or `/Applications/Chromium.app`
    — install from [google.com/chrome](https://www.google.com/chrome/) or
    `brew install --cask google-chrome`.
  - Linux: `/usr/bin/google-chrome`, `/usr/bin/chromium`, or
    `/usr/bin/chromium-browser` — e.g. `sudo apt install chromium-browser`
    (Debian/Ubuntu) or your distro's equivalent.
  - Windows: `C:\Program Files\Google\Chrome\Application\chrome.exe` (or the
    `(x86)` path) — install from
    [google.com/chrome](https://www.google.com/chrome/).
- **Node.js + npm** (any recent LTS) — used to run Playwright MCP and
  autoqa's own block-server via `npx`/`npm`.
  - macOS: `brew install node`
  - Linux: `sudo apt install nodejs npm` or your distro's equivalent
  - Windows: `winget install OpenJS.NodeJS.LTS` or the installer from
    [nodejs.org](https://nodejs.org/)
- **At least one supported harness CLI**, installed and authenticated:
  `claude` (Claude Code), `copilot` (GitHub Copilot CLI), `opencode`,
  `codex`, or `gemini`. Install and auth each per its own docs — autoqa just
  shells out to whichever one you pick with `--harness`.

## Verify

```
autoqa --help
```

## Uninstall

Homebrew install:

```
brew uninstall autoqa
brew untap merzoukemansouri/homebrew
```

Manual install: delete the binary from wherever you placed it (e.g.
`rm ~/.local/bin/autoqa` or `del %USERPROFILE%\bin\autoqa.exe`).

## Troubleshooting

- **`fatal: could not read Username for 'https://github.com'`** — you ran the
  bare `brew tap MerzoukeMansouri/homebrew` without the explicit URL. Re-run
  with the full `https://github.com/MerzoukeMansouri/homebrew.git` URL as
  shown above.
- **`Refusing to load formula ... from untrusted tap`** — `brew trust` was
  run before `brew tap`, or skipped. Run `brew trust merzoukemansouri/homebrew`
  again after tapping.
- **`Unknown command: brew trust`** — your Homebrew is too old; the `trust`
  command is a recent addition. Run `brew update` first.
- **`autoqa run` fails with "no Chrome/Chromium executable found"** — install
  Chrome/Chromium at one of the exact paths listed under
  [Dependencies](#dependencies); autoqa doesn't search `PATH` for it.
- **manual install: `command not found: autoqa` after installing** — the
  install directory isn't on `PATH` yet in your *current* shell; open a new
  terminal, or re-source your shell rc file (`source ~/.zshrc`, etc.).
