# gitgraph-tui Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 打造一個只讀的 Rust TUI git 歷史瀏覽器（gitgraph-tui），在終端呈現 VSCode Git Graph 式的彩色 branch graph、commit 詳情、diff、搜尋與 branch 篩選。

**Architecture:** Elm 式單向資料流（單一 `App` state + `handle_key` 純狀態轉移 + `ui::render`）。git2 讀取資料並轉成自家型別（git2 型別不外洩）；`graph::layout` 是無 IO 的純函式 lane 分配引擎，獨立密集測試；ratatui 只負責把 state 畫出來。

**Tech Stack:** Rust 2024 edition（rustc 1.95 已驗證）· ratatui 0.30 · crossterm 0.29 · git2 0.21 · clap 4（derive）· anyhow 1 · time 0.3 · unicode-width 0.2 · lru 0.18 · tempfile 3（dev）

**Spec:** `docs/superpowers/specs/2026-07-06-gitgraph-tui-design.md`（已核准）

## Global Constraints

- **只讀**：任何 task 都不得呼叫會改變 repo 狀態的 git2 API（checkout / reset / commit 到目標 repo 等）。測試 fixture 內建 repo 除外。
- **依賴版本已於 2026-07-06 在本機編譯探針實測**（scratchpad/api-probe，`cargo check --all-targets` + 2 tests 通過）：`ratatui = "0.30"`、`crossterm = "0.29"`、`git2 = "0.21"`、`clap = { version = "4", features = ["derive"] }`、`anyhow = "1"`、`time = { version = "0.3", features = ["formatting", "macros"] }`、`unicode-width = "0.2"`、`lru = "0.18"`、dev: `tempfile = "3"`。不要改版本。
- **git2 0.21 API 注意**（與舊版不同，已實測）：`Commit::message()` → `Result<&str>`；`Commit::summary()` → `Result<Option<&str>>`；`Signature::name()/email()` → `Result<&str>`。寫法：`c.summary().ok().flatten().unwrap_or("")`、`c.message().unwrap_or("")`。
- 分塊載入大小：`App.chunk_size`，預設 **300**（測試可注入小值）。
- Graph 色盤：**8 色**，lane 開啟時從遞增計數器 `% 8` 取色。
- UI 文案一律**英文**（"No commits yet"、"Uncommitted changes" …）。
- 鍵位（spec §6，不可改）：`j/k/↑/↓` 移動、`g/G` 頂/底、`Tab` 焦點切換、`Enter` 開 diff、`/` 搜尋、`n/N` 跳符合、`b` branch 篩選、`r` 重新載入、`Esc/q` 返回/離開。
- 每個 task 結束前：`cargo test` 全綠、`cargo clippy --all-targets -- -D warnings` 無錯、`cargo fmt --check` 通過，**展示輸出**再 commit。
- Commit 訊息格式：`<type>: <description>`（feat/fix/refactor/docs/test/chore）。不加 Co-Authored-By。
- 函式 <50 行、檔案盡量 200–400 行；錯誤不吞、boundary 驗證輸入。

## 檔案地圖（最終形態）

```
gitgraph-tui/
├── Cargo.toml
├── .gitignore
├── README.md                    (Task 13)
├── src/
│   ├── main.rs                  進入點：clap、錯誤出口、terminal 生命週期 (Task 1, 7)
│   ├── lib.rs                   pub mod git; graph; app; event; ui (Task 2 起)
│   ├── app.rs                   App state + handle_key (Task 7, 9–12)
│   ├── event.rs                 poll_key 封裝 (Task 7)
│   ├── git/
│   │   ├── mod.rs
│   │   ├── types.rs             CommitInfo/RefInfo/FileChange/DiffLine (Task 2)
│   │   ├── repo.rs              GitRepo: discover/refs/commit_ids/load_commits (Task 2, 3)
│   │   └── diff.rs              commit_files/file_diff/worktree_* (Task 4)
│   ├── graph/
│   │   ├── mod.rs
│   │   └── layout.rs            LayoutEngine：lane 分配 + cell 渲染 (Task 5)
│   └── ui/
│       ├── mod.rs               render 總調度、PALETTE、ref_style (Task 6–8)
│       ├── util.rs              relative_time/absolute_time/truncate_width (Task 6)
│       ├── graph_view.rs        graph + commit 列表 (Task 8)
│       ├── detail_view.rs       詳情面板 (Task 9)
│       ├── diff_view.rs         全螢幕 diff (Task 10)
│       └── popup.rs             branch 篩選彈窗 (Task 12)
└── tests/
    ├── common/mod.rs            Fixture：程式化建測試 repo (Task 2)
    ├── git_repo.rs              (Task 2, 3)
    ├── git_diff.rs              (Task 4)
    ├── app_state.rs             (Task 7, 9–12)
    ├── ui_render.rs             (Task 8–12)
    └── cli.rs                   exit code 整合測試 (Task 13)
```

（`graph/layout.rs` 的單元測試放模組內 `#[cfg(test)]`，因為要驗證私有 lane 狀態的行為。）

---

### Task 1: Cargo 專案骨架

**Files:**
- Create: `Cargo.toml`
- Create: `.gitignore`
- Create: `src/main.rs`

**Interfaces:**
- Consumes: 無（第一個 task）
- Produces: 可執行的 `gitgraph-tui` binary、後續所有 task 依賴的 Cargo.toml 依賴集

- [ ] **Step 1: 寫 Cargo.toml**（版本是實測值，勿改）

```toml
[package]
name = "gitgraph-tui"
version = "0.1.0"
edition = "2024"
rust-version = "1.88"
description = "A read-only git history graph viewer for the terminal"
license = "MIT"

[dependencies]
anyhow = "1"
clap = { version = "4", features = ["derive"] }
crossterm = "0.29"
git2 = "0.21"
lru = "0.18"
ratatui = "0.30"
time = { version = "0.3", features = ["formatting", "macros"] }
unicode-width = "0.2"

[dev-dependencies]
tempfile = "3"

[profile.release]
lto = true
strip = true
```

- [ ] **Step 2: 寫 .gitignore**

```gitignore
/target
```

- [ ] **Step 3: 寫 src/main.rs 暫時骨架**

```rust
use clap::Parser;

/// A read-only git history graph viewer for the terminal.
#[derive(Parser)]
#[command(name = "gitgraph-tui", version, about)]
struct Cli {
    /// Path to a git repository (defaults to the current directory)
    path: Option<std::path::PathBuf>,
}

fn main() {
    let _cli = Cli::parse();
    println!("gitgraph-tui: TUI not yet implemented");
}
```

- [ ] **Step 4: 驗證編譯與執行**

Run: `cargo run -- --help`
Expected: 印出 usage，含 `Path to a git repository`。第一次編譯會 build libgit2，需 2–4 分鐘。

Run: `cargo clippy --all-targets -- -D warnings && cargo fmt --check`
Expected: 皆無輸出（通過）。

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock .gitignore src/main.rs
git commit -m "chore: scaffold cargo project with pinned dependencies"
```

---

### Task 2: git 資料型別、測試 Fixture、GitRepo 開啟與 refs

**Files:**
- Create: `src/lib.rs`
- Create: `src/git/mod.rs`
- Create: `src/git/types.rs`
- Create: `src/git/repo.rs`
- Test: `tests/common/mod.rs`（fixture 工具，全案共用）
- Test: `tests/git_repo.rs`

**Interfaces:**
- Consumes: 無
- Produces:
  - `git::types`：`CommitId = String`、`CommitInfo { id, short_id, parents: Vec<CommitId>, summary, message, author_name, author_email, timestamp: i64 }`、`RefKind { Head, LocalBranch, RemoteBranch, Tag }`、`RefInfo { name, refname, kind, target: CommitId }`、`ChangeKind { Added, Modified, Deleted, Renamed { from: String } }`、`FileChange { path, kind, additions: usize, deletions: usize, is_binary: bool }`、`DiffLine { origin: char, content: String }`
  - `git::GitRepo`：`discover(&Path) -> Result<GitRepo>`、`name() -> String`、`refs() -> Result<Vec<RefInfo>>`、`GitRepo::ref_map(&[RefInfo]) -> HashMap<CommitId, Vec<RefInfo>>`
  - `tests/common/mod.rs`：`Fixture::new()`、`.path()`、`.write_file(rel, content)`、`.commit(msg, adds: &[(&str, &str)], removes: &[&str], parents: &[Oid], ts: i64) -> Oid`、`.branch(name, target)`、`.tag(name, target)`、`.set_head(refname)`

- [ ] **Step 1: 寫 fixture 工具 tests/common/mod.rs**（先寫工具，之後所有整合測試都用它）

```rust
//! Programmatic git repository fixtures with deterministic timestamps.
//! Compiled separately into EVERY integration-test binary; not every binary
//! uses every helper, so dead_code must be allowed or clippy -D warnings
//! fails in test files this module didn't change.
#![allow(dead_code)]

use git2::{Oid, Repository, Signature, Time};
use std::fs;
use std::path::Path;
use tempfile::TempDir;

pub struct Fixture {
    pub dir: TempDir,
    pub repo: Repository,
}

impl Fixture {
    pub fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = Repository::init(dir.path()).expect("init repo");
        Self { dir, repo }
    }

    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    pub fn write_file(&self, rel: &str, content: &str) {
        let p = self.dir.path().join(rel);
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).expect("mkdir");
        }
        fs::write(p, content).expect("write file");
    }

    /// Create a commit with `adds` written+staged and `removes` deleted+staged.
    /// Moves no ref: attach branches yourself via `branch`/`set_head`.
    pub fn commit(
        &self,
        msg: &str,
        adds: &[(&str, &str)],
        removes: &[&str],
        parents: &[Oid],
        ts: i64,
    ) -> Oid {
        let mut index = self.repo.index().expect("index");
        for (path, content) in adds {
            self.write_file(path, content);
            index.add_path(Path::new(path)).expect("index add");
        }
        for path in removes {
            let _ = fs::remove_file(self.dir.path().join(path));
            index.remove_path(Path::new(path)).expect("index remove");
        }
        index.write().expect("index write");
        let tree_id = index.write_tree().expect("write tree");
        let tree = self.repo.find_tree(tree_id).expect("find tree");
        let sig = Signature::new("Test Author", "test@example.com", &Time::new(ts, 0))
            .expect("signature");
        let parent_commits: Vec<git2::Commit> = parents
            .iter()
            .map(|o| self.repo.find_commit(*o).expect("parent"))
            .collect();
        let parent_refs: Vec<&git2::Commit> = parent_commits.iter().collect();
        self.repo
            .commit(None, &sig, &sig, msg, &tree, &parent_refs)
            .expect("commit")
    }

    pub fn branch(&self, name: &str, target: Oid) {
        let c = self.repo.find_commit(target).expect("target commit");
        self.repo.branch(name, &c, true).expect("branch");
    }

    pub fn tag(&self, name: &str, target: Oid) {
        let obj = self.repo.find_object(target, None).expect("target object");
        self.repo.tag_lightweight(name, &obj, true).expect("tag");
    }

    /// Simulate a fetched remote-tracking branch (refs/remotes/origin/<name>).
    pub fn remote_branch(&self, name: &str, target: Oid) {
        self.repo
            .reference(
                &format!("refs/remotes/origin/{name}"),
                target,
                true,
                "test: remote branch",
            )
            .expect("remote ref");
    }

    pub fn set_head(&self, refname: &str) {
        self.repo.set_head(refname).expect("set head");
        let mut co = git2::build::CheckoutBuilder::new();
        co.force();
        self.repo.checkout_head(Some(&mut co)).expect("checkout");
    }
}
```

- [ ] **Step 2: 寫失敗測試 tests/git_repo.rs**

```rust
mod common;

use common::Fixture;
use gitgraph_tui::git::types::RefKind;
use gitgraph_tui::git::GitRepo;

/// Standard fixture: main(c1→c2, HEAD) · dev@c1 · tag v1.0@c1 · origin/main@c2
fn repo_with_refs() -> (Fixture, git2::Oid, git2::Oid) {
    let f = Fixture::new();
    let c1 = f.commit("init", &[("a.txt", "1")], &[], &[], 1_000);
    let c2 = f.commit("second", &[("a.txt", "2")], &[], &[c1], 2_000);
    f.branch("main", c2);
    f.branch("dev", c1);
    f.tag("v1.0", c1);
    f.remote_branch("main", c2);
    f.set_head("refs/heads/main");
    (f, c1, c2)
}

#[test]
fn discover_fails_outside_a_repo() {
    let dir = tempfile::tempdir().unwrap();
    let err = GitRepo::discover(dir.path()).unwrap_err();
    assert!(err.to_string().contains("not a git repository"));
}

#[test]
fn discover_works_from_a_subdirectory() {
    let (f, _, _) = repo_with_refs();
    let sub = f.path().join("src/deep");
    std::fs::create_dir_all(&sub).unwrap();
    assert!(GitRepo::discover(&sub).is_ok());
}

#[test]
fn name_is_the_repo_directory_name() {
    let (f, _, _) = repo_with_refs();
    let repo = GitRepo::discover(f.path()).unwrap();
    let dirname = f.path().file_name().unwrap().to_string_lossy().to_string();
    assert_eq!(repo.name(), dirname);
}

#[test]
fn refs_include_head_branches_and_tags() {
    let (f, c1, c2) = repo_with_refs();
    let repo = GitRepo::discover(f.path()).unwrap();
    let refs = repo.refs().unwrap();
    let find = |n: &str| refs.iter().find(|r| r.name == n).unwrap_or_else(|| panic!("missing ref {n}"));
    assert_eq!(find("main").kind, RefKind::LocalBranch);
    assert_eq!(find("main").target, c2.to_string());
    assert_eq!(find("main").refname, "refs/heads/main");
    assert_eq!(find("dev").target, c1.to_string());
    assert_eq!(find("v1.0").kind, RefKind::Tag);
    assert_eq!(find("origin/main").kind, RefKind::RemoteBranch);
    assert_eq!(find("origin/main").refname, "refs/remotes/origin/main");
    assert_eq!(find("HEAD").kind, RefKind::Head);
    assert_eq!(find("HEAD").target, c2.to_string());
}

#[test]
fn ref_map_groups_by_target_commit() {
    let (f, c1, _) = repo_with_refs();
    let repo = GitRepo::discover(f.path()).unwrap();
    let refs = repo.refs().unwrap();
    let map = GitRepo::ref_map(&refs);
    let at_c1 = map.get(&c1.to_string()).unwrap();
    let names: Vec<&str> = at_c1.iter().map(|r| r.name.as_str()).collect();
    assert!(names.contains(&"dev"));
    assert!(names.contains(&"v1.0"));
}
```

- [ ] **Step 3: 跑測試確認失敗**

Run: `cargo test --test git_repo 2>&1 | tail -5`
Expected: 編譯錯誤 `unresolved import gitgraph_tui`（lib 尚不存在）。

- [ ] **Step 4: 寫 src/lib.rs 與 src/git/mod.rs**

```rust
// src/lib.rs
pub mod git;
```

```rust
// src/git/mod.rs
pub mod repo;
pub mod types;

pub use repo::GitRepo;
```

- [ ] **Step 5: 寫 src/git/types.rs**

```rust
//! Plain data types shared across layers. git2 types never leak past src/git/.

pub type CommitId = String; // 40-char hex

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitInfo {
    pub id: CommitId,
    pub short_id: String, // first 7 chars
    pub parents: Vec<CommitId>,
    pub summary: String,
    pub message: String,
    pub author_name: String,
    pub author_email: String,
    pub timestamp: i64, // unix seconds (author time)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefKind {
    Head,
    LocalBranch,
    RemoteBranch,
    Tag,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefInfo {
    /// Short display name, e.g. "main", "origin/main", "v1.0", "HEAD".
    pub name: String,
    /// Full reference name, e.g. "refs/heads/main"; "HEAD" for the Head entry.
    pub refname: String,
    pub kind: RefKind,
    pub target: CommitId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChangeKind {
    Added,
    Modified,
    Deleted,
    Renamed { from: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileChange {
    pub path: String,
    pub kind: ChangeKind,
    pub additions: usize,
    pub deletions: usize,
    pub is_binary: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    /// git2 line origin: '+', '-', ' ' (context), '@' (hunk header), 'B' (binary marker).
    pub origin: char,
    pub content: String,
}
```

- [ ] **Step 6: 寫 src/git/repo.rs（discover / name / refs / ref_map）**

```rust
use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};
use git2::Repository;

use super::types::{CommitId, RefInfo, RefKind};

pub struct GitRepo {
    pub(crate) inner: Repository,
}

impl GitRepo {
    /// Open the repository containing `path` (walks up parent directories,
    /// like the git CLI).
    pub fn discover(path: &Path) -> Result<Self> {
        let inner = Repository::discover(path).with_context(|| {
            format!("not a git repository (or any parent): {}", path.display())
        })?;
        Ok(Self { inner })
    }

    /// Repository directory name, for the title bar.
    pub fn name(&self) -> String {
        let p = self.inner.workdir().unwrap_or_else(|| self.inner.path());
        p.file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "repo".to_string())
    }

    /// HEAD plus every local branch, remote branch, and tag, resolved to
    /// the commit each points at (annotated tags are peeled).
    pub fn refs(&self) -> Result<Vec<RefInfo>> {
        let mut out = Vec::new();
        if let Ok(head) = self.inner.head()
            && let Ok(commit) = head.peel_to_commit()
        {
            let name = if self.inner.head_detached().unwrap_or(false) {
                "HEAD (detached)".to_string()
            } else {
                "HEAD".to_string()
            };
            out.push(RefInfo {
                name,
                refname: "HEAD".to_string(),
                kind: RefKind::Head,
                target: commit.id().to_string(),
            });
        }
        for r in self.inner.references()? {
            let Ok(r) = r else { continue };
            let kind = if r.is_branch() {
                RefKind::LocalBranch
            } else if r.is_remote() {
                RefKind::RemoteBranch
            } else if r.is_tag() {
                RefKind::Tag
            } else {
                continue;
            };
            let (Some(refname), Some(short)) = (r.name(), r.shorthand()) else {
                continue;
            };
            let (refname, name) = (refname.to_string(), short.to_string());
            let Ok(commit) = r.peel_to_commit() else { continue };
            out.push(RefInfo {
                name,
                refname,
                kind,
                target: commit.id().to_string(),
            });
        }
        Ok(out)
    }

    /// Group refs by the commit they point at, for per-row label lookup.
    pub fn ref_map(refs: &[RefInfo]) -> HashMap<CommitId, Vec<RefInfo>> {
        let mut map: HashMap<CommitId, Vec<RefInfo>> = HashMap::new();
        for r in refs {
            map.entry(r.target.clone()).or_default().push(r.clone());
        }
        map
    }
}
```

- [ ] **Step 7: 跑測試確認通過**

Run: `cargo test --test git_repo 2>&1 | tail -3`
Expected: `test result: ok. 5 passed; 0 failed`

- [ ] **Step 8: 品質閘門後 commit**

Run: `cargo clippy --all-targets -- -D warnings && cargo fmt --check`
Expected: 無輸出。

```bash
git add src/ tests/
git commit -m "feat: git data types, repo open/refs, and test fixtures"
```

---

### Task 3: commit 走訪（topo 排序、分塊載入、branch 篩選）

**Files:**
- Modify: `src/git/repo.rs`（在 `impl GitRepo` 內追加）
- Test: `tests/git_repo.rs`（追加測試）

**Interfaces:**
- Consumes: Task 2 的 `GitRepo`、`CommitInfo`、`Fixture`
- Produces:
  - `GitRepo::commit_ids(&self, filter: Option<&str>) -> Result<Vec<CommitId>>` — 一次走完整個 DAG 只收 oid（快）；`filter` 是完整 refname（如 `"refs/heads/dev"`），`None` = 全部 refs + HEAD。回傳 `CommitId`（hex 字串）而非 `git2::Oid`，git2 型別不出 `src/git/`
  - `GitRepo::load_commits(&self, ids: &[CommitId]) -> Result<Vec<CommitInfo>>` — 把一塊 oid 轉成 `CommitInfo`（懶載入的「重」半邊）

> 設計說明：spec 的「分塊 lazy load」由兩段 API 組成 — oid 走訪一次做完（10 萬 commits 也只是毫秒級），`load_commits` 每次只轉一塊（預設 300 個）。這比可續傳的 revwalk 簡單得多且效果相同。

- [ ] **Step 1: 追加失敗測試到 tests/git_repo.rs**

```rust
/// main: c1→c2→c4(HEAD) · feature: c1→c3 （c3 不可達自 main）
fn repo_with_branches() -> (Fixture, Vec<String>) {
    let f = Fixture::new();
    let c1 = f.commit("c1 init", &[("a.txt", "1")], &[], &[], 1_000);
    let c2 = f.commit("c2 main work", &[("a.txt", "2")], &[], &[c1], 2_000);
    let c3 = f.commit("c3 feature work", &[("b.txt", "3")], &[], &[c1], 3_000);
    let c4 = f.commit("c4 more main", &[("a.txt", "4")], &[], &[c2], 4_000);
    f.branch("main", c4);
    f.branch("feature", c3);
    f.set_head("refs/heads/main");
    (f, [c1, c2, c3, c4].iter().map(|o| o.to_string()).collect())
}

#[test]
fn commit_ids_walk_all_refs_in_topo_order() {
    let (f, c) = repo_with_branches();
    let repo = GitRepo::discover(f.path()).unwrap();
    let ids = repo.commit_ids(None).unwrap();
    assert_eq!(ids.len(), 4);
    let pos = |id: &str| ids.iter().position(|i| i == id).unwrap();
    // topological: every child appears before its parent
    assert!(pos(&c[3]) < pos(&c[1]), "c4 before its parent c2");
    assert!(pos(&c[1]) < pos(&c[0]), "c2 before its parent c1");
    assert!(pos(&c[2]) < pos(&c[0]), "c3 before its parent c1");
}

#[test]
fn commit_ids_with_filter_only_walks_that_ref() {
    let (f, c) = repo_with_branches();
    let repo = GitRepo::discover(f.path()).unwrap();
    let ids = repo.commit_ids(Some("refs/heads/main")).unwrap();
    assert_eq!(ids.len(), 3); // c4, c2, c1 — c3 excluded
    assert!(!ids.contains(&c[2]));
}

#[test]
fn commit_ids_on_empty_repo_is_empty() {
    let f = Fixture::new();
    let repo = GitRepo::discover(f.path()).unwrap();
    assert!(repo.commit_ids(None).unwrap().is_empty());
}

#[test]
fn load_commits_fills_all_fields() {
    let (f, c) = repo_with_branches();
    let repo = GitRepo::discover(f.path()).unwrap();
    let infos = repo.load_commits(&[c[1].clone()]).unwrap();
    let info = &infos[0];
    assert_eq!(info.id, c[1]);
    assert_eq!(info.short_id, c[1][..7]);
    assert_eq!(info.summary, "c2 main work");
    assert_eq!(info.parents, vec![c[0].clone()]);
    assert_eq!(info.author_name, "Test Author");
    assert_eq!(info.author_email, "test@example.com");
    assert_eq!(info.timestamp, 2_000);
}

#[test]
fn load_commits_in_chunks_equals_loading_all_at_once() {
    let (f, _) = repo_with_branches();
    let repo = GitRepo::discover(f.path()).unwrap();
    let ids = repo.commit_ids(None).unwrap();
    let all = repo.load_commits(&ids).unwrap();
    let mut chunked = repo.load_commits(&ids[..2]).unwrap();
    chunked.extend(repo.load_commits(&ids[2..]).unwrap());
    assert_eq!(all, chunked);
}
```

- [ ] **Step 2: 跑測試確認失敗**

Run: `cargo test --test git_repo 2>&1 | tail -5`
Expected: 編譯錯誤 `no method named commit_ids`。

- [ ] **Step 3: 實作（追加到 src/git/repo.rs 的 impl GitRepo）**

檔頭 use 追加：`use git2::{Oid, Sort};`，types use 追加 `CommitInfo`（`Oid` 只在函式內部使用，不出現在公開簽名）。

```rust
    /// Walk the whole commit DAG (oids only — cheap even for huge repos).
    /// `filter`: a full refname such as "refs/heads/main" limits the walk
    /// to commits reachable from that ref; None walks every ref plus HEAD.
    pub fn commit_ids(&self, filter: Option<&str>) -> Result<Vec<CommitId>> {
        let mut walk = self.inner.revwalk()?;
        walk.set_sorting(Sort::TOPOLOGICAL | Sort::TIME)?;
        match filter {
            Some(refname) => {
                walk.push_ref(refname)
                    .with_context(|| format!("unknown ref: {refname}"))?;
            }
            None => {
                walk.push_glob("refs/heads/*")?;
                walk.push_glob("refs/tags/*")?;
                walk.push_glob("refs/remotes/*")?;
                if self.inner.head().is_ok() {
                    walk.push_head()?;
                }
            }
        }
        Ok(walk
            .filter_map(|o| o.ok())
            .map(|oid| oid.to_string())
            .collect())
    }

    /// Convert one chunk of commit ids into full CommitInfo values.
    pub fn load_commits(&self, ids: &[CommitId]) -> Result<Vec<CommitInfo>> {
        ids.iter()
            .map(|id| {
                let oid = Oid::from_str(id).context("invalid commit id")?;
                let c = self
                    .inner
                    .find_commit(oid)
                    .with_context(|| format!("commit {id} not found"))?;
                let author = c.author();
                Ok(CommitInfo {
                    short_id: id[..7].to_string(),
                    id: id.clone(),
                    parents: c.parent_ids().map(|p| p.to_string()).collect(),
                    summary: c.summary().ok().flatten().unwrap_or("").to_string(),
                    message: c.message().unwrap_or("").to_string(),
                    author_name: author.name().unwrap_or("").to_string(),
                    author_email: author.email().unwrap_or("").to_string(),
                    timestamp: author.when().seconds(),
                })
            })
            .collect()
    }
```

- [ ] **Step 4: 跑測試確認通過**

Run: `cargo test --test git_repo 2>&1 | tail -3`
Expected: `test result: ok. 10 passed; 0 failed`

- [ ] **Step 5: 品質閘門後 commit**

Run: `cargo clippy --all-targets -- -D warnings && cargo fmt --check`
Expected: 無輸出。

```bash
git add src/git/repo.rs tests/git_repo.rs
git commit -m "feat: topological commit walk with chunked loading and ref filter"
```

---

### Task 4: diff 層（commit 檔案列表、單檔 diff、工作區狀態）

**Files:**
- Create: `src/git/diff.rs`
- Modify: `src/git/mod.rs`（加 `pub mod diff;`）
- Test: `tests/git_diff.rs`

**Interfaces:**
- Consumes: Task 2 的 `GitRepo`、`FileChange`、`ChangeKind`、`DiffLine`、`Fixture`
- Produces（全部掛在 `impl GitRepo`，由 Task 9/10 的 App 呼叫）:
  - `commit_files(&self, id: &CommitId) -> Result<Vec<FileChange>>` — 對第一個 parent 的變更（root commit 對空樹）
  - `commit_file_diff(&self, id: &CommitId, path: &str) -> Result<Vec<DiffLine>>`
  - `worktree_status(&self) -> Result<Vec<FileChange>>` — HEAD 樹 vs 工作區+index（staged+unstaged 合併），含 untracked
  - `worktree_file_diff(&self, path: &str) -> Result<Vec<DiffLine>>`

- [ ] **Step 1: 寫失敗測試 tests/git_diff.rs**

```rust
mod common;

use common::Fixture;
use gitgraph_tui::git::types::ChangeKind;
use gitgraph_tui::git::GitRepo;

#[test]
fn commit_files_reports_kinds_and_line_counts() {
    let f = Fixture::new();
    let c1 = f.commit(
        "base",
        &[("keep.txt", "one\ntwo\n"), ("gone.txt", "bye\n")],
        &[],
        &[],
        1_000,
    );
    let c2 = f.commit(
        "changes",
        &[("keep.txt", "one\nTWO\nthree\n"), ("new.txt", "hi\n")],
        &["gone.txt"],
        &[c1],
        2_000,
    );
    f.branch("main", c2);
    f.set_head("refs/heads/main");
    let repo = GitRepo::discover(f.path()).unwrap();
    let files = repo.commit_files(&c2.to_string()).unwrap();
    let by_path = |p: &str| files.iter().find(|c| c.path == p).unwrap_or_else(|| panic!("missing {p}"));
    assert_eq!(by_path("new.txt").kind, ChangeKind::Added);
    assert_eq!(by_path("new.txt").additions, 1);
    assert_eq!(by_path("gone.txt").kind, ChangeKind::Deleted);
    let keep = by_path("keep.txt");
    assert_eq!(keep.kind, ChangeKind::Modified);
    assert_eq!(keep.additions, 2); // TWO + three
    assert_eq!(keep.deletions, 1); // two
}

#[test]
fn root_commit_diffs_against_the_empty_tree() {
    let f = Fixture::new();
    let c1 = f.commit("init", &[("a.txt", "1\n")], &[], &[], 1_000);
    f.branch("main", c1);
    f.set_head("refs/heads/main");
    let repo = GitRepo::discover(f.path()).unwrap();
    let files = repo.commit_files(&c1.to_string()).unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].kind, ChangeKind::Added);
}

#[test]
fn renames_are_detected_when_content_is_identical() {
    let f = Fixture::new();
    let c1 = f.commit("base", &[("old_name.txt", "same content\nlines\n")], &[], &[], 1_000);
    let c2 = f.commit(
        "rename",
        &[("new_name.txt", "same content\nlines\n")],
        &["old_name.txt"],
        &[c1],
        2_000,
    );
    f.branch("main", c2);
    f.set_head("refs/heads/main");
    let repo = GitRepo::discover(f.path()).unwrap();
    let files = repo.commit_files(&c2.to_string()).unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].path, "new_name.txt");
    assert_eq!(
        files[0].kind,
        ChangeKind::Renamed { from: "old_name.txt".to_string() }
    );
}

#[test]
fn binary_files_are_flagged() {
    let f = Fixture::new();
    let c1 = f.commit("bin", &[("blob.bin", "a\0b\0c")], &[], &[], 1_000);
    f.branch("main", c1);
    f.set_head("refs/heads/main");
    let repo = GitRepo::discover(f.path()).unwrap();
    let files = repo.commit_files(&c1.to_string()).unwrap();
    assert!(files[0].is_binary);
    let lines = repo.commit_file_diff(&c1.to_string(), "blob.bin").unwrap();
    assert!(lines.iter().any(|l| l.origin == 'B'));
}

#[test]
fn commit_file_diff_has_hunk_header_and_signed_lines() {
    let f = Fixture::new();
    let c1 = f.commit("base", &[("a.txt", "one\ntwo\n")], &[], &[], 1_000);
    let c2 = f.commit("edit", &[("a.txt", "one\nTWO\n")], &[], &[c1], 2_000);
    f.branch("main", c2);
    f.set_head("refs/heads/main");
    let repo = GitRepo::discover(f.path()).unwrap();
    let lines = repo.commit_file_diff(&c2.to_string(), "a.txt").unwrap();
    assert!(lines.iter().any(|l| l.origin == '@' && l.content.starts_with("@@")));
    assert!(lines.iter().any(|l| l.origin == '-' && l.content == "two"));
    assert!(lines.iter().any(|l| l.origin == '+' && l.content == "TWO"));
    assert!(lines.iter().any(|l| l.origin == ' ' && l.content == "one"));
}

#[test]
fn merge_commit_diffs_against_its_first_parent_only() {
    let f = Fixture::new();
    let base = f.commit("base", &[("a.txt", "base\n")], &[], &[], 1_000);
    let main2 = f.commit("main change", &[("a.txt", "main\n")], &[], &[base], 2_000);
    let feat = f.commit("feature adds", &[("feat.txt", "x\n")], &[], &[base], 3_000);
    let merge = f.commit("merge", &[], &[], &[main2, feat], 4_000);
    f.branch("main", merge);
    f.set_head("refs/heads/main");
    let repo = GitRepo::discover(f.path()).unwrap();
    let files = repo.commit_files(&merge.to_string()).unwrap();
    let paths: Vec<&str> = files.iter().map(|c| c.path.as_str()).collect();
    assert!(paths.contains(&"feat.txt"), "second-parent change appears vs first parent");
    assert!(!paths.contains(&"a.txt"), "first-parent content is not a change");
}

#[test]
fn worktree_status_merges_staged_unstaged_and_untracked() {
    let f = Fixture::new();
    let c1 = f.commit("base", &[("tracked.txt", "old\n")], &[], &[], 1_000);
    f.branch("main", c1);
    f.set_head("refs/heads/main");
    f.write_file("tracked.txt", "new\n"); // unstaged modify
    f.write_file("untracked.txt", "hello\n"); // untracked
    let repo = GitRepo::discover(f.path()).unwrap();
    let files = repo.worktree_status().unwrap();
    let paths: Vec<&str> = files.iter().map(|c| c.path.as_str()).collect();
    assert!(paths.contains(&"tracked.txt"));
    assert!(paths.contains(&"untracked.txt"));
    let lines = repo.worktree_file_diff("tracked.txt").unwrap();
    assert!(lines.iter().any(|l| l.origin == '+' && l.content == "new"));
}

#[test]
fn clean_worktree_status_is_empty() {
    let f = Fixture::new();
    let c1 = f.commit("base", &[("a.txt", "1\n")], &[], &[], 1_000);
    f.branch("main", c1);
    f.set_head("refs/heads/main");
    let repo = GitRepo::discover(f.path()).unwrap();
    assert!(repo.worktree_status().unwrap().is_empty());
}

#[test]
fn worktree_status_on_empty_repo_lists_untracked_files() {
    let f = Fixture::new();
    f.write_file("first.txt", "hi\n");
    let repo = GitRepo::discover(f.path()).unwrap();
    let files = repo.worktree_status().unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].kind, ChangeKind::Added);
}
```

- [ ] **Step 2: 跑測試確認失敗**

Run: `cargo test --test git_diff 2>&1 | tail -5`
Expected: 編譯錯誤 `no method named commit_files`。

- [ ] **Step 3: 實作 src/git/diff.rs**（並在 `src/git/mod.rs` 加一行 `pub mod diff;`）

```rust
//! Diff extraction: per-commit file lists, single-file diffs, worktree status.
use std::cell::RefCell;

use anyhow::{Context, Result};
use git2::{Delta, Diff, DiffOptions, Oid};

use super::repo::GitRepo;
use super::types::{ChangeKind, CommitId, DiffLine, FileChange};

impl GitRepo {
    /// Files changed by a commit, diffed against its first parent
    /// (or the empty tree for a root commit). Renames are detected.
    pub fn commit_files(&self, id: &CommitId) -> Result<Vec<FileChange>> {
        let mut diff = self.commit_diff(id, None)?;
        diff.find_similar(None)?;
        collect_file_changes(&diff)
    }

    /// Unified diff of one file within a commit.
    pub fn commit_file_diff(&self, id: &CommitId, path: &str) -> Result<Vec<DiffLine>> {
        let diff = self.commit_diff(id, Some(path))?;
        collect_diff_lines(&diff)
    }

    /// Combined staged + unstaged + untracked changes (HEAD tree vs
    /// workdir-with-index) — the "Uncommitted changes" row.
    pub fn worktree_status(&self) -> Result<Vec<FileChange>> {
        let mut diff = self.worktree_diff(None)?;
        diff.find_similar(None)?;
        collect_file_changes(&diff)
    }

    /// Unified diff of one uncommitted file.
    pub fn worktree_file_diff(&self, path: &str) -> Result<Vec<DiffLine>> {
        let diff = self.worktree_diff(Some(path))?;
        collect_diff_lines(&diff)
    }

    fn commit_diff(&self, id: &CommitId, pathspec: Option<&str>) -> Result<Diff<'_>> {
        let oid = Oid::from_str(id).context("invalid commit id")?;
        let commit = self.inner.find_commit(oid)?;
        let tree = commit.tree()?;
        let parent_tree = commit.parent(0).ok().map(|p| p.tree()).transpose()?;
        let mut opts = DiffOptions::new();
        opts.context_lines(3);
        if let Some(p) = pathspec {
            opts.pathspec(p);
        }
        Ok(self
            .inner
            .diff_tree_to_tree(parent_tree.as_ref(), Some(&tree), Some(&mut opts))?)
    }

    fn worktree_diff(&self, pathspec: Option<&str>) -> Result<Diff<'_>> {
        let head_tree = self.inner.head().ok().and_then(|h| h.peel_to_tree().ok());
        let mut opts = DiffOptions::new();
        opts.include_untracked(true)
            .recurse_untracked_dirs(true)
            .show_untracked_content(true)
            .context_lines(3);
        if let Some(p) = pathspec {
            opts.pathspec(p);
        }
        Ok(self
            .inner
            .diff_tree_to_workdir_with_index(head_tree.as_ref(), Some(&mut opts))?)
    }
}

/// Fold a Diff into per-file changes with +/- line counts.
/// Callbacks arrive file-by-file, so "last pushed entry" is always the
/// delta currently being walked.
fn collect_file_changes(diff: &Diff) -> Result<Vec<FileChange>> {
    let files: RefCell<Vec<FileChange>> = RefCell::new(Vec::new());
    diff.foreach(
        &mut |delta, _| {
            let path = delta
                .new_file()
                .path()
                .or_else(|| delta.old_file().path())
                .map(|p| p.display().to_string())
                .unwrap_or_default();
            let kind = match delta.status() {
                Delta::Added | Delta::Untracked => ChangeKind::Added,
                Delta::Deleted => ChangeKind::Deleted,
                Delta::Renamed => ChangeKind::Renamed {
                    from: delta
                        .old_file()
                        .path()
                        .map(|p| p.display().to_string())
                        .unwrap_or_default(),
                },
                _ => ChangeKind::Modified,
            };
            files.borrow_mut().push(FileChange {
                path,
                kind,
                additions: 0,
                deletions: 0,
                is_binary: delta.flags().is_binary(),
            });
            true
        },
        Some(&mut |_delta, _binary| {
            if let Some(last) = files.borrow_mut().last_mut() {
                last.is_binary = true;
            }
            true
        }),
        None,
        Some(&mut |_delta, _hunk, line| {
            if let Some(last) = files.borrow_mut().last_mut() {
                match line.origin() {
                    '+' => last.additions += 1,
                    '-' => last.deletions += 1,
                    _ => {}
                }
            }
            true
        }),
    )?;
    Ok(files.into_inner())
}

/// Flatten a Diff into displayable lines (hunk headers included).
fn collect_diff_lines(diff: &Diff) -> Result<Vec<DiffLine>> {
    let lines: RefCell<Vec<DiffLine>> = RefCell::new(Vec::new());
    diff.foreach(
        &mut |_delta, _| true,
        Some(&mut |_delta, _binary| {
            lines.borrow_mut().push(DiffLine {
                origin: 'B',
                content: "(binary file)".to_string(),
            });
            true
        }),
        Some(&mut |_delta, hunk| {
            lines.borrow_mut().push(DiffLine {
                origin: '@',
                content: String::from_utf8_lossy(hunk.header()).trim_end().to_string(),
            });
            true
        }),
        Some(&mut |_delta, _hunk, line| {
            let origin = line.origin();
            if matches!(origin, '+' | '-' | ' ') {
                lines.borrow_mut().push(DiffLine {
                    origin,
                    content: String::from_utf8_lossy(line.content())
                        .trim_end_matches('\n')
                        .to_string(),
                });
            }
            true
        }),
    )?;
    Ok(lines.into_inner())
}
```

- [ ] **Step 4: 跑測試確認通過**

Run: `cargo test --test git_diff 2>&1 | tail -3`
Expected: `test result: ok. 9 passed; 0 failed`
若 rename 測試失敗於 `Delta::Renamed` 未出現：改用 `git2::DiffFindOptions::new().renames(true)` 傳入 `find_similar(Some(&mut opts))` 再試。

- [ ] **Step 5: 品質閘門後 commit**

Run: `cargo clippy --all-targets -- -D warnings && cargo fmt --check && cargo test 2>&1 | tail -3`
Expected: 閘門無輸出；全部測試 `ok`。

```bash
git add src/git/ tests/git_diff.rs
git commit -m "feat: commit file lists, single-file diffs, and worktree status"
```

---

### Task 5: Graph 佈局引擎（lane 分配 + cell 渲染）

全案風險核心。純函式、無 IO；測試直接寫在模組內（要驗證增量狀態）。

**Files:**
- Create: `src/graph/mod.rs`
- Create: `src/graph/layout.rs`
- Modify: `src/lib.rs`（加 `pub mod graph;`）

**Interfaces:**
- Consumes: Task 2 的 `CommitInfo`（只用 `id` 和 `parents` 欄位）
- Produces:
  - `graph::layout::PALETTE_SIZE: usize = 8`
  - `graph::layout::Cell { glyph: char, color: usize }`（color 是 0..8 的色盤索引）
  - `graph::layout::GraphRow { lane: usize, color: usize, cells: Vec<Cell> }`（每 lane 佔 2 格：偶數格是 lane 字元、奇數格是水平連接）
  - `graph::layout::LayoutEngine`：`new()`、`reset()`、`process(&mut self, commits: &[CommitInfo]) -> Vec<GraphRow>`（可分塊重複呼叫，lane 狀態跨塊延續）

**演算法**（spec §4）：由上而下逐 commit 處理，維護「進行中的 lanes」（每條記著等待中的 parent id）。commit C 落在等它的最左 lane（其餘等它的 lanes 畫 `╯`/`╰` 收攏關閉）；沒人等它就開新 lane。C 的第一個 parent 接手該 lane；其餘 parents 各開新 lane 畫 `╮`/`╭`。同一 parent 不去重（多條 lane 等同一 parent 時，會在該 parent 的列一起收攏——圖稍寬但正確）。開新 lane 時避開本列剛關閉的 slot，避免 `╯` 被 `╮` 蓋掉。

- [ ] **Step 1: 建 src/graph/mod.rs 與 lib.rs 掛載**

```rust
// src/graph/mod.rs
pub mod layout;

pub use layout::{Cell, GraphRow, LayoutEngine};
```

```rust
// src/lib.rs（全文替換）
pub mod git;
pub mod graph;
```

- [ ] **Step 2: 寫 src/graph/layout.rs 的失敗測試**（先只貼下方測試區塊——`LayoutEngine` 還不存在，紅燈就是編譯錯誤；Step 4 再貼實作轉綠）

測試（放在 layout.rs 底部）：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::types::CommitInfo;

    /// Minimal commit for layout purposes.
    fn c(id: &str, parents: &[&str]) -> CommitInfo {
        CommitInfo {
            id: id.to_string(),
            short_id: id.chars().take(7).collect(),
            parents: parents.iter().map(|s| s.to_string()).collect(),
            summary: format!("commit {id}"),
            message: String::new(),
            author_name: "t".to_string(),
            author_email: "t@t".to_string(),
            timestamp: 0,
        }
    }

    /// Rows as glyph strings (trailing spaces trimmed) — readable assertions.
    fn glyphs(rows: &[GraphRow]) -> Vec<String> {
        rows.iter()
            .map(|r| {
                r.cells
                    .iter()
                    .map(|c| c.glyph)
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect()
    }

    #[test]
    fn linear_history_stays_in_lane_zero() {
        let mut e = LayoutEngine::new();
        let rows = e.process(&[c("c3", &["c2"]), c("c2", &["c1"]), c("c1", &[])]);
        assert_eq!(glyphs(&rows), vec!["●", "●", "●"]);
        assert!(rows.iter().all(|r| r.lane == 0 && r.color == 0));
    }

    #[test]
    fn branch_forks_then_joins() {
        // main: m2→m1 · feature: f1→m1 · order: m2, f1, m1
        let mut e = LayoutEngine::new();
        let rows = e.process(&[c("m2", &["m1"]), c("f1", &["m1"]), c("m1", &[])]);
        assert_eq!(glyphs(&rows), vec!["●", "│ ●", "●─╯"]);
        assert_eq!(rows[1].lane, 1);
        assert_eq!(rows[1].color, 1); // second lane gets the next palette color
        assert_eq!(rows[2].lane, 0);
    }

    #[test]
    fn merge_commit_opens_a_lane_for_its_second_parent() {
        // m3 = merge(m2, f1); m2→m1; f1→m1; m1 root
        let mut e = LayoutEngine::new();
        let rows = e.process(&[
            c("m3", &["m2", "f1"]),
            c("m2", &["m1"]),
            c("f1", &["m1"]),
            c("m1", &[]),
        ]);
        assert_eq!(glyphs(&rows), vec!["●─╮", "● │", "│ ●", "●─╯"]);
    }

    #[test]
    fn octopus_merge_opens_multiple_lanes() {
        let mut e = LayoutEngine::new();
        let rows = e.process(&[
            c("o", &["a", "b", "x"]),
            c("a", &[]),
            c("b", &[]),
            c("x", &[]),
        ]);
        assert_eq!(glyphs(&rows), vec!["●─╮─╮", "● │ │", "  ● │", "    ●"]);
    }

    #[test]
    fn criss_cross_crossing_uses_the_cross_glyph() {
        // a=merge(c,d), b=merge(c,d) — the close of b's lane at c crosses
        // a's second-parent lane.
        let mut e = LayoutEngine::new();
        let rows = e.process(&[
            c("a", &["c", "d"]),
            c("b", &["c", "d"]),
            c("c", &[]),
            c("d", &[]),
        ]);
        assert_eq!(
            glyphs(&rows),
            vec!["●─╮", "│ │ ●─╮", "●─┼─╯ │", "  ●───╯"]
        );
    }

    #[test]
    fn disconnected_histories_reuse_freed_lanes() {
        let mut e = LayoutEngine::new();
        let rows = e.process(&[
            c("a2", &["a1"]),
            c("b2", &["b1"]),
            c("a1", &[]), // frees lane 0
            c("b1", &[]),
        ]);
        assert_eq!(glyphs(&rows), vec!["●", "│ ●", "● │", "  ●"]);
    }

    #[test]
    fn chunked_processing_matches_single_pass() {
        let commits = [
            c("m3", &["m2", "f1"]),
            c("m2", &["m1"]),
            c("f1", &["m1"]),
            c("m1", &[]),
        ];
        let mut whole = LayoutEngine::new();
        let all = whole.process(&commits);
        let mut chunked = LayoutEngine::new();
        let mut rows = chunked.process(&commits[..2]);
        rows.extend(chunked.process(&commits[2..]));
        assert_eq!(all, rows);
    }

    #[test]
    fn colors_cycle_through_the_palette() {
        // 9 parallel tips: the 9th lane wraps to color 0
        let mut e = LayoutEngine::new();
        let tips: Vec<CommitInfo> = (0..9).map(|i| c(&format!("t{i}"), &["root"])).collect();
        let rows = e.process(&tips);
        assert_eq!(rows[8].color, 8 % PALETTE_SIZE);
        assert_eq!(rows[8].lane, 8);
    }

    #[test]
    fn reset_clears_lane_state() {
        let mut e = LayoutEngine::new();
        e.process(&[c("a", &["p"])]);
        e.reset();
        let rows = e.process(&[c("b", &[])]);
        assert_eq!(rows[0].lane, 0);
        assert_eq!(rows[0].color, 0);
    }
}
```

- [ ] **Step 3: 跑測試確認失敗**

Run: `cargo test --lib graph 2>&1 | tail -5`
Expected: 編譯錯誤（`LayoutEngine` 不存在）。

- [ ] **Step 4: 實作 src/graph/layout.rs（測試上方）**

```rust
//! Pure lane-assignment layout: commit DAG (topological order) → drawable
//! rows of colored glyphs. No IO, no git2, no ratatui.
use crate::git::types::{CommitId, CommitInfo};

pub const PALETTE_SIZE: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    pub glyph: char,
    /// Palette index 0..PALETTE_SIZE. The UI maps it to a real color.
    pub color: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphRow {
    /// Lane of the commit dot. Even cell index = 2 * lane.
    pub lane: usize,
    pub color: usize,
    pub cells: Vec<Cell>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Lane {
    waiting_for: CommitId,
    color: usize,
}

/// Keeps open-lane state between chunks so loading is incremental.
#[derive(Debug, Default)]
pub struct LayoutEngine {
    slots: Vec<Option<Lane>>,
    next_color: usize,
}

impl LayoutEngine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        self.slots.clear();
        self.next_color = 0;
    }

    /// Process the next chunk of commits (topological order), continuing
    /// from the lane state left by previous calls.
    pub fn process(&mut self, commits: &[CommitInfo]) -> Vec<GraphRow> {
        commits.iter().map(|c| self.process_one(c)).collect()
    }

    fn process_one(&mut self, commit: &CommitInfo) -> GraphRow {
        let waiting: Vec<usize> = (0..self.slots.len())
            .filter(|&i| {
                matches!(&self.slots[i], Some(l) if l.waiting_for == commit.id)
            })
            .collect();
        // The commit sits on the leftmost lane that waits for it, or a new one.
        let (lane, color) = match waiting.first() {
            Some(&i) => (i, self.slots[i].as_ref().unwrap().color),
            None => self.alloc(commit.id.clone(), &[]),
        };
        // Other lanes waiting for this commit join it here and close.
        let closes: Vec<(usize, usize)> = waiting
            .iter()
            .skip(1) // waiting[0] is the commit's own lane; slicing [1..] would panic when empty
            .map(|&i| (i, self.slots[i].as_ref().unwrap().color))
            .collect();
        for &(i, _) in &closes {
            self.slots[i] = None;
        }
        // Lanes that continue straight through this row.
        let pass: Vec<(usize, usize)> = (0..self.slots.len())
            .filter(|&i| i != lane)
            .filter_map(|i| self.slots[i].as_ref().map(|l| (i, l.color)))
            .collect();
        // First parent inherits the commit's lane; a root closes it.
        match commit.parents.first() {
            Some(p) => {
                self.slots[lane] = Some(Lane { waiting_for: p.clone(), color });
            }
            None => self.slots[lane] = None,
        }
        // Remaining parents (merge sources) each open a new lane. Avoid slots
        // closed on this very row so the closing glyph stays visible.
        let closed_now: Vec<usize> = closes.iter().map(|&(i, _)| i).collect();
        let opens: Vec<(usize, usize)> = commit
            .parents
            .iter()
            .skip(1)
            .map(|p| self.alloc(p.clone(), &closed_now))
            .collect();
        self.trim();
        let cells = render_cells(lane, color, &closes, &opens, &pass);
        GraphRow { lane, color, cells }
    }

    /// Allocate the leftmost free slot not in `avoid`; assign the next
    /// palette color. Returns (lane index, color).
    fn alloc(&mut self, waiting_for: CommitId, avoid: &[usize]) -> (usize, usize) {
        let color = self.next_color % PALETTE_SIZE;
        self.next_color += 1;
        let free = (0..self.slots.len())
            .find(|i| self.slots[*i].is_none() && !avoid.contains(i));
        let lane = match free {
            Some(i) => i,
            None => {
                self.slots.push(None);
                self.slots.len() - 1
            }
        };
        self.slots[lane] = Some(Lane { waiting_for, color });
        (lane, color)
    }

    fn trim(&mut self) {
        while matches!(self.slots.last(), Some(None)) {
            self.slots.pop();
        }
    }
}

/// Paint one row: pass-through verticals first, then connector curves,
/// finally the commit dot (dots always win).
fn render_cells(
    lane: usize,
    color: usize,
    closes: &[(usize, usize)],
    opens: &[(usize, usize)],
    pass: &[(usize, usize)],
) -> Vec<Cell> {
    let max_lane = std::iter::once(lane)
        .chain(closes.iter().map(|&(i, _)| i))
        .chain(opens.iter().map(|&(i, _)| i))
        .chain(pass.iter().map(|&(i, _)| i))
        .max()
        .unwrap_or(0);
    let mut cells = vec![Cell { glyph: ' ', color: 0 }; 2 * (max_lane + 1)];
    for &(i, c) in pass {
        cells[2 * i] = Cell { glyph: '│', color: c };
    }
    for &(i, c) in closes {
        let end = if i > lane { '╯' } else { '╰' };
        connector(&mut cells, lane, i, c, end);
    }
    for &(i, c) in opens {
        let end = if i > lane { '╮' } else { '╭' };
        connector(&mut cells, lane, i, c, end);
    }
    cells[2 * lane] = Cell { glyph: '●', color };
    cells
}

/// Horizontal run from the commit's lane to `to`, ending in a curve glyph.
/// Crossing a vertical becomes '┼' (keeping the vertical's color); existing
/// curves from earlier connectors are left intact.
fn connector(cells: &mut [Cell], from: usize, to: usize, color: usize, end: char) {
    let (lo, hi) = (from.min(to), from.max(to));
    for cell in &mut cells[(2 * lo + 1)..(2 * hi)] {
        *cell = match cell.glyph {
            '│' | '┼' => Cell { glyph: '┼', color: cell.color },
            ' ' | '─' => Cell { glyph: '─', color },
            other => Cell { glyph: other, color: cell.color },
        };
    }
    cells[2 * to] = Cell { glyph: end, color };
}
```

- [ ] **Step 5: 跑測試確認通過**

Run: `cargo test --lib graph 2>&1 | tail -3`
Expected: `test result: ok. 9 passed; 0 failed`
若 glyph 字串 assertion 失敗：先手算該 DAG 每列的 lane 事件（誰等誰、誰開誰關），修實作而不是改測試期望——期望值已在計畫階段人工驗算過。

- [ ] **Step 6: 品質閘門後 commit**

Run: `cargo clippy --all-targets -- -D warnings && cargo fmt --check && cargo test 2>&1 | tail -3`
Expected: 閘門無輸出；全部測試 `ok`。

```bash
git add src/lib.rs src/graph/
git commit -m "feat: pure lane-assignment graph layout engine"
```

---

### Task 6: UI 純函式工具（時間格式、寬度截斷、色盤、ref 樣式）

**Files:**
- Create: `src/ui/mod.rs`
- Create: `src/ui/util.rs`
- Modify: `src/lib.rs`（加 `pub mod ui;`）

**Interfaces:**
- Consumes: Task 2 的 `RefKind`
- Produces:
  - `ui::util::relative_time(ts: i64, now: i64) -> String`（"now"/"5m"/"2h"/"3d"/"2mo"/"1y"）
  - `ui::util::absolute_time(ts: i64) -> String`（"2026-07-06 14:30" UTC）
  - `ui::util::truncate_width(s: &str, max: usize) -> String`（unicode 顯示寬度截斷，超長以 `…` 結尾）
  - `ui::util::pad_to_width(s: &str, width: usize) -> String`
  - `ui::lane_color(idx: usize) -> ratatui::style::Color`（8 色盤循環）
  - `ui::ref_style(kind: RefKind) -> ratatui::style::Style`
  - `ui::render(frame, app)` 之後由 Task 7 開始填內容；本 task 先不建 render。

- [ ] **Step 1: 寫 src/ui/util.rs（含模組內失敗測試）**

```rust
//! Pure formatting helpers for the UI layer.
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Compact relative age, Git-Graph style.
pub fn relative_time(ts: i64, now: i64) -> String {
    let d = (now - ts).max(0);
    const MIN: i64 = 60;
    const HOUR: i64 = 3_600;
    const DAY: i64 = 86_400;
    const MONTH: i64 = 30 * DAY;
    const YEAR: i64 = 365 * DAY;
    match d {
        _ if d < MIN => "now".to_string(),
        _ if d < HOUR => format!("{}m", d / MIN),
        _ if d < DAY => format!("{}h", d / HOUR),
        _ if d < MONTH => format!("{}d", d / DAY),
        _ if d < YEAR => format!("{}mo", d / MONTH),
        _ => format!("{}y", d / YEAR),
    }
}

/// "YYYY-MM-DD HH:MM" in UTC, for the detail panel.
pub fn absolute_time(ts: i64) -> String {
    use time::macros::format_description;
    let Ok(dt) = time::OffsetDateTime::from_unix_timestamp(ts) else {
        return "?".to_string();
    };
    let fmt = format_description!("[year]-[month]-[day] [hour]:[minute]");
    dt.format(&fmt).unwrap_or_else(|_| "?".to_string())
}

/// Truncate to a display width (CJK-aware); appends '…' when cut.
pub fn truncate_width(s: &str, max: usize) -> String {
    if s.width() <= max {
        return s.to_string();
    }
    let budget = max.saturating_sub(1);
    let mut out = String::new();
    let mut used = 0;
    for ch in s.chars() {
        let w = ch.width().unwrap_or(0);
        if used + w > budget {
            break;
        }
        out.push(ch);
        used += w;
    }
    out.push('…');
    out
}

/// Right-pad with spaces to an exact display width (input must already fit).
pub fn pad_to_width(s: &str, width: usize) -> String {
    let pad = width.saturating_sub(s.width());
    format!("{s}{}", " ".repeat(pad))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_time_buckets() {
        let now = 1_000_000_000;
        assert_eq!(relative_time(now - 5, now), "now");
        assert_eq!(relative_time(now - 300, now), "5m");
        assert_eq!(relative_time(now - 7_200, now), "2h");
        assert_eq!(relative_time(now - 3 * 86_400, now), "3d");
        assert_eq!(relative_time(now - 65 * 86_400, now), "2mo");
        assert_eq!(relative_time(now - 800 * 86_400, now), "2y");
        assert_eq!(relative_time(now + 999, now), "now"); // clock skew clamps
    }

    #[test]
    fn absolute_time_formats_utc() {
        assert_eq!(absolute_time(0), "1970-01-01 00:00");
    }

    #[test]
    fn truncate_respects_cjk_double_width() {
        assert_eq!(truncate_width("hello", 10), "hello");
        assert_eq!(truncate_width("hello world", 8), "hello w…");
        // Each CJK char is width 2: "訊息" = 4 columns.
        assert_eq!(truncate_width("訊息訊息", 5), "訊息…");
    }

    #[test]
    fn pad_fills_to_exact_width() {
        assert_eq!(pad_to_width("ab", 4), "ab  ");
        assert_eq!(pad_to_width("訊息", 6), "訊息  ");
    }
}
```

- [ ] **Step 2: 跑測試確認失敗**

Run: `cargo test --lib ui 2>&1 | tail -5`
Expected: 通過但 **0 個測試被執行**——util.rs 還是孤兒檔案，lib.rs 尚未掛 `pub mod ui;`，cargo 根本沒編譯它。這一步的「紅」是「測試不存在」；Step 3 掛上模組後測試才會編譯執行。

- [ ] **Step 3: 寫 src/ui/mod.rs 並掛上 lib.rs**

```rust
// src/ui/mod.rs
pub mod util;

use ratatui::style::{Color, Modifier, Style};

use crate::git::types::RefKind;
use crate::graph::layout::PALETTE_SIZE;

/// Lane palette; index comes from the layout engine (0..PALETTE_SIZE).
pub const PALETTE: [Color; PALETTE_SIZE] = [
    Color::Cyan,
    Color::Magenta,
    Color::Blue,
    Color::Green,
    Color::Yellow,
    Color::Red,
    Color::LightCyan,
    Color::LightMagenta,
];

pub fn lane_color(idx: usize) -> Color {
    PALETTE[idx % PALETTE.len()]
}

pub fn ref_style(kind: RefKind) -> Style {
    match kind {
        RefKind::Head => Style::new().fg(Color::LightGreen).add_modifier(Modifier::BOLD),
        RefKind::LocalBranch => Style::new().fg(Color::Green),
        RefKind::RemoteBranch => Style::new().fg(Color::Blue),
        RefKind::Tag => Style::new().fg(Color::Yellow),
    }
}
```

```rust
// src/lib.rs（全文替換）
pub mod git;
pub mod graph;
pub mod ui;
```

- [ ] **Step 4: 跑測試確認通過**

Run: `cargo test --lib 2>&1 | tail -3`
Expected: `test result: ok. 13 passed; 0 failed`（graph 9 + util 4）

- [ ] **Step 5: 品質閘門後 commit**

Run: `cargo clippy --all-targets -- -D warnings && cargo fmt --check`
Expected: 無輸出。

```bash
git add src/lib.rs src/ui/
git commit -m "feat: ui formatting helpers, palette, and ref styles"
```

---

### Task 7: App 狀態機、事件迴圈、terminal 生命週期

**Files:**
- Create: `src/app.rs`
- Create: `src/event.rs`
- Modify: `src/lib.rs`（加 `pub mod app; pub mod event;`）
- Modify: `src/main.rs`（全文替換）
- Modify: `src/ui/mod.rs`（加最小 `render`）
- Test: `tests/app_state.rs`

**Interfaces:**
- Consumes: Task 2–5 的 `GitRepo`（refs/ref_map/commit_ids/load_commits/worktree_status）、`LayoutEngine`、`Fixture`
- Produces（後續所有 task 都在這上面加功能）:
  - `app::Mode { Normal, Search, Diff, BranchFilter }`、`app::Focus { Commits, Files }`
  - `app::SearchState { input: String, query: String, matches: Vec<usize> }`
  - `app::DiffState { title: String, lines: Vec<DiffLine>, scroll: usize }`
  - `app::App`（欄位見下方程式碼；**本 task 就定義完整欄位集**，後續 task 只加方法）
  - `App::new(repo) -> Result<App>`、`App::new_at(repo, now: i64) -> Result<App>`（測試注入時間）
  - `App::handle_key(&mut self, KeyEvent)`、`display_len()`、`total_len()`、`all_loaded()`、`uncommitted_offset()`、`selected_commit() -> Option<&CommitInfo>`
  - `event::poll_key(timeout: Duration) -> Result<Option<KeyEvent>>`
  - `ui::render(frame, &mut App)`（本 task 為殼，Task 8 填圖）

本 task 鍵位：`q`/`Esc` 離開、`j/k/↑/↓` 移動、`g/G` 頂/底；分塊懶載入在移動時觸發。

- [ ] **Step 1: 寫失敗測試 tests/app_state.rs**

```rust
mod common;

use common::Fixture;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use gitgraph_tui::app::App;
use gitgraph_tui::git::GitRepo;

pub fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

pub fn ch(c: char) -> KeyEvent {
    key(KeyCode::Char(c))
}

/// Linear history of `n` commits on main, clean worktree.
/// Returns the fixture too: its TempDir must outlive the App.
fn linear_app(n: usize, chunk: usize) -> (Fixture, App) {
    let f = Fixture::new();
    let mut prev: Vec<git2::Oid> = vec![];
    for i in 0..n {
        let oid = f.commit(
            &format!("commit {i}"),
            &[("a.txt", &format!("{i}\n"))],
            &[],
            &prev,
            1_000 + i as i64,
        );
        prev = vec![oid];
    }
    f.branch("main", prev[0]);
    f.set_head("refs/heads/main");
    let repo = GitRepo::discover(f.path()).unwrap();
    let mut app = App::new_at(repo, 10_000).unwrap();
    app.chunk_size = chunk;
    app.load_margin = 1;
    (f, app)
}

#[test]
fn new_app_loads_only_the_first_chunk() {
    let (_f, mut app) = linear_app(10, 3);
    // new_at ran with the default chunk; rebuild state with the small one
    app.reload().unwrap();
    assert_eq!(app.commits.len(), 3);
    assert_eq!(app.total_len(), 10);
    assert!(!app.all_loaded());
    assert_eq!(app.selected, 0);
}

#[test]
fn moving_down_lazily_loads_more_chunks() {
    let (_f, mut app) = linear_app(10, 3);
    app.reload().unwrap();
    for _ in 0..5 {
        app.handle_key(ch('j'));
    }
    assert_eq!(app.selected, 5);
    assert!(app.commits.len() > 3, "moving near the end must load more");
}

#[test]
fn selection_clamps_at_both_ends() {
    let (_f, mut app) = linear_app(3, 300);
    app.handle_key(ch('k'));
    assert_eq!(app.selected, 0);
    for _ in 0..99 {
        app.handle_key(ch('j'));
    }
    assert_eq!(app.selected, 2);
}

#[test]
fn capital_g_loads_everything_and_jumps_to_bottom() {
    let (_f, mut app) = linear_app(10, 3);
    app.reload().unwrap();
    app.handle_key(ch('G'));
    assert!(app.all_loaded());
    assert_eq!(app.selected, app.display_len() - 1);
    app.handle_key(ch('g'));
    assert_eq!(app.selected, 0);
}

#[test]
fn q_quits() {
    let (_f, mut app) = linear_app(2, 300);
    app.handle_key(ch('q'));
    assert!(app.should_quit);
}

#[test]
fn dirty_worktree_adds_a_synthetic_first_row() {
    let f = Fixture::new();
    let c1 = f.commit("base", &[("a.txt", "1\n")], &[], &[], 1_000);
    f.branch("main", c1);
    f.set_head("refs/heads/main");
    f.write_file("a.txt", "dirty\n");
    let repo = GitRepo::discover(f.path()).unwrap();
    let app = App::new_at(repo, 10_000).unwrap();
    assert_eq!(app.uncommitted_offset(), 1);
    assert_eq!(app.display_len(), 2);
    assert!(app.selected_commit().is_none(), "row 0 is the uncommitted row");
}

#[test]
fn selected_commit_maps_display_index_to_commit() {
    let (_f, mut app) = linear_app(3, 300);
    app.handle_key(ch('j'));
    let c = app.selected_commit().unwrap();
    assert_eq!(c.summary, "commit 1"); // topo order: newest (2) first
}

#[test]
fn empty_repo_yields_an_empty_app() {
    let f = Fixture::new();
    let repo = GitRepo::discover(f.path()).unwrap();
    let app = App::new_at(repo, 10_000).unwrap();
    assert_eq!(app.display_len(), 0);
    assert!(app.selected_commit().is_none());
}
```

（`reload()` 本 task 就實作——它是「用目前的 chunk_size/filter 重建載入狀態」的方法，Task 12 的 `r` 鍵直接重用它。）

- [ ] **Step 2: 跑測試確認失敗**

Run: `cargo test --test app_state 2>&1 | tail -5`
Expected: 編譯錯誤（`gitgraph_tui::app` 不存在）。

- [ ] **Step 3: 寫 src/app.rs**

```rust
//! Application state machine. All state changes flow through handle_key —
//! the UI layer only reads this struct.
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use lru::LruCache;
use ratatui::widgets::ListState;

use crate::git::types::{CommitId, CommitInfo, DiffLine, FileChange, RefInfo};
use crate::git::GitRepo;
use crate::graph::{GraphRow, LayoutEngine};

pub const DEFAULT_CHUNK: usize = 300;
const DETAIL_CACHE_SIZE: usize = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Search,
    Diff,
    BranchFilter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Commits,
    Files,
}

#[derive(Debug, Default)]
pub struct SearchState {
    /// Text being typed in the search bar.
    pub input: String,
    /// Last confirmed query; n/N navigate its matches.
    pub query: String,
    /// Display-row indices matching `query` (or live `input` while typing).
    pub matches: Vec<usize>,
}

#[derive(Debug)]
pub struct DiffState {
    pub title: String,
    pub lines: Vec<DiffLine>,
    pub scroll: usize,
}

pub struct App {
    pub repo: GitRepo,
    pub repo_name: String,
    pub refs: Vec<RefInfo>,
    pub ref_map: HashMap<CommitId, Vec<RefInfo>>,
    /// Full walk order (ids only); commits/rows are the loaded prefix.
    pub oids: Vec<CommitId>,
    pub commits: Vec<CommitInfo>,
    pub rows: Vec<GraphRow>,
    engine: LayoutEngine,
    /// Uncommitted worktree changes; non-empty adds a synthetic row 0.
    pub uncommitted: Vec<FileChange>,
    /// Selected display row (0 = uncommitted row when present).
    pub selected: usize,
    pub file_selected: usize,
    pub focus: Focus,
    pub mode: Mode,
    pub search: SearchState,
    pub diff: Option<DiffState>,
    pub branch_filter: Option<RefInfo>,
    /// Branch-filter popup rows; None entry = "All branches".
    pub filter_choices: Vec<Option<RefInfo>>,
    pub filter_selected: usize,
    pub list_state: ListState,
    pub detail_cache: LruCache<CommitId, Vec<FileChange>>,
    pub chunk_size: usize,
    /// Load more when selection comes within this margin of the loaded end.
    pub load_margin: usize,
    /// Unix seconds used for relative times (injected in tests).
    pub now: i64,
    pub status: String,
    pub should_quit: bool,
}

impl App {
    pub fn new(repo: GitRepo) -> Result<Self> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        Self::new_at(repo, now)
    }

    pub fn new_at(repo: GitRepo, now: i64) -> Result<Self> {
        let mut app = Self {
            repo_name: repo.name(),
            repo,
            refs: Vec::new(),
            ref_map: HashMap::new(),
            oids: Vec::new(),
            commits: Vec::new(),
            rows: Vec::new(),
            engine: LayoutEngine::new(),
            uncommitted: Vec::new(),
            selected: 0,
            file_selected: 0,
            focus: Focus::Commits,
            mode: Mode::Normal,
            search: SearchState::default(),
            diff: None,
            branch_filter: None,
            filter_choices: Vec::new(),
            filter_selected: 0,
            list_state: ListState::default(),
            detail_cache: LruCache::new(NonZeroUsize::new(DETAIL_CACHE_SIZE).unwrap()),
            chunk_size: DEFAULT_CHUNK,
            load_margin: 50,
            now,
            status: String::new(),
            should_quit: false,
        };
        app.reload()?;
        Ok(app)
    }

    /// Re-read refs and commits with the current chunk_size/branch_filter.
    /// Also serves the `r` key.
    pub fn reload(&mut self) -> Result<()> {
        self.refs = self.repo.refs()?;
        self.ref_map = GitRepo::ref_map(&self.refs);
        let filter = self.branch_filter.as_ref().map(|r| r.refname.clone());
        self.oids = self.repo.commit_ids(filter.as_deref())?;
        self.commits.clear();
        self.rows.clear();
        self.engine.reset();
        self.uncommitted = self.repo.worktree_status().unwrap_or_default();
        self.detail_cache.clear();
        self.search.matches.clear();
        self.load_next_chunk()?;
        self.selected = self.selected.min(self.display_len().saturating_sub(1));
        self.file_selected = 0;
        self.sync_list_state();
        Ok(())
    }

    pub fn uncommitted_offset(&self) -> usize {
        usize::from(!self.uncommitted.is_empty())
    }

    pub fn display_len(&self) -> usize {
        self.uncommitted_offset() + self.commits.len()
    }

    pub fn total_len(&self) -> usize {
        self.uncommitted_offset() + self.oids.len()
    }

    pub fn all_loaded(&self) -> bool {
        self.commits.len() >= self.oids.len()
    }

    /// The commit under the cursor; None when the uncommitted row (or
    /// nothing) is selected.
    pub fn selected_commit(&self) -> Option<&CommitInfo> {
        self.selected
            .checked_sub(self.uncommitted_offset())
            .and_then(|i| self.commits.get(i))
    }

    fn load_next_chunk(&mut self) -> Result<()> {
        if self.all_loaded() {
            return Ok(());
        }
        let start = self.commits.len();
        let end = (start + self.chunk_size).min(self.oids.len());
        let chunk = self.repo.load_commits(&self.oids[start..end])?;
        self.rows.extend(self.engine.process(&chunk));
        self.commits.extend(chunk);
        Ok(())
    }

    fn load_all(&mut self) {
        while !self.all_loaded() {
            if self.load_next_chunk().is_err() {
                break;
            }
        }
    }

    fn ensure_margin(&mut self) {
        while !self.all_loaded() && self.selected + self.load_margin >= self.display_len() {
            if self.load_next_chunk().is_err() {
                break;
            }
        }
    }

    fn sync_list_state(&mut self) {
        if self.display_len() == 0 {
            self.list_state.select(None);
        } else {
            self.list_state.select(Some(self.selected));
        }
    }

    fn move_selection(&mut self, delta: isize) {
        let len = self.display_len();
        if len == 0 {
            return;
        }
        self.selected = (self.selected as isize + delta).clamp(0, len as isize - 1) as usize;
        self.file_selected = 0;
        self.ensure_margin();
        self.sync_list_state();
    }

    fn select_top(&mut self) {
        self.selected = 0;
        self.file_selected = 0;
        self.sync_list_state();
    }

    fn select_bottom(&mut self) {
        self.load_all();
        self.selected = self.display_len().saturating_sub(1);
        self.file_selected = 0;
        self.sync_list_state();
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        self.status.clear();
        // Task 10 turns this into a match once a second mode exists
        // (a 2-arm match with a wildcard trips clippy::single_match).
        if self.mode == Mode::Normal {
            self.handle_normal_key(key);
        }
    }

    fn handle_normal_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::Char('j') | KeyCode::Down => self.move_selection(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_selection(-1),
            KeyCode::Char('g') => self.select_top(),
            KeyCode::Char('G') => self.select_bottom(),
            _ => {}
        }
    }
}
```

- [ ] **Step 4: 寫 src/event.rs**

```rust
//! Terminal input, decoupled from the main loop for testability.
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event, KeyEvent, KeyEventKind};

/// Next key press within `timeout`; None doubles as a redraw tick.
pub fn poll_key(timeout: Duration) -> Result<Option<KeyEvent>> {
    if event::poll(timeout)?
        && let Event::Key(k) = event::read()?
        && k.kind == KeyEventKind::Press
    {
        return Ok(Some(k));
    }
    Ok(None)
}
```

- [ ] **Step 5: ui::render 殼（src/ui/mod.rs 追加）與 lib.rs 掛載**

src/ui/mod.rs 檔頭追加：

```rust
use ratatui::text::Line;
use ratatui::Frame;

use crate::app::App;
```

並追加函式：

```rust
/// Top-level draw. Fleshed out in the graph/detail/diff view tasks.
pub fn render(frame: &mut Frame, app: &mut App) {
    let title = format!(
        " {} — {}/{} commits",
        app.repo_name,
        app.display_len(),
        app.total_len()
    );
    frame.render_widget(Line::from(title), frame.area());
}
```

```rust
// src/lib.rs（全文替換）
pub mod app;
pub mod event;
pub mod git;
pub mod graph;
pub mod ui;
```

- [ ] **Step 6: src/main.rs 全文替換**

```rust
use std::process::ExitCode;
use std::time::Duration;

use clap::Parser;
use gitgraph_tui::{app::App, event, git::GitRepo, ui};
use ratatui::DefaultTerminal;

/// A read-only git history graph viewer for the terminal.
#[derive(Parser)]
#[command(name = "gitgraph-tui", version, about)]
struct Cli {
    /// Path to a git repository (defaults to the current directory)
    path: Option<std::path::PathBuf>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let path = cli.path.unwrap_or_else(|| ".".into());
    // Fail before touching the terminal so the error stays readable.
    let app = match GitRepo::discover(&path).and_then(App::new) {
        Ok(app) => app,
        Err(e) => {
            eprintln!("gitgraph-tui: {e:#}");
            return ExitCode::FAILURE;
        }
    };
    let terminal = ratatui::init(); // installs a panic hook that restores the terminal
    let result = run(terminal, app);
    ratatui::restore();
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("gitgraph-tui: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(mut terminal: DefaultTerminal, mut app: App) -> anyhow::Result<()> {
    while !app.should_quit {
        terminal.draw(|frame| ui::render(frame, &mut app))?;
        if let Some(key) = event::poll_key(Duration::from_millis(250))? {
            app.handle_key(key);
        }
    }
    Ok(())
}
```

- [ ] **Step 7: 跑測試確認通過**

Run: `cargo test --test app_state 2>&1 | tail -3`
Expected: `test result: ok. 8 passed; 0 failed`

- [ ] **Step 8: 手動煙霧測試（真 terminal）**

Run: `cargo run -- . 2>/dev/null` → 應顯示標題列，`q` 可離開，terminal 正常還原。
Run: `cargo run -- /tmp; echo "exit=$?"`
Expected: stderr 一行 `gitgraph-tui: not a git repository (or any parent): /tmp`，`exit=1`。

- [ ] **Step 9: 品質閘門後 commit**

Run: `cargo clippy --all-targets -- -D warnings && cargo fmt --check && cargo test 2>&1 | tail -3`
Expected: 閘門無輸出；全部測試 `ok`。

```bash
git add src/ tests/app_state.rs
git commit -m "feat: app state machine with lazy loading and event loop"
```

---

### Task 8: graph 視圖（graph 線 + refs 標籤 + commit 列 + 標題/說明列）

**Files:**
- Create: `src/ui/graph_view.rs`
- Modify: `src/ui/mod.rs`（`render` 全文替換成三段版）
- Test: `tests/ui_render.rs`

**Interfaces:**
- Consumes: Task 5 的 `GraphRow`/`Cell`、Task 6 的 `lane_color`/`ref_style`/util、Task 7 的 `App`
- Produces:
  - `ui::graph_view::render(frame: &mut Frame, area: Rect, app: &mut App)`
  - `ui::render` 最終版型：上 70% graph、下 30% detail（本 task 先畫空框，Task 9 填）、底部 1 行 help/狀態
  - 測試 helper `render_app(app, w, h) -> Vec<String>`（`tests/ui_render.rs` 內，後續 UI 測試共用）

- [ ] **Step 1: 寫失敗測試 tests/ui_render.rs**

```rust
mod common;

use common::Fixture;
use gitgraph_tui::{app::App, git::GitRepo, ui};
use ratatui::{backend::TestBackend, Terminal};

/// Render into a TestBackend and return the buffer as row strings.
pub fn render_app(app: &mut App, w: u16, h: u16) -> Vec<String> {
    let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
    term.draw(|f| ui::render(f, app)).unwrap();
    let buf = term.backend().buffer().clone();
    let width = buf.area.width as usize;
    buf.content()
        .chunks(width)
        .map(|row| row.iter().map(|c| c.symbol()).collect::<String>())
        .collect()
}

fn merge_fixture() -> Fixture {
    let f = Fixture::new();
    let c1 = f.commit("init", &[("a.txt", "1\n")], &[], &[], 1_000);
    let c2 = f.commit("main work", &[("a.txt", "2\n")], &[], &[c1], 2_000);
    let c3 = f.commit("feature work", &[("b.txt", "3\n")], &[], &[c1], 3_000);
    let c4 = f.commit("merge feature", &[], &[], &[c2, c3], 4_000);
    f.branch("main", c4);
    f.branch("feature", c3);
    f.tag("v1.0", c1);
    f.remote_branch("feature", c3);
    f.set_head("refs/heads/main");
    f
}

fn app_of(f: &Fixture) -> App {
    let repo = GitRepo::discover(f.path()).unwrap();
    App::new_at(repo, 10_000).unwrap()
}

#[test]
fn graph_rows_show_dots_labels_summary_author_and_age() {
    let f = merge_fixture();
    let mut app = app_of(&f);
    let lines = render_app(&mut app, 90, 16);
    let all = lines.join("\n");
    assert!(all.contains('●'), "graph dots render");
    assert!(all.contains('╮') || all.contains('╯'), "merge curves render");
    assert!(all.contains("[main]"), "branch label renders");
    assert!(all.contains("[v1.0]"), "tag label renders");
    assert!(all.contains("[origin/feature]"), "remote branch label renders");
    assert!(all.contains("merge feature"), "summary renders");
    assert!(all.contains("Test Author"), "author renders");
    assert!(all.contains("2h"), "relative age renders (c2/c1 rows are 8_000-9_000s old = 2h)");
}

#[test]
fn title_shows_repo_name_filter_and_counts() {
    let f = merge_fixture();
    let mut app = app_of(&f);
    let lines = render_app(&mut app, 90, 16);
    assert!(lines[0].contains("all branches"));
    assert!(lines[0].contains("4/4"));
}

#[test]
fn uncommitted_row_renders_at_the_top() {
    let f = merge_fixture();
    f.write_file("a.txt", "dirty\n");
    let mut app = app_of(&f);
    let lines = render_app(&mut app, 90, 16);
    assert!(lines.join("\n").contains("Uncommitted changes (1 files)"));
}

#[test]
fn help_line_lists_the_key_bindings() {
    let f = merge_fixture();
    let mut app = app_of(&f);
    let lines = render_app(&mut app, 90, 16);
    let last = lines.last().unwrap();
    assert!(last.contains("q:quit"));
    assert!(last.contains("/:search"));
}

#[test]
fn cjk_summaries_render_without_panicking() {
    let f = Fixture::new();
    let c1 = f.commit("加入中文訊息的提交", &[("a.txt", "1\n")], &[], &[], 1_000);
    f.branch("main", c1);
    f.set_head("refs/heads/main");
    let mut app = app_of(&f);
    // 60 cols: after graph column, [HEAD] [main] labels, author and age,
    // the summary budget still fits 加入中文… (verified: labels eat 14 cols).
    let lines = render_app(&mut app, 60, 10);
    assert!(lines.join("\n").contains("加入中文"));
}
```

- [ ] **Step 2: 跑測試確認失敗**

Run: `cargo test --test ui_render 2>&1 | tail -5`
Expected: 前兩個 assertion 失敗（目前的殼只畫標題一行）。
若 `buf.content()` 不存在改用 `buf.cell((x, y))` 逐格讀取（兩者其一必在 ratatui 0.30 存在；probe 專案可先驗）。

- [ ] **Step 3: 寫 src/ui/graph_view.rs**

```rust
//! The main graph + commit list.
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, List, ListItem},
    Frame,
};
use unicode_width::UnicodeWidthStr;

use crate::app::App;
use crate::ui::util::{pad_to_width, relative_time, truncate_width};
use crate::ui::{lane_color, ref_style};

const AUTHOR_W: usize = 12;
const TIME_W: usize = 5;
/// Graph column cap: beyond 16 lanes the graph is unreadable anyway.
const MAX_GRAPH_W: usize = 32;

pub fn render(frame: &mut Frame, area: Rect, app: &mut App) {
    let graph_w = graph_width(app);
    let text_w = (area.width as usize).saturating_sub(2 + graph_w + 1); // borders + gap
    let items: Vec<ListItem> = (0..app.display_len())
        .map(|i| ListItem::new(row_line(app, i, graph_w, text_w)))
        .collect();
    let filter = match &app.branch_filter {
        Some(r) => r.name.clone(),
        None => "all branches".to_string(),
    };
    let title = format!(
        " {} — {} — {}/{} commits ",
        app.repo_name,
        filter,
        app.display_len(),
        app.total_len()
    );
    let list = List::new(items)
        .block(Block::bordered().title(title))
        .highlight_style(Style::new().add_modifier(Modifier::REVERSED));
    frame.render_stateful_widget(list, area, &mut app.list_state);
}

fn graph_width(app: &App) -> usize {
    app.rows
        .iter()
        .map(|r| r.cells.len())
        .max()
        .unwrap_or(2)
        .clamp(2, MAX_GRAPH_W)
}

fn row_line(app: &App, i: usize, graph_w: usize, text_w: usize) -> Line<'static> {
    let off = app.uncommitted_offset();
    if off == 1 && i == 0 {
        let style = Style::new().fg(Color::Yellow);
        return Line::from(vec![
            Span::styled(pad_to_width("●", graph_w), style),
            Span::raw(" "),
            Span::styled(
                format!("Uncommitted changes ({} files)", app.uncommitted.len()),
                style,
            ),
        ]);
    }
    let idx = i - off;
    let commit = &app.commits[idx];
    let row = &app.rows[idx];
    let mut spans: Vec<Span> = row
        .cells
        .iter()
        .take(graph_w)
        .map(|c| Span::styled(c.glyph.to_string(), Style::new().fg(lane_color(c.color))))
        .collect();
    if row.cells.len() < graph_w {
        spans.push(Span::raw(" ".repeat(graph_w - row.cells.len())));
    }
    spans.push(Span::raw(" "));
    // Ref labels, then the summary in whatever width is left.
    let mut left = text_w.saturating_sub(AUTHOR_W + 1 + TIME_W);
    if let Some(refs) = app.ref_map.get(&commit.id) {
        for r in refs {
            let label = format!("[{}] ", r.name);
            if label.width() > left {
                break;
            }
            left -= label.width();
            spans.push(Span::styled(label, ref_style(r.kind)));
        }
    }
    let summary = truncate_width(&commit.summary, left.saturating_sub(1));
    let pad = left.saturating_sub(summary.width());
    spans.push(Span::raw(summary));
    spans.push(Span::raw(" ".repeat(pad)));
    let dim = Style::new().fg(Color::DarkGray);
    spans.push(Span::styled(
        pad_to_width(&truncate_width(&commit.author_name, AUTHOR_W), AUTHOR_W),
        dim,
    ));
    spans.push(Span::raw(" "));
    spans.push(Span::styled(
        relative_time(commit.timestamp, app.now),
        dim,
    ));
    Line::from(spans)
}
```

- [ ] **Step 4: src/ui/mod.rs 的 render 全文替換成三段版**

mod 區塊補上（檔頭）：

```rust
pub mod graph_view;
```

use 追加：

```rust
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Stylize;
use ratatui::widgets::Block;

use crate::app::Mode;
```

`render` 與新的 `render_help` 全文：

```rust
/// Top-level draw: graph (70%) / detail (30%) / one help line.
pub fn render(frame: &mut Frame, app: &mut App) {
    let [main_area, detail_area, help_area] = Layout::vertical([
        Constraint::Percentage(70),
        Constraint::Percentage(30),
        Constraint::Length(1),
    ])
    .areas(frame.area());
    graph_view::render(frame, main_area, app);
    render_detail_placeholder(frame, detail_area);
    render_help(frame, help_area, app);
}

/// Replaced by detail_view in the next task.
fn render_detail_placeholder(frame: &mut Frame, area: Rect) {
    frame.render_widget(Block::bordered().title(" commit "), area);
}

fn render_help(frame: &mut Frame, area: Rect, app: &App) {
    let text = match app.mode {
        Mode::Search => format!(" /{}▌  enter:confirm  esc:cancel", app.search.input),
        Mode::Diff => " j/k:scroll  g/G:top/bottom  esc:back".to_string(),
        Mode::BranchFilter => " j/k:choose  enter:apply  esc:close".to_string(),
        Mode::Normal if !app.status.is_empty() => format!(" {}", app.status),
        Mode::Normal => {
            " j/k:move g/G:top/bot tab:focus enter:diff /:search n/N:next b:branches r:reload q:quit"
                .to_string()
        }
    };
    frame.render_widget(Line::from(text.dim()), area);
}
```

- [ ] **Step 5: 跑測試確認通過**

Run: `cargo test --test ui_render 2>&1 | tail -3`
Expected: `test result: ok. 5 passed; 0 failed`

- [ ] **Step 6: 手動視覺驗收**

Run: `cargo run -- <某個有 merge 歷史的真實 repo 路徑>`
Expected: 彩色 graph 線與 `[branch]` 標籤如 spec §6 草圖；`j/k` 移動時反白列跟著走；`q` 離開。

- [ ] **Step 7: 品質閘門後 commit**

Run: `cargo clippy --all-targets -- -D warnings && cargo fmt --check && cargo test 2>&1 | tail -3`
Expected: 閘門無輸出；全部測試 `ok`。

```bash
git add src/ui/ tests/ui_render.rs
git commit -m "feat: graph view with lane colors, ref labels, and help bar"
```

---

### Task 9: 詳情面板（commit 資訊 + 檔案列表 + LRU 快取 + Tab 焦點）

**Files:**
- Create: `src/ui/detail_view.rs`
- Modify: `src/ui/mod.rs`（掛 `pub mod detail_view;`，把 `render_detail_placeholder` 換成 `detail_view::render`，並刪掉 placeholder 函式）
- Modify: `src/app.rs`（加 `current_files`、Tab/檔案導覽鍵）
- Test: `tests/app_state.rs`、`tests/ui_render.rs`（各追加）

**Interfaces:**
- Consumes: Task 4 的 `commit_files`/`worktree_status`、Task 7 的 `App`
- Produces:
  - `App::current_files(&mut self) -> Vec<FileChange>` — 選中列的變更檔案：uncommitted 列回 `self.uncommitted`；commit 列走 `detail_cache`（LRU 50）→ miss 才呼叫 `repo.commit_files`
  - Normal 模式新鍵位：`Tab` 切換 `Focus::Commits ↔ Focus::Files`；焦點在 Files 時 `j/k/↑/↓` 改移動 `file_selected`
  - `ui::detail_view::render(frame, area, app)`

- [ ] **Step 1: 追加失敗測試**

tests/app_state.rs 追加：

```rust
use gitgraph_tui::app::Focus;
use gitgraph_tui::git::types::ChangeKind;

#[test]
fn current_files_returns_commit_changes_and_caches_them() {
    let (_f, mut app) = linear_app(3, 300);
    let files = app.current_files();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].path, "a.txt");
    assert_eq!(files[0].kind, ChangeKind::Modified); // commit 2 rewrites a.txt
    let id = app.selected_commit().unwrap().id.clone();
    assert!(app.detail_cache.contains(&id), "result must be cached");
}

#[test]
fn current_files_for_the_uncommitted_row_lists_worktree_changes() {
    let f = Fixture::new();
    let c1 = f.commit("base", &[("a.txt", "1\n")], &[], &[], 1_000);
    f.branch("main", c1);
    f.set_head("refs/heads/main");
    f.write_file("b.txt", "new\n");
    let repo = GitRepo::discover(f.path()).unwrap();
    let mut app = App::new_at(repo, 10_000).unwrap();
    let files = app.current_files();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].path, "b.txt");
}

#[test]
fn tab_toggles_focus_and_j_k_move_the_file_cursor() {
    let f = Fixture::new();
    let c1 = f.commit(
        "two files",
        &[("a.txt", "1\n"), ("b.txt", "2\n")],
        &[],
        &[],
        1_000,
    );
    f.branch("main", c1);
    f.set_head("refs/heads/main");
    let repo = GitRepo::discover(f.path()).unwrap();
    let mut app = App::new_at(repo, 10_000).unwrap();
    assert_eq!(app.focus, Focus::Commits);
    app.handle_key(key(KeyCode::Tab));
    assert_eq!(app.focus, Focus::Files);
    app.handle_key(ch('j'));
    assert_eq!(app.file_selected, 1);
    app.handle_key(ch('j'));
    assert_eq!(app.file_selected, 1, "clamps at the last file");
    app.handle_key(ch('k'));
    assert_eq!(app.file_selected, 0);
    app.handle_key(key(KeyCode::Tab));
    assert_eq!(app.focus, Focus::Commits);
}

#[test]
fn moving_the_commit_cursor_resets_file_focus_state() {
    let (_f, mut app) = linear_app(3, 300);
    app.handle_key(key(KeyCode::Tab));
    app.handle_key(ch('j')); // file cursor (only 1 file → stays 0)
    app.handle_key(key(KeyCode::Tab));
    app.handle_key(ch('j')); // commit cursor
    assert_eq!(app.file_selected, 0);
}
```

tests/ui_render.rs 追加：

```rust
#[test]
fn detail_panel_shows_commit_meta_and_file_list() {
    let f = merge_fixture();
    let mut app = app_of(&f);
    // select "main work" (row with a real file change)
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('j'),
        crossterm::event::KeyModifiers::NONE,
    ));
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('j'),
        crossterm::event::KeyModifiers::NONE,
    ));
    let lines = render_app(&mut app, 90, 20);
    let all = lines.join("\n");
    assert!(all.contains("Test Author <test@example.com>"));
    assert!(all.contains("1970-01-01 00:"), "absolute date renders");
    assert!(all.contains("M a.txt"), "file list renders with change kind");
    assert!(all.contains("+1"), "addition count renders");
}

#[test]
fn detail_panel_shows_the_full_message_body() {
    let f = Fixture::new();
    let c1 = f.commit(
        "subject line\n\nbody first line\nbody second line",
        &[("a.txt", "1\n")],
        &[],
        &[],
        1_000,
    );
    f.branch("main", c1);
    f.set_head("refs/heads/main");
    let mut app = app_of(&f);
    let lines = render_app(&mut app, 90, 24);
    let all = lines.join("\n");
    assert!(all.contains("body first line"));
    assert!(all.contains("body second line"));
}

#[test]
fn detail_panel_for_uncommitted_row() {
    let f = merge_fixture();
    f.write_file("z.txt", "wip\n");
    let mut app = app_of(&f);
    let lines = render_app(&mut app, 90, 20);
    let all = lines.join("\n");
    assert!(all.contains("Uncommitted changes"));
    assert!(all.contains("A z.txt"));
}
```

- [ ] **Step 2: 跑測試確認失敗**

Run: `cargo test --test app_state --test ui_render 2>&1 | tail -5`
Expected: 編譯錯誤 `no method named current_files`。

- [ ] **Step 3: src/app.rs 追加方法與鍵位**

`impl App` 追加：

```rust
    /// Changed files for the selected row (uncommitted row or commit),
    /// LRU-cached per commit.
    pub fn current_files(&mut self) -> Vec<FileChange> {
        let Some(commit) = self.selected_commit() else {
            return self.uncommitted.clone();
        };
        let id = commit.id.clone();
        if let Some(files) = self.detail_cache.get(&id) {
            return files.clone();
        }
        let files = self.repo.commit_files(&id).unwrap_or_default();
        self.detail_cache.put(id, files.clone());
        files
    }

    fn toggle_focus(&mut self) {
        self.focus = match self.focus {
            Focus::Commits => Focus::Files,
            Focus::Files => Focus::Commits,
        };
    }

    fn move_file_selection(&mut self, delta: isize) {
        let len = self.current_files().len();
        if len == 0 {
            return;
        }
        self.file_selected =
            (self.file_selected as isize + delta).clamp(0, len as isize - 1) as usize;
    }
```

`handle_normal_key` 全文替換（j/k 依焦點分流、加 Tab）：

```rust
    fn handle_normal_key(&mut self, key: KeyEvent) {
        match (self.focus, key.code) {
            (_, KeyCode::Char('q')) | (_, KeyCode::Esc) => self.should_quit = true,
            (_, KeyCode::Tab) => self.toggle_focus(),
            (Focus::Commits, KeyCode::Char('j')) | (Focus::Commits, KeyCode::Down) => {
                self.move_selection(1)
            }
            (Focus::Commits, KeyCode::Char('k')) | (Focus::Commits, KeyCode::Up) => {
                self.move_selection(-1)
            }
            (Focus::Files, KeyCode::Char('j')) | (Focus::Files, KeyCode::Down) => {
                self.move_file_selection(1)
            }
            (Focus::Files, KeyCode::Char('k')) | (Focus::Files, KeyCode::Up) => {
                self.move_file_selection(-1)
            }
            (_, KeyCode::Char('g')) => self.select_top(),
            (_, KeyCode::Char('G')) => self.select_bottom(),
            _ => {}
        }
    }
```

（`move_selection` 已會把 `file_selected` 歸零，維持「換 commit 就從第一個檔案看起」。）

- [ ] **Step 4: 寫 src/ui/detail_view.rs**

```rust
//! Bottom panel: metadata + changed-file list for the selected row.
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, List, ListItem, ListState, Paragraph},
    Frame,
};

use crate::app::{App, Focus};
use crate::git::types::ChangeKind;
use crate::ui::util::absolute_time;

pub fn render(frame: &mut Frame, area: Rect, app: &mut App) {
    let files = app.current_files();
    let (title, meta) = match app.selected_commit() {
        Some(c) => {
            let mut meta = vec![
                Line::from(Span::styled(
                    c.summary.clone(),
                    Style::new().add_modifier(Modifier::BOLD),
                )),
                Line::from(format!("{} <{}>", c.author_name, c.author_email)),
                Line::from(format!(
                    "{}  ·  {} parent(s)",
                    absolute_time(c.timestamp),
                    c.parents.len()
                )),
            ];
            // Full message body (spec: 完整訊息), capped so the file list
            // below keeps some room.
            let body = c.message.lines().skip(1).skip_while(|l| l.is_empty());
            for line in body.take(4) {
                meta.push(Line::from(line.to_string()));
            }
            (format!(" commit {} ", c.short_id), meta)
        }
        None if !files.is_empty() => (
            " Uncommitted changes ".to_string(),
            vec![Line::from(format!("{} files changed", files.len()))],
        ),
        None => (" commit ".to_string(), vec![Line::from("No commits yet")]),
    };
    let block = Block::bordered().title(title);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let meta_h = (meta.len() as u16).min(inner.height);
    let meta_area = Rect { height: meta_h, ..inner };
    frame.render_widget(Paragraph::new(meta), meta_area);
    let files_area = Rect {
        y: inner.y + meta_h,
        height: inner.height.saturating_sub(meta_h),
        ..inner
    };
    let items: Vec<ListItem> = files.iter().map(|f| ListItem::new(file_line(f))).collect();
    let focused = app.focus == Focus::Files;
    let list = List::new(items).highlight_style(if focused {
        Style::new().add_modifier(Modifier::REVERSED)
    } else {
        Style::new()
    });
    let mut state = ListState::default().with_selected(Some(app.file_selected));
    frame.render_stateful_widget(list, files_area, &mut state);
}

fn file_line(f: &crate::git::types::FileChange) -> Line<'static> {
    let (letter, color) = match &f.kind {
        ChangeKind::Added => ("A", Color::Green),
        ChangeKind::Modified => ("M", Color::Yellow),
        ChangeKind::Deleted => ("D", Color::Red),
        ChangeKind::Renamed { .. } => ("R", Color::Cyan),
    };
    let mut spans = vec![
        Span::styled(format!(" {letter} "), Style::new().fg(color)),
        Span::raw(f.path.clone()),
    ];
    if let ChangeKind::Renamed { from } = &f.kind {
        spans.push(Span::styled(
            format!(" (from {from})"),
            Style::new().fg(Color::DarkGray),
        ));
    }
    if f.is_binary {
        spans.push(Span::styled(" (binary)", Style::new().fg(Color::Magenta)));
    } else {
        spans.push(Span::styled(
            format!("  +{}", f.additions),
            Style::new().fg(Color::Green),
        ));
        spans.push(Span::styled(
            format!(" -{}", f.deletions),
            Style::new().fg(Color::Red),
        ));
    }
    Line::from(spans)
}
```

- [ ] **Step 5: src/ui/mod.rs 換掉 placeholder**

mod 區塊加 `pub mod detail_view;`；`render` 內 `render_detail_placeholder(frame, detail_area);` 改為 `detail_view::render(frame, detail_area, app);`；刪除 `render_detail_placeholder` 函式與其多餘的 use（`Block`、`Rect` 若已無他處使用一併移除，否則 clippy -D warnings 會擋）。

- [ ] **Step 6: 跑測試確認通過**

Run: `cargo test 2>&1 | tail -3`
Expected: 全部 `ok`（app_state 12、ui_render 8、lib 13、git_repo 10、git_diff 9）。

- [ ] **Step 7: 品質閘門後 commit**

Run: `cargo clippy --all-targets -- -D warnings && cargo fmt --check`
Expected: 無輸出。

```bash
git add src/app.rs src/ui/ tests/
git commit -m "feat: detail panel with cached file lists and focus toggle"
```

---

### Task 10: 全螢幕 diff 檢視

**Files:**
- Create: `src/ui/diff_view.rs`
- Modify: `src/ui/mod.rs`（掛模組；Diff 模式時整個畫面改畫 diff）
- Modify: `src/app.rs`（`open_diff`、Diff 模式鍵位）
- Test: `tests/app_state.rs`、`tests/ui_render.rs`（各追加）

**Interfaces:**
- Consumes: Task 4 的 `commit_file_diff`/`worktree_file_diff`、Task 9 的 `current_files`/`Focus`
- Produces:
  - `App::open_diff(&mut self)` — 焦點在 Files 且該列有檔案時，載入選中檔案的 diff、進入 `Mode::Diff`
  - Diff 模式鍵位：`j/k/↑/↓` 逐行捲動、`g/G` 頂/底、`Esc`/`q` 返回 Normal
  - `ui::diff_view::render(frame, area, app)`（全螢幕覆蓋，含底部 help 列）

- [ ] **Step 1: 追加失敗測試**

tests/app_state.rs 追加：

```rust
use gitgraph_tui::app::Mode;

#[test]
fn enter_on_a_file_opens_the_diff_and_esc_closes_it() {
    let f = Fixture::new();
    let c1 = f.commit("base", &[("a.txt", "one\n")], &[], &[], 1_000);
    let c2 = f.commit("edit", &[("a.txt", "one\ntwo\n")], &[], &[c1], 2_000);
    f.branch("main", c2);
    f.set_head("refs/heads/main");
    let repo = GitRepo::discover(f.path()).unwrap();
    let mut app = App::new_at(repo, 10_000).unwrap();
    app.handle_key(key(KeyCode::Tab)); // focus files
    app.handle_key(key(KeyCode::Enter));
    assert_eq!(app.mode, Mode::Diff);
    let diff = app.diff.as_ref().unwrap();
    assert_eq!(diff.title, "a.txt");
    assert!(diff.lines.iter().any(|l| l.origin == '+' && l.content == "two"));
    app.handle_key(key(KeyCode::Esc));
    assert_eq!(app.mode, Mode::Normal);
    assert!(app.diff.is_none());
}

#[test]
fn enter_without_file_focus_does_nothing() {
    let (_f, mut app) = linear_app(3, 300);
    app.handle_key(key(KeyCode::Enter));
    assert_eq!(app.mode, Mode::Normal);
}

#[test]
fn diff_scrolling_clamps() {
    let f = Fixture::new();
    let many: String = (0..50).map(|i| format!("line{i}\n")).collect();
    let c1 = f.commit("big", &[("a.txt", &many)], &[], &[], 1_000);
    f.branch("main", c1);
    f.set_head("refs/heads/main");
    let repo = GitRepo::discover(f.path()).unwrap();
    let mut app = App::new_at(repo, 10_000).unwrap();
    app.handle_key(key(KeyCode::Tab));
    app.handle_key(key(KeyCode::Enter));
    app.handle_key(ch('k'));
    assert_eq!(app.diff.as_ref().unwrap().scroll, 0, "clamps at top");
    app.handle_key(ch('G'));
    let bottom = app.diff.as_ref().unwrap().scroll;
    assert!(bottom > 0);
    app.handle_key(ch('j'));
    assert_eq!(app.diff.as_ref().unwrap().scroll, bottom, "clamps at bottom");
    app.handle_key(ch('g'));
    assert_eq!(app.diff.as_ref().unwrap().scroll, 0);
}

#[test]
fn diff_works_for_uncommitted_files() {
    let f = Fixture::new();
    let c1 = f.commit("base", &[("a.txt", "old\n")], &[], &[], 1_000);
    f.branch("main", c1);
    f.set_head("refs/heads/main");
    f.write_file("a.txt", "new\n");
    let repo = GitRepo::discover(f.path()).unwrap();
    let mut app = App::new_at(repo, 10_000).unwrap();
    app.handle_key(key(KeyCode::Tab));
    app.handle_key(key(KeyCode::Enter));
    assert_eq!(app.mode, Mode::Diff);
    let diff = app.diff.as_ref().unwrap();
    assert!(diff.lines.iter().any(|l| l.origin == '+' && l.content == "new"));
}
```

tests/ui_render.rs 追加：

```rust
#[test]
fn diff_view_renders_colored_lines_full_screen() {
    let f = Fixture::new();
    let c1 = f.commit("base", &[("a.txt", "one\n")], &[], &[], 1_000);
    let c2 = f.commit("edit", &[("a.txt", "one\ntwo\n")], &[], &[c1], 2_000);
    f.branch("main", c2);
    f.set_head("refs/heads/main");
    let mut app = app_of(&f);
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Tab,
        crossterm::event::KeyModifiers::NONE,
    ));
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Enter,
        crossterm::event::KeyModifiers::NONE,
    ));
    let lines = render_app(&mut app, 60, 12);
    let all = lines.join("\n");
    assert!(all.contains("a.txt"), "title shows the file path");
    assert!(all.contains("@@"), "hunk header renders");
    assert!(all.contains("+two"), "addition renders");
    assert!(!all.contains("all branches"), "graph view is fully covered");
}
```

- [ ] **Step 2: 跑測試確認失敗**

Run: `cargo test --test app_state 2>&1 | tail -5`
Expected: 三個 diff 測試失敗——`enter_on_a_file_opens_the_diff_and_esc_closes_it` 斷言失敗；`diff_scrolling_clamps` 與 `diff_works_for_uncommitted_files` 在 `app.diff.as_ref().unwrap()` panic（diff 還是 None）。`enter_without_file_focus_does_nothing` 本來就會過。

- [ ] **Step 3: src/app.rs 追加**

`handle_key` 內的 `if self.mode == Mode::Normal { ... }` 換成 match：

```rust
        match self.mode {
            Mode::Normal => self.handle_normal_key(key),
            Mode::Diff => self.handle_diff_key(key),
            _ => {} // Search/BranchFilter arrive in later tasks
        }
```

`handle_normal_key` 的 match 加一臂（放在 Tab 臂之後）：

```rust
            (Focus::Files, KeyCode::Enter) => self.open_diff(),
```

`impl App` 追加：

```rust
    /// Open the full-screen diff for the file under the file cursor.
    fn open_diff(&mut self) {
        let files = self.current_files();
        let Some(file) = files.get(self.file_selected) else {
            return;
        };
        let lines = match self.selected_commit() {
            Some(c) => {
                let id = c.id.clone();
                self.repo.commit_file_diff(&id, &file.path)
            }
            None => self.repo.worktree_file_diff(&file.path),
        };
        match lines {
            Ok(lines) => {
                self.diff = Some(DiffState { title: file.path.clone(), lines, scroll: 0 });
                self.mode = Mode::Diff;
            }
            Err(e) => self.status = format!("diff failed: {e:#}"),
        }
    }

    fn handle_diff_key(&mut self, key: KeyEvent) {
        let Some(diff) = self.diff.as_mut() else {
            self.mode = Mode::Normal;
            return;
        };
        let max = diff.lines.len().saturating_sub(1);
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => {
                self.diff = None;
                self.mode = Mode::Normal;
            }
            KeyCode::Char('j') | KeyCode::Down => diff.scroll = (diff.scroll + 1).min(max),
            KeyCode::Char('k') | KeyCode::Up => diff.scroll = diff.scroll.saturating_sub(1),
            KeyCode::Char('g') => diff.scroll = 0,
            KeyCode::Char('G') => diff.scroll = max,
            _ => {}
        }
    }
```

- [ ] **Step 4: 寫 src/ui/diff_view.rs 並接上 render**

```rust
//! Full-screen single-file diff.
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph},
    Frame,
};

use crate::app::App;

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let Some(diff) = app.diff.as_ref() else { return };
    let lines: Vec<Line> = diff
        .lines
        .iter()
        .map(|l| {
            let style = match l.origin {
                '+' => Style::new().fg(Color::Green),
                '-' => Style::new().fg(Color::Red),
                '@' => Style::new().fg(Color::Cyan),
                'B' => Style::new().fg(Color::Magenta).add_modifier(Modifier::ITALIC),
                _ => Style::new(),
            };
            let prefix = if matches!(l.origin, '@' | 'B') {
                String::new()
            } else {
                l.origin.to_string()
            };
            Line::from(Span::styled(format!("{prefix}{}", l.content), style))
        })
        .collect();
    let para = Paragraph::new(lines)
        .block(Block::bordered().title(format!(" {} ", diff.title)))
        .scroll((diff.scroll as u16, 0));
    frame.render_widget(para, area);
}
```

src/ui/mod.rs：mod 區塊加 `pub mod diff_view;`；`render` 開頭加 Diff 模式攔截：

```rust
pub fn render(frame: &mut Frame, app: &mut App) {
    let [main_area, help_area] =
        Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(frame.area());
    if app.mode == Mode::Diff {
        diff_view::render(frame, main_area, app);
        render_help(frame, help_area, app);
        return;
    }
    let [graph_area, detail_area] =
        Layout::vertical([Constraint::Percentage(70), Constraint::Percentage(30)])
            .areas(main_area);
    graph_view::render(frame, graph_area, app);
    detail_view::render(frame, detail_area, app);
    render_help(frame, help_area, app);
}
```

- [ ] **Step 5: 跑測試確認通過**

Run: `cargo test 2>&1 | tail -3`
Expected: 全部 `ok`。

- [ ] **Step 6: 品質閘門後 commit**

Run: `cargo clippy --all-targets -- -D warnings && cargo fmt --check`
Expected: 無輸出。

```bash
git add src/app.rs src/ui/ tests/
git commit -m "feat: full-screen file diff view"
```

---

### Task 11: 增量搜尋（`/`、`n/N`、自動載入續搜）

**Files:**
- Modify: `src/app.rs`（Search 模式 + 搜尋方法）
- Modify: `src/ui/graph_view.rs`（符合列的 summary 加底線）
- Test: `tests/app_state.rs`、`tests/ui_render.rs`（各追加）

**Interfaces:**
- Consumes: Task 7 的 `App`/`SearchState`、Task 8 的 `row_line`
- Produces:
  - Normal 鍵位：`/` 進 Search 模式、`n`/`N` 下一個/上一個符合（`query` 為空時提示）
  - Search 模式：打字即時比對已載入 commits 並跳到最近符合；`Backspace` 刪字；`Enter` 確認（回 Normal、status 顯示 N matches）；`Esc` 取消
  - 比對欄位：summary、message、作者名（不分大小寫）、commit id 前綴
  - `n` 到底時自動載入下一塊繼續搜（spec §7），全載完才回捲（wrap）

- [ ] **Step 1: 追加失敗測試到 tests/app_state.rs**

```rust
#[test]
fn slash_enters_search_and_typing_jumps_to_the_nearest_match() {
    let (_f, mut app) = linear_app(10, 300);
    app.handle_key(ch('/'));
    assert_eq!(app.mode, Mode::Search);
    for c in "commit 3".chars() {
        app.handle_key(ch(c));
    }
    // topo order: newest first → "commit 3" is display row 6
    assert_eq!(app.selected, 6);
    app.handle_key(key(KeyCode::Enter));
    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(app.search.query, "commit 3");
    assert!(app.status.contains("1 match"));
}

#[test]
fn search_is_case_insensitive_and_finds_hash_prefixes() {
    let (_f, mut app) = linear_app(3, 300);
    app.handle_key(ch('/'));
    for c in "COMMIT 1".chars() {
        app.handle_key(ch(c));
    }
    app.handle_key(key(KeyCode::Enter));
    assert_eq!(app.search.matches.len(), 1);

    let hash7 = app.commits[0].short_id.clone();
    app.handle_key(ch('/'));
    for c in hash7.chars() {
        app.handle_key(ch(c));
    }
    app.handle_key(key(KeyCode::Enter));
    assert_eq!(app.selected, 0);
    assert_eq!(app.search.matches.len(), 1);
}

#[test]
fn esc_cancels_the_search_input() {
    let (_f, mut app) = linear_app(3, 300);
    app.handle_key(ch('/'));
    app.handle_key(ch('x'));
    app.handle_key(key(KeyCode::Esc));
    assert_eq!(app.mode, Mode::Normal);
    assert!(app.search.input.is_empty());
    assert!(app.search.matches.is_empty());
}

#[test]
fn n_wraps_around_when_everything_is_loaded() {
    let (_f, mut app) = linear_app(5, 300);
    app.handle_key(ch('/'));
    for c in "commit".chars() {
        app.handle_key(ch(c));
    }
    app.handle_key(key(KeyCode::Enter)); // matches all 5 rows
    assert_eq!(app.selected, 0);
    app.handle_key(ch('n'));
    assert_eq!(app.selected, 1);
    app.handle_key(ch('N'));
    assert_eq!(app.selected, 0);
    for _ in 0..4 {
        app.handle_key(ch('n'));
    }
    assert_eq!(app.selected, 4);
    app.handle_key(ch('n')); // wrap
    assert_eq!(app.selected, 0);
}

#[test]
fn n_auto_loads_further_chunks_until_it_finds_a_match() {
    let (_f, mut app) = linear_app(10, 3);
    app.reload().unwrap();
    assert_eq!(app.commits.len(), 3); // only "commit 9..7" loaded
    app.handle_key(ch('/'));
    for c in "commit 0".chars() {
        app.handle_key(ch(c));
    }
    app.handle_key(key(KeyCode::Enter));
    assert!(app.search.matches.is_empty(), "oldest commit not loaded yet");
    app.handle_key(ch('n'));
    assert_eq!(app.selected, 9, "auto-loaded chunks until the match appeared");
    assert!(app.all_loaded());
}

#[test]
fn n_without_a_confirmed_query_sets_a_hint_status() {
    let (_f, mut app) = linear_app(3, 300);
    app.handle_key(ch('n'));
    assert!(app.status.contains('/'));
}
```

- [ ] **Step 2: 跑測試確認失敗**

Run: `cargo test --test app_state 2>&1 | tail -5`
Expected: 新測試失敗（`/` 目前無作用，mode 仍 Normal）。

- [ ] **Step 3: src/app.rs 實作**

`handle_key` 的 match 補上：

```rust
            Mode::Search => self.handle_search_key(key),
```

`handle_normal_key` 加三臂（放在 `g`/`G` 臂前）：

```rust
            (_, KeyCode::Char('/')) => {
                self.search.input.clear();
                self.mode = Mode::Search;
            }
            (_, KeyCode::Char('n')) => self.next_match(1),
            (_, KeyCode::Char('N')) => self.next_match(-1),
```

`impl App` 追加：

```rust
    fn matches_query(commit: &CommitInfo, q: &str) -> bool {
        let q = q.to_lowercase();
        commit.summary.to_lowercase().contains(&q)
            || commit.message.to_lowercase().contains(&q)
            || commit.author_name.to_lowercase().contains(&q)
            || commit.id.starts_with(&q)
    }

    /// Rebuild the match list for `q` over the loaded commits.
    fn recompute_matches(&mut self, q: &str) {
        let off = self.uncommitted_offset();
        self.search.matches = self
            .commits
            .iter()
            .enumerate()
            .filter(|(_, c)| Self::matches_query(c, q))
            .map(|(i, _)| i + off)
            .collect();
    }

    fn handle_search_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.search.input.clear();
                self.search.matches.clear();
                self.mode = Mode::Normal;
            }
            KeyCode::Enter => {
                self.search.query = self.search.input.clone();
                let q = self.search.query.clone();
                self.recompute_matches(&q);
                let n = self.search.matches.len();
                self.status = format!("{n} match{}", if n == 1 { "" } else { "es" });
                self.mode = Mode::Normal;
            }
            KeyCode::Backspace => {
                self.search.input.pop();
                self.live_search();
            }
            KeyCode::Char(c) => {
                self.search.input.push(c);
                self.live_search();
            }
            _ => {}
        }
    }

    /// Incremental search while typing: jump to the nearest match at or
    /// after the cursor (wrapping to the first).
    fn live_search(&mut self) {
        let q = self.search.input.clone();
        if q.is_empty() {
            self.search.matches.clear();
            return;
        }
        self.recompute_matches(&q);
        let target = self
            .search
            .matches
            .iter()
            .copied()
            .find(|&i| i >= self.selected)
            .or_else(|| self.search.matches.first().copied());
        if let Some(i) = target {
            self.jump_to(i);
        }
    }

    fn jump_to(&mut self, i: usize) {
        self.selected = i.min(self.display_len().saturating_sub(1));
        self.file_selected = 0;
        self.ensure_margin();
        self.sync_list_state();
    }

    /// n/N. Forward search loads further chunks until a match appears
    /// (spec: auto-continue); wraps only once everything is loaded.
    fn next_match(&mut self, dir: isize) {
        if self.search.query.is_empty() {
            self.status = "no search query — press / first".to_string();
            return;
        }
        loop {
            let found = if dir > 0 {
                self.search.matches.iter().copied().find(|&i| i > self.selected)
            } else {
                self.search.matches.iter().rev().copied().find(|&i| i < self.selected)
            };
            if let Some(i) = found {
                self.jump_to(i);
                return;
            }
            if dir > 0 && !self.all_loaded() {
                let before = self.commits.len();
                if self.load_next_chunk().is_err() {
                    return;
                }
                let off = self.uncommitted_offset();
                let q = self.search.query.clone();
                let fresh: Vec<usize> = self.commits[before..]
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| Self::matches_query(c, &q))
                    .map(|(i, _)| before + i + off)
                    .collect();
                self.search.matches.extend(fresh);
                continue; // re-check with the extended match list
            }
            // Fully loaded: wrap around.
            let wrapped = if dir > 0 {
                self.search.matches.first()
            } else {
                self.search.matches.last()
            };
            match wrapped {
                Some(&i) => {
                    self.jump_to(i);
                    self.status = "search wrapped".to_string();
                }
                None => {
                    self.status = format!("no matches for '{}'", self.search.query);
                }
            }
            return;
        }
    }
```

- [ ] **Step 4: graph_view 標出符合列**

`src/ui/graph_view.rs` 的 `row_line` 中，替換 summary 那兩行：

```rust
    let summary = truncate_width(&commit.summary, left.saturating_sub(1));
    let pad = left.saturating_sub(summary.width());
    let sum_style = if app.search.matches.contains(&i) {
        Style::new().add_modifier(Modifier::UNDERLINED)
    } else {
        Style::new()
    };
    spans.push(Span::styled(summary, sum_style));
```

（原本的 `spans.push(Span::raw(summary));` 刪除；`pad` 行保留。）

- [ ] **Step 5: 追加 UI 測試到 tests/ui_render.rs**

```rust
#[test]
fn search_mode_shows_the_live_input_in_the_help_line() {
    let f = merge_fixture();
    let mut app = app_of(&f);
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('/'),
        crossterm::event::KeyModifiers::NONE,
    ));
    for c in "feat".chars() {
        app.handle_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char(c),
            crossterm::event::KeyModifiers::NONE,
        ));
    }
    let lines = render_app(&mut app, 80, 12);
    assert!(lines.last().unwrap().contains("/feat"));
}
```

- [ ] **Step 6: 跑測試確認通過**

Run: `cargo test 2>&1 | tail -3`
Expected: 全部 `ok`。

- [ ] **Step 7: 品質閘門後 commit**

Run: `cargo clippy --all-targets -- -D warnings && cargo fmt --check`
Expected: 無輸出。

```bash
git add src/app.rs src/ui/graph_view.rs tests/
git commit -m "feat: incremental search with n/N and auto-loading"
```

---

### Task 12: branch 篩選彈窗與重新載入

**Files:**
- Create: `src/ui/popup.rs`
- Modify: `src/ui/mod.rs`（掛模組；BranchFilter 模式時疊加彈窗）
- Modify: `src/app.rs`（`b`/`r` 鍵、彈窗鍵位）
- Test: `tests/app_state.rs`、`tests/ui_render.rs`（各追加）

**Interfaces:**
- Consumes: Task 7 的 `App`/`reload()`、`RefInfo`/`RefKind`
- Produces:
  - Normal 鍵位：`b` 開彈窗、`r` 重新載入（status 顯示 "reloaded"）
  - BranchFilter 模式：`j/k/↑/↓` 選、`Enter` 套用（`branch_filter` 設定後 `reload()`、跳回頂端）、`Esc`/`q` 關閉
  - `filter_choices[0]` 恆為 `None`（"All branches"）；其餘為本地+遠端 branch
  - `ui::popup::render(frame, app)` — 置中彈窗

- [ ] **Step 1: 追加失敗測試到 tests/app_state.rs**

```rust
/// main: c1→c2→c4(HEAD) · feature: c1→c3 — reuses the Task 3 shape.
fn branchy_app() -> (Fixture, App) {
    let f = Fixture::new();
    let c1 = f.commit("c1 init", &[("a.txt", "1")], &[], &[], 1_000);
    let c2 = f.commit("c2 main work", &[("a.txt", "2")], &[], &[c1], 2_000);
    let c3 = f.commit("c3 feature work", &[("b.txt", "3")], &[], &[c1], 3_000);
    let c4 = f.commit("c4 more main", &[("a.txt", "4")], &[], &[c2], 4_000);
    f.branch("main", c4);
    f.branch("feature", c3);
    f.remote_branch("main", c4);
    f.set_head("refs/heads/main");
    let repo = GitRepo::discover(f.path()).unwrap();
    let app = App::new_at(repo, 10_000).unwrap();
    (f, app)
}

#[test]
fn b_opens_the_filter_popup_with_all_branches_first() {
    let (_f, mut app) = branchy_app();
    app.handle_key(ch('b'));
    assert_eq!(app.mode, Mode::BranchFilter);
    assert!(app.filter_choices[0].is_none(), "row 0 is All branches");
    let names: Vec<String> = app
        .filter_choices
        .iter()
        .flatten()
        .map(|r| r.name.clone())
        .collect();
    assert!(names.contains(&"main".to_string()));
    assert!(names.contains(&"feature".to_string()));
    assert!(names.contains(&"origin/main".to_string()), "remote branches are offered too");
}

#[test]
fn applying_a_branch_filter_narrows_the_walk() {
    let (_f, mut app) = branchy_app();
    assert_eq!(app.total_len(), 4);
    app.handle_key(ch('b'));
    let feature_pos = app
        .filter_choices
        .iter()
        .position(|c| c.as_ref().is_some_and(|r| r.name == "feature"))
        .unwrap();
    for _ in 0..feature_pos {
        app.handle_key(ch('j'));
    }
    app.handle_key(key(KeyCode::Enter));
    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(app.total_len(), 2); // c3, c1
    assert!(app.commits.iter().all(|c| c.summary != "c2 main work"));
    assert_eq!(app.selected, 0);
}

#[test]
fn reopening_the_popup_highlights_the_active_filter() {
    let (_f, mut app) = branchy_app();
    app.handle_key(ch('b'));
    app.handle_key(ch('j'));
    app.handle_key(key(KeyCode::Enter));
    let picked = app.branch_filter.clone().unwrap();
    app.handle_key(ch('b'));
    let highlighted = app.filter_choices[app.filter_selected].clone().unwrap();
    assert_eq!(highlighted.refname, picked.refname);
}

#[test]
fn esc_closes_the_popup_without_changing_the_filter() {
    let (_f, mut app) = branchy_app();
    app.handle_key(ch('b'));
    app.handle_key(ch('j'));
    app.handle_key(key(KeyCode::Esc));
    assert_eq!(app.mode, Mode::Normal);
    assert!(app.branch_filter.is_none());
    assert_eq!(app.total_len(), 4);
}

#[test]
fn r_reloads_and_picks_up_new_commits() {
    let (f, mut app) = branchy_app();
    assert_eq!(app.total_len(), 4);
    let head = git2::Oid::from_str(&app.commits[0].id).unwrap();
    let c5 = f.commit("c5 fresh", &[("a.txt", "5")], &[], &[head], 5_000);
    f.branch("main", c5);
    app.handle_key(ch('r'));
    assert_eq!(app.total_len(), 5);
    assert!(app.status.contains("reloaded"));
}
```

- [ ] **Step 2: 跑測試確認失敗**

Run: `cargo test --test app_state 2>&1 | tail -5`
Expected: 新測試失敗（`b` 目前無作用）。

- [ ] **Step 3: src/app.rs 實作**

use 追加：`use crate::git::types::RefKind;`（併入既有的 types use）。

`handle_key` 的 match 換成完整版（移除萬用臂）：

```rust
        match self.mode {
            Mode::Normal => self.handle_normal_key(key),
            Mode::Search => self.handle_search_key(key),
            Mode::Diff => self.handle_diff_key(key),
            Mode::BranchFilter => self.handle_filter_key(key),
        }
```

`handle_normal_key` 加兩臂：

```rust
            (_, KeyCode::Char('b')) => self.open_branch_filter(),
            (_, KeyCode::Char('r')) => {
                match self.reload() {
                    Ok(()) => self.status = "reloaded".to_string(),
                    Err(e) => self.status = format!("reload failed: {e:#}"),
                }
            }
```

`impl App` 追加：

```rust
    fn open_branch_filter(&mut self) {
        let mut choices: Vec<Option<RefInfo>> = vec![None];
        choices.extend(
            self.refs
                .iter()
                .filter(|r| matches!(r.kind, RefKind::LocalBranch | RefKind::RemoteBranch))
                .cloned()
                .map(Some),
        );
        self.filter_choices = choices;
        self.filter_selected = self
            .filter_choices
            .iter()
            .position(|c| match (c, &self.branch_filter) {
                (None, None) => true,
                (Some(a), Some(b)) => a.refname == b.refname,
                _ => false,
            })
            .unwrap_or(0);
        self.mode = Mode::BranchFilter;
    }

    fn handle_filter_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => self.mode = Mode::Normal,
            KeyCode::Char('j') | KeyCode::Down => {
                if self.filter_selected + 1 < self.filter_choices.len() {
                    self.filter_selected += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.filter_selected = self.filter_selected.saturating_sub(1);
            }
            KeyCode::Enter => {
                self.branch_filter = self.filter_choices[self.filter_selected].clone();
                self.mode = Mode::Normal;
                self.selected = 0;
                if let Err(e) = self.reload() {
                    self.status = format!("reload failed: {e:#}");
                }
            }
            _ => {}
        }
    }
```

- [ ] **Step 4: 寫 src/ui/popup.rs 並接上 render**

```rust
//! Centered branch-filter popup.
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::Line,
    widgets::{Block, Clear, List, ListItem, ListState},
    Frame,
};

use crate::app::App;

pub fn render(frame: &mut Frame, app: &App) {
    let h = (app.filter_choices.len() as u16 + 2).min(15);
    let area = centered(frame.area(), 40, h);
    frame.render_widget(Clear, area);
    let items: Vec<ListItem> = app
        .filter_choices
        .iter()
        .map(|c| {
            let label = match c {
                None => "All branches".to_string(),
                Some(r) => r.name.clone(),
            };
            ListItem::new(Line::from(format!(" {label}")))
        })
        .collect();
    let list = List::new(items)
        .block(Block::bordered().title(" filter by branch "))
        .highlight_style(Style::new().add_modifier(Modifier::REVERSED));
    let mut state = ListState::default().with_selected(Some(app.filter_selected));
    frame.render_stateful_widget(list, area, &mut state);
}

fn centered(outer: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(outer.width);
    let h = h.min(outer.height);
    Rect {
        x: outer.x + (outer.width - w) / 2,
        y: outer.y + (outer.height - h) / 2,
        width: w,
        height: h,
    }
}
```

src/ui/mod.rs：mod 區塊加 `pub mod popup;`；`render` 的結尾（help 之後）追加：

```rust
    if app.mode == Mode::BranchFilter {
        popup::render(frame, app);
    }
```

- [ ] **Step 5: 追加 UI 測試到 tests/ui_render.rs**

```rust
#[test]
fn branch_filter_popup_renders_over_the_graph() {
    let f = merge_fixture();
    let mut app = app_of(&f);
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('b'),
        crossterm::event::KeyModifiers::NONE,
    ));
    let lines = render_app(&mut app, 80, 16);
    let all = lines.join("\n");
    assert!(all.contains("filter by branch"));
    assert!(all.contains("All branches"));
    assert!(all.contains("feature"));
}
```

- [ ] **Step 6: 跑測試確認通過**

Run: `cargo test 2>&1 | tail -3`
Expected: 全部 `ok`。

- [ ] **Step 7: 品質閘門後 commit**

Run: `cargo clippy --all-targets -- -D warnings && cargo fmt --check`
Expected: 無輸出。

```bash
git add src/app.rs src/ui/ tests/
git commit -m "feat: branch filter popup and manual reload"
```

---

### Task 13: 收尾（空 repo 畫面、CLI 錯誤出口測試、README、最終驗收）

**Files:**
- Modify: `src/ui/graph_view.rs`（空 repo 顯示 "No commits yet"）
- Create: `tests/cli.rs`
- Create: `README.md`
- Test: `tests/ui_render.rs`（追加）

**Interfaces:**
- Consumes: 全部先前 task
- Produces: v0.1.0 可交付狀態

- [ ] **Step 1: 追加失敗測試**

tests/ui_render.rs 追加：

```rust
#[test]
fn empty_repo_shows_a_placeholder_message_in_the_graph_panel() {
    let f = Fixture::new();
    let mut app = app_of(&f);
    let lines = render_app(&mut app, 60, 12);
    // Row 1 is the first graph-list row (row 0 is the border/title).
    // Asserting the graph panel specifically: the detail panel's own
    // "No commits yet" fallback (Task 9) would make a whole-buffer
    // assertion pass before this task's change.
    assert!(lines[1].contains("No commits yet"));
}

#[test]
fn empty_repo_with_untracked_files_shows_hint_and_uncommitted_row() {
    let f = Fixture::new();
    f.write_file("first.txt", "hi\n");
    let mut app = app_of(&f);
    let lines = render_app(&mut app, 60, 12);
    assert!(lines[1].contains("Uncommitted changes (1 files)"));
    assert!(lines[2].contains("No commits yet"));
}
```

tests/cli.rs（整檔新增；main 在進 terminal 前就失敗，所以不需要 TTY）：

```rust
use std::process::Command;

#[test]
fn not_a_repo_exits_with_code_1_and_a_friendly_message() {
    let dir = tempfile::tempdir().unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_gitgraph-tui"))
        .arg(dir.path())
        .output()
        .expect("binary runs");
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("not a git repository"));
    assert!(!stderr.contains("panicked"), "must fail cleanly, not panic");
}
```

- [ ] **Step 2: 跑測試確認失敗**

Run: `cargo test --test ui_render --test cli 2>&1 | tail -5`
Expected: 兩個 empty_repo 測試失敗（graph 面板列目前是空白——注意 detail 面板已有自己的 "No commits yet" 字樣，所以測試斷言鎖定 `lines[1]`/`lines[2]` 而非整個 buffer）；cli 測試應直接通過（Task 7 已實作錯誤出口——若失敗，修 main.rs 而不是測試）。

- [ ] **Step 3: graph_view 空狀態**

`src/ui/graph_view.rs` 的 `render` 中，`items` 那行改成：

```rust
    let mut items: Vec<ListItem> = (0..app.display_len())
        .map(|i| ListItem::new(row_line(app, i, graph_w, text_w)))
        .collect();
    if app.commits.is_empty() {
        // Spec §7: empty repo shows the hint AND the worktree status —
        // so the hint renders below the (optional) uncommitted row.
        items.push(ListItem::new(Line::from(Span::styled(
            " No commits yet",
            Style::new().fg(Color::DarkGray),
        ))));
    }
```

- [ ] **Step 4: 寫 README.md**

````markdown
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
````

- [ ] **Step 5: 跑全部測試與品質閘門，展示輸出**

Run: `set -o pipefail; cargo test 2>&1 | grep -E "^test result|FAILED" && cargo clippy --all-targets -- -D warnings && cargo fmt --check && echo GATES-OK`
Expected: 每個測試 binary 一行 `test result: ok`（lib + 5 個整合測試檔 + doctest），無任何 `FAILED`，最後印出 `GATES-OK`。（grep 配 pipefail：測試失敗時整條命令鏈會停，不會被 tail/grep 吃掉 exit code。）

- [ ] **Step 6: 最終手動驗收（對照 spec §6 草圖）**

Run: `cargo run --release -- <一個有 merge 歷史的真實 repo>`
檢查：graph 彩色分叉/收攏、`[branch]`/`[tag]` 標籤、詳情面板、`Enter` 看 diff、`/` 搜尋、`b` 篩選、`r` 重載、dirty repo 頂端黃色列、`q` 後 terminal 完好。

- [ ] **Step 7: Commit**

```bash
git add src/ui/graph_view.rs tests/cli.rs README.md
git commit -m "feat: empty-repo state, cli exit-code test, and README"
```

---

## 與 spec 的既知偏差（驗證審查後記錄，實作時不需再議）

1. **搜尋輸入列**：spec §3 列了獨立的 `ui/search_bar.rs`；計畫改在底部 help 列直接顯示 `/{input}▌`（`render_help` 的 Search 分支），功能等價、少一個檔案。若使用者反饋想要獨立輸入列再拆。
2. **lane 顏色**：spec §4 字面是「lane index % 8」；計畫用「lane 開啟時遞增計數器 % 8」——lane slot 被重複使用時會拿到新顏色，避免相鄰分支同色，視覺上更接近 Git Graph 的行為。`colors_cycle_through_the_palette` 測試鎖定此行為。
3. **revwalk 分塊**：spec §5 的「revwalk 分塊載入」實作為「oid 全量走訪（毫秒級）+ CommitInfo 分塊轉換」，見 Task 3 的設計說明。

## 完成定義（整個計畫）

- spec §1 的七項功能全部可操作；§6 鍵位表全數生效。
- `cargo test`（lib + 5 個整合測試檔）全綠、`cargo clippy --all-targets -- -D warnings` 無錯、`cargo fmt --check` 通過——輸出已展示，不是口頭宣稱。
- 對真實 repo 的手動驗收完成（Task 13 Step 6）。
- 未做事項與 spec 的偏差已記錄：無（若實作中出現，列在最終回報）。
