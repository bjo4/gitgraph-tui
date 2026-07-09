# gitgraph-tui

[![CI](https://github.com/bjo4/gitgraph-tui/actions/workflows/ci.yml/badge.svg)](https://github.com/bjo4/gitgraph-tui/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/bjo4/gitgraph-tui)](https://github.com/bjo4/gitgraph-tui/releases/latest)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

**VSCode Git Graph, but in your terminal.** A fast, read-only git history
viewer: colored branch lanes, commit details, diffs, search — and it never,
ever writes to your repository.

[繁體中文說明](README.zh-TW.md)

```text
┌ my-repo — all branches — 128/500 commits ────────────────────────────┐
│ ●    Uncommitted changes (1 files)                                   │
│ ●─╮  [HEAD] [main] [v1.0] merge: dev into main       anna       2h   │
│ ● │  fix: main work                                  anna       5h   │
│ │ ●  [dev] feat: dev work                            ben        1d   │
│ ●─╯  init                                            anna       2d   │
├──────────────────────────────────────────────────────────────────────┤
│ commit a1b2c3d · anna <anna@example.com> · 2026-07-06 14:30          │
│  M src/lib.rs  +12 -3                                                │
└ j/k:move g/G:top/bot tab:focus enter:diff /:search b:branches q:quit ┘
```

## Features

- **Colored branch graph** — lane-assignment layout handles forks, merges,
  octopus merges, and criss-cross histories
- **Ref labels** — local / remote branches, tags, HEAD, right on the rows
- **Commit details** — full message, author, date, changed files with +/- counts
- **Full-screen diffs** — per file, colored, scrollable
- **Incremental search** — message / author / hash; `n`/`N` auto-load older
  history until the next match
- **Branch filter** — show only what's reachable from one branch
- **Uncommitted changes** — a live row above the newest commit
- **Live auto-refresh** — external commits, checkouts, branch/tag edits, and
  worktree changes show up on their own, no keypress; your cursor and active
  search stay put across the refresh
- **Big-repo friendly** — history loads in chunks of 300 as you scroll
- **Read-only by design** — no checkout, no merge, no reset. Ever.

## Install

### One-liner (Linux and macOS)

```sh
curl -fsSL https://raw.githubusercontent.com/bjo4/gitgraph-tui/main/install.sh | sh
```

Downloads the prebuilt binary for your platform from the latest GitHub
release, verifies its sha256, and installs to `~/.local/bin`.

- Choose the directory: `GITGRAPH_INSTALL_DIR=/usr/local/bin curl ... | sh`
- Pin a version: `GITGRAPH_VERSION=v0.2.0 curl ... | sh`
- No prebuilt binary for your platform? The script falls back to
  `cargo install` automatically (needs [Rust](https://rustup.rs)).

### With cargo (any platform, including Windows)

```sh
cargo install --git https://github.com/bjo4/gitgraph-tui --locked
```

### From source

```sh
git clone https://github.com/bjo4/gitgraph-tui
cd gitgraph-tui
cargo install --path . --locked
```

## Usage

```sh
gitgraph-tui              # repository containing the current directory
gitgraph-tui ~/src/foo    # explicit path
```

Tip: `alias gg=gitgraph-tui`

## Keys

| Key | Action |
| --- | --- |
| `j` `k` `↑` `↓` | move (commit list, or file list when focused) |
| `g` / `G` | jump to top / bottom (loads the full history) |
| `Tab` | toggle focus: commits ↔ changed files |
| `Enter` | open the diff of the selected file |
| `/` | incremental search (message, author, hash) |
| `n` / `N` | next / previous match (auto-loads older commits) |
| `b` | filter by branch |
| `r` | force a full reload (the view also auto-refreshes on its own) |
| `Esc` / `q` | back / quit |

## How it works

Three layers, strictly separated: `src/git/` reads the repository through
libgit2 and exposes plain data types; `src/graph/layout.rs` is a pure
lane-assignment engine that turns the commit DAG into colored cells (no IO,
exhaustively unit-tested); `src/app.rs` + `src/ui/` are an Elm-style state
machine with ratatui views. The developer tour lives in
[CONTRIBUTING.md](CONTRIBUTING.md).

## Contributing

Issues and PRs welcome — see [CONTRIBUTING.md](CONTRIBUTING.md).
The short version: `cargo test`, `cargo clippy --all-targets -- -D warnings`,
and `cargo fmt --check` must all pass.

## License

[MIT](LICENSE)
