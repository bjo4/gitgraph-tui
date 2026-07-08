//! Cheap change detection for auto-refresh. A `Fingerprint` is a stat-only
//! snapshot of the files git rewrites on any state change (commit, checkout,
//! branch/tag edit, merge, rebase, fetch, `git add`); comparing two of them
//! tells the app when to reload — no git2 walk, no extra dependency.
//!
//! Worktree-only edits (a file changed but no git command run) do NOT touch
//! these paths; the app catches those with a separate periodic status poll.
//!
//! A stat is (mtime, size), so a rewrite that keeps both identical is invisible
//! — e.g. switching between two equal-length branch names on a filesystem with
//! coarse mtime resolution. In practice this is unreachable on ext4/APFS/NTFS
//! (nanosecond mtime), and a branch switch also rewrites `index`, whose size
//! moves with the checked-out tree; the residual risk is accepted.
use std::fs;
use std::path::Path;
use std::time::SystemTime;

/// A file's (modified-time, size), or None when it does not exist — absence
/// is itself a meaningful state (e.g. MERGE_HEAD appears only mid-merge).
type Stat = Option<(SystemTime, u64)>;

/// Ordered list of (label, stat) pairs. Equality means "no git change seen".
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Fingerprint {
    entries: Vec<(String, Stat)>,
}

/// Per-worktree files (HEAD movement, staging, in-progress merge/rebase state).
const WORKTREE_FILES: &[&str] = &["HEAD", "index", "MERGE_HEAD", "ORIG_HEAD"];

impl Fingerprint {
    /// Snapshot the refs-affecting files. `git_dir` is the per-worktree gitdir
    /// (HEAD, index, …); `common_dir` holds the shared refs (they are the same
    /// path in an ordinary repo, and differ only for linked worktrees).
    pub fn snapshot(git_dir: &Path, common_dir: &Path) -> Self {
        let mut entries = Vec::new();
        for name in WORKTREE_FILES {
            entries.push((name.to_string(), stat(&git_dir.join(name))));
        }
        entries.push((
            "packed-refs".to_string(),
            stat(&common_dir.join("packed-refs")),
        ));
        // Loose refs: any branch/tag/remote create, delete, or move shows up
        // here. Sort so the fingerprint is order-independent across reads.
        let refs_root = common_dir.join("refs");
        let mut loose = Vec::new();
        walk(&refs_root, &refs_root, &mut loose);
        loose.sort_by(|a, b| a.0.cmp(&b.0));
        entries.extend(loose);
        Self { entries }
    }
}

fn stat(path: &Path) -> Stat {
    let meta = fs::metadata(path).ok()?;
    Some((meta.modified().ok()?, meta.len()))
}

/// Depth-first walk of `dir`, pushing (path-relative-to-`base`, stat) for each
/// file. Unreadable directories are skipped rather than propagated — a missing
/// refs dir just yields no loose refs.
fn walk(base: &Path, dir: &Path, out: &mut Vec<(String, Stat)>) {
    let Ok(read) = fs::read_dir(dir) else { return };
    for entry in read.flatten() {
        let path = entry.path();
        match entry.file_type() {
            Ok(ft) if ft.is_dir() => walk(base, &path, out),
            Ok(_) => {
                let rel = path
                    .strip_prefix(base)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .into_owned();
                out.push((rel, stat(&path)));
            }
            Err(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn git_dir() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("refs/heads")).unwrap();
        fs::write(dir.path().join("HEAD"), "ref: refs/heads/main\n").unwrap();
        fs::write(dir.path().join("refs/heads/main"), "a".repeat(40)).unwrap();
        dir
    }

    #[test]
    fn an_unchanged_tree_fingerprints_equal() {
        let d = git_dir();
        let g = d.path();
        assert_eq!(Fingerprint::snapshot(g, g), Fingerprint::snapshot(g, g));
    }

    #[test]
    fn a_new_loose_ref_changes_the_fingerprint() {
        let d = git_dir();
        let g = d.path();
        let before = Fingerprint::snapshot(g, g);
        fs::write(g.join("refs/heads/feature"), "b".repeat(40)).unwrap();
        assert_ne!(before, Fingerprint::snapshot(g, g));
    }

    #[test]
    fn deleting_a_loose_ref_changes_the_fingerprint() {
        let d = git_dir();
        let g = d.path();
        let before = Fingerprint::snapshot(g, g);
        fs::remove_file(g.join("refs/heads/main")).unwrap();
        assert_ne!(before, Fingerprint::snapshot(g, g));
    }

    #[test]
    fn an_appearing_file_is_a_state_change() {
        let d = git_dir();
        let g = d.path();
        let before = Fingerprint::snapshot(g, g); // no MERGE_HEAD yet
        fs::write(g.join("MERGE_HEAD"), "x").unwrap();
        assert_ne!(
            before,
            Fingerprint::snapshot(g, g),
            "MERGE_HEAD appearing (a merge began) must be detected"
        );
    }
}
