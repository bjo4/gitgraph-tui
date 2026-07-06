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
                content: String::from_utf8_lossy(hunk.header())
                    .trim_end()
                    .to_string(),
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
