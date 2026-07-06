# gitgraph-tui

A read-only git history graph viewer for the terminal — VSCode Git Graph,
but in your shell. Colored branch lanes, ref labels, commit details, file
diffs, incremental search, and branch filtering. Never mutates your repo.

## Install

```sh
cargo install --path .
```

Tip: alias it to `gg`:

```sh
alias gg=gitgraph-tui
```

## Usage

```sh
gitgraph-tui            # repository containing the current directory
gitgraph-tui ~/src/foo  # explicit path
```

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
| `r` | reload the repository |
| `Esc` / `q` | back / quit |

Uncommitted changes appear as a yellow row above the newest commit.

## Notes

- Large repositories load in chunks of 300 commits as you scroll.
- Read-only by design: no checkout, merge, or reset. Ever.
