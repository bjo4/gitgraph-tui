# Contributing to gitgraph-tui

Thanks for your interest! This page gets you from clone to merged PR.

## Prerequisites

- Rust **1.88+** (`rustup update stable`)
- Nothing else — libgit2 is built from source by the `git2` crate; no system
  libraries required.

## Build, run, test

```sh
cargo run -- <path-to-a-git-repo>   # run against any repository
cargo test                          # full suite (unit + integration)
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

All three gates (test / clippy / fmt) must pass before a PR is reviewed —
CI enforces them.

## Project map

```
src/
├── main.rs          entry point: CLI args, terminal lifecycle, event loop
├── app.rs           App state machine — every state change goes through handle_key
├── event.rs         crossterm input polling
├── git/             data layer over libgit2
│   ├── types.rs     plain data types (CommitInfo, RefInfo, FileChange, ...)
│   ├── repo.rs      open repo, refs, topological commit walk (chunked)
│   └── diff.rs      per-commit file lists, single-file diffs, worktree status
├── graph/
│   └── layout.rs    pure lane-assignment layout engine (commit DAG → colored cells)
└── ui/              ratatui views — read App state, draw, no business logic
    ├── graph_view.rs / detail_view.rs / diff_view.rs / popup.rs
    └── util.rs      time formatting, width-aware truncation
tests/
├── common/mod.rs    Fixture: builds real git repos with deterministic timestamps
└── *.rs             integration tests per layer
```

## Architecture rules (please keep these invariants)

1. **git2 types never leave `src/git/`.** Public APIs of that module return
   plain types from `types.rs` only.
2. **`graph/layout.rs` stays pure.** No IO, no git2, no ratatui — it maps a
   commit list to drawable cells and is unit-tested exhaustively. If you touch
   layout behavior, add a glyph-string test for the DAG shape you changed.
3. **State changes only in `App::handle_key`.** Views read state; they never
   mutate it.
4. **Read-only.** The tool must never mutate the repository it displays.

## Testing conventions

- Integration tests build **real** git repositories via `tests/common/mod.rs`'s
  `Fixture` (deterministic timestamps, so ordering assertions are stable).
- UI is tested with ratatui's `TestBackend` — see `render_app` in
  `tests/ui_render.rs`.
- Error paths that are hard to reach via git are exercised by injecting an
  invalid oid into `App.oids` (see `load_errors_surface_in_status_instead_of_vanishing`).

## Pull requests

1. Fork, branch from `main`.
2. Add tests for behavior you add or change (TDD encouraged).
3. Make the three gates pass locally.
4. Keep commits in `<type>: <description>` form (feat / fix / refactor / docs / test / chore).
5. Open the PR — CI runs the same gates plus a macOS build check.
