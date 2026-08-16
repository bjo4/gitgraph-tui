//! Incremental history search and match navigation.
use std::collections::BTreeSet;
use std::ops::Bound::{Excluded, Unbounded};

use crossterm::event::{KeyCode, KeyEvent};

use super::{App, Mode};
use crate::git::types::CommitInfo;

impl App {
    fn matches_query(commit: &CommitInfo, query: &str) -> bool {
        commit.summary.to_lowercase().contains(query)
            || commit.message.to_lowercase().contains(query)
            || commit.author_name.to_lowercase().contains(query)
            || commit.id.starts_with(query)
    }

    /// Rebuild the match list for `query` over the loaded commits.
    pub(super) fn recompute_matches(&mut self, query: &str) {
        if query.is_empty() {
            self.search.matches.clear();
            return;
        }
        let query = query.to_lowercase();
        let offset = self.uncommitted_offset();
        self.search.matches = self
            .commits
            .iter()
            .enumerate()
            .filter(|(_, commit)| Self::matches_query(commit, &query))
            .map(|(index, _)| index + offset)
            .collect();
    }

    pub(super) fn handle_search_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.search.input.clear();
                self.search.query.clear();
                self.search.matches.clear();
                self.mode = Mode::Normal;
            }
            KeyCode::Enter => {
                self.search.query = self.search.input.clone();
                let query = self.search.query.clone();
                self.recompute_matches(&query);
                let count = self.search.matches.len();
                self.status = format!("{count} match{}", if count == 1 { "" } else { "es" });
                self.mode = Mode::Normal;
            }
            KeyCode::Backspace => {
                self.search.input.pop();
                self.live_search();
            }
            KeyCode::Char(character) => {
                self.search.input.push(character);
                self.live_search();
            }
            _ => {}
        }
    }

    fn live_search(&mut self) {
        let query = self.search.input.clone();
        if query.is_empty() {
            self.search.matches.clear();
            return;
        }
        self.recompute_matches(&query);
        let target = self
            .search
            .matches
            .range(self.selected..)
            .next()
            .copied()
            .or_else(|| self.search.matches.first().copied());
        if let Some(index) = target {
            self.jump_to(index);
        }
    }

    fn jump_to(&mut self, index: usize) {
        self.selected = index.min(self.display_len().saturating_sub(1));
        self.file_selected = 0;
        self.ensure_margin();
        self.sync_list_state();
    }

    /// Navigate matches. Forward search loads chunks until a match appears;
    /// wrapping happens only once the entire walk is loaded.
    pub(super) fn next_match(&mut self, direction: isize) {
        if self.search.query.is_empty() {
            self.status = "no search query — press / first".to_string();
            return;
        }
        loop {
            let found = if direction > 0 {
                self.search
                    .matches
                    .range((Excluded(self.selected), Unbounded))
                    .next()
                    .copied()
            } else {
                self.search
                    .matches
                    .range(..self.selected)
                    .next_back()
                    .copied()
            };
            if let Some(index) = found {
                self.jump_to(index);
                return;
            }
            if direction > 0 && !self.all_loaded() {
                let before = self.commits.len();
                if let Err(error) = self.load_next_chunk() {
                    self.status = format!("load failed: {error:#}");
                    return;
                }
                let offset = self.uncommitted_offset();
                let query = self.search.query.to_lowercase();
                let fresh: BTreeSet<usize> = self.commits[before..]
                    .iter()
                    .enumerate()
                    .filter(|(_, commit)| Self::matches_query(commit, &query))
                    .map(|(index, _)| before + index + offset)
                    .collect();
                self.search.matches.extend(fresh);
                continue;
            }
            let wrapped = if direction > 0 {
                self.search.matches.first()
            } else {
                self.search.matches.last()
            };
            match wrapped {
                Some(&index) => {
                    self.jump_to(index);
                    self.status = "search wrapped".to_string();
                }
                None => self.status = format!("no matches for '{}'", self.search.query),
            }
            return;
        }
    }
}
