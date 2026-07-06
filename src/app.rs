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
use crate::git::types::{CommitId, CommitInfo, DiffLine, FileChange, RefInfo, RefKind};
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
        match self.mode {
            Mode::Normal => self.handle_normal_key(key),
            Mode::Search => self.handle_search_key(key),
            Mode::Diff => self.handle_diff_key(key),
            Mode::BranchFilter => self.handle_filter_key(key),
        }
    }

    fn handle_normal_key(&mut self, key: KeyEvent) {
        match (self.focus, key.code) {
            (_, KeyCode::Char('q')) | (_, KeyCode::Esc) => self.should_quit = true,
            (_, KeyCode::Tab) => self.toggle_focus(),
            (Focus::Files, KeyCode::Enter) => self.open_diff(),
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
            (_, KeyCode::Char('/')) => {
                self.search.input.clear();
                self.mode = Mode::Search;
            }
            (_, KeyCode::Char('n')) => self.next_match(1),
            (_, KeyCode::Char('N')) => self.next_match(-1),
            (_, KeyCode::Char('g')) => self.select_top(),
            (_, KeyCode::Char('G')) => self.select_bottom(),
            (_, KeyCode::Char('b')) => self.open_branch_filter(),
            (_, KeyCode::Char('r')) => match self.reload() {
                Ok(()) => self.status = "reloaded".to_string(),
                Err(e) => self.status = format!("reload failed: {e:#}"),
            },
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
                self.diff = Some(DiffState {
                    title: file.path.clone(),
                    lines,
                    scroll: 0,
                });
                self.mode = Mode::Diff;
            }
            Err(e) => self.status = format!("diff failed: {e:#}"),
        }
    }

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
                self.search
                    .matches
                    .iter()
                    .copied()
                    .find(|&i| i > self.selected)
            } else {
                self.search
                    .matches
                    .iter()
                    .rev()
                    .copied()
                    .find(|&i| i < self.selected)
            };
            if let Some(i) = found {
                self.jump_to(i);
                return;
            }
            if dir > 0 && !self.all_loaded() {
                let before = self.commits.len();
                if let Err(e) = self.load_next_chunk() {
                    self.status = format!("load failed: {e:#}");
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
            KeyCode::Char('j') | KeyCode::Down
                if self.filter_selected + 1 < self.filter_choices.len() =>
            {
                self.filter_selected += 1;
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
}
