# Installing autoqa

autoqa ships through a third-party Homebrew tap. Because it's not on
`homebrew-core`, two things differ from a normal `brew install`:

1. The tap must be added with an **explicit repo URL** — the bare
   `brew tap MerzoukeMansouri/homebrew` shorthand expands to looking for a
   repo named `homebrew-homebrew`, which doesn't exist. The actual repo is
   named `homebrew`, so the URL must be given explicitly.
2. Homebrew requires an explicit **trust** step for third-party taps, and it
   must run *after* tapping — trusting a tap before it's been added is a
   no-op.

## Steps

```
brew tap MerzoukeMansouri/homebrew https://github.com/MerzoukeMansouri/homebrew.git
brew trust merzoukemansouri/homebrew
brew cat autoqa   # optional: review the formula source first
brew install autoqa
```

## Verify

```
autoqa --version
```

## Uninstall

```
brew uninstall autoqa
brew untap merzoukemansouri/homebrew
```

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
