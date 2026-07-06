mod common;

use common::Fixture;
use gitgraph_tui::git::GitRepo;
use gitgraph_tui::git::types::ChangeKind;

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
    let by_path = |p: &str| {
        files
            .iter()
            .find(|c| c.path == p)
            .unwrap_or_else(|| panic!("missing {p}"))
    };
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
    let c1 = f.commit(
        "base",
        &[("old_name.txt", "same content\nlines\n")],
        &[],
        &[],
        1_000,
    );
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
        ChangeKind::Renamed {
            from: "old_name.txt".to_string()
        }
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
    assert!(
        lines
            .iter()
            .any(|l| l.origin == '@' && l.content.starts_with("@@"))
    );
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
    assert!(
        paths.contains(&"feat.txt"),
        "second-parent change appears vs first parent"
    );
    assert!(
        !paths.contains(&"a.txt"),
        "first-parent content is not a change"
    );
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
