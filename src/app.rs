//! Application state machine. All state changes flow through handle_key —
//! the UI layer only reads this struct.
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use lru::LruCache;
use ratatui::widgets::ListState;

use crate::git::GitRepo;
use crate::git::types::{CommitId, CommitInfo, DiffLine, FileChange, RefInfo};
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
            if let Err(e) = self.load_next_chunk() {
                self.status = format!("load failed: {e:#}");
                break;
            }
        }
    }

    fn ensure_margin(&mut self) {
        while !self.all_loaded() && self.selected + self.load_margin >= self.display_len() {
            if let Err(e) = self.load_next_chunk() {
                self.status = format!("load failed: {e:#}");
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
}
