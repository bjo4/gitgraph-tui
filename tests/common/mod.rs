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

    /// Create or force-move a local branch ref. Uses the raw reference API
    /// (not `Repository::branch`, which refuses to force-move the branch
    /// that is the current HEAD — libgit2 mirrors `git branch -f`'s refusal
    /// to force-update the checked-out branch). Fixtures build history via
    /// detached commits with explicit parents, so re-pointing the checked-out
    /// branch after the fact (simulating "new commits appeared upstream")
    /// must be allowed here.
    pub fn branch(&self, name: &str, target: Oid) {
        self.repo
            .reference(&format!("refs/heads/{name}"), target, true, "test: branch")
            .expect("branch");
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
