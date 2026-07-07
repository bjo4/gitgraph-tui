# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-07-07

### Added

- Colored branch graph with lane assignment (forks, merges, octopus merges, criss-cross)
- Branch / tag / remote / HEAD labels on commit rows
- Commit detail panel: full message, author, date, changed files with +/- counts
- Full-screen single-file diff view
- Incremental search (`/`) over message, author, and hash, with `n`/`N`
  navigation that auto-loads older history
- Branch filter popup (`b`)
- Uncommitted-changes row above the newest commit
- Chunked lazy loading (300 commits per chunk) for large repositories
- Read-only by design: never mutates the repository

[0.1.0]: https://github.com/bjo4/gitgraph-tui/releases/tag/v0.1.0
