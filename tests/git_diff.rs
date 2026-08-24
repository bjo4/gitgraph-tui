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
            .find(|c| c.path == std::path::Path::new(p))
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
    assert_eq!(files[0].path, std::path::Path::new("new_name.txt"));
    assert_eq!(
        files[0].kind,
        ChangeKind::Renamed {
            from: "old_name.txt".into()
        }
    );
}

#[test]
fn opening_a_rename_uses_the_same_similarity_detection_as_the_file_list() {
    let f = Fixture::new();
    let c1 = f.commit(
        "base",
        &[("old.txt", "same content\nlines\n")],
        &[],
        &[],
        1_000,
    );
    let c2 = f.commit(
        "rename",
        &[("new.txt", "same content\nlines\n")],
        &["old.txt"],
        &[c1],
        2_000,
    );
    f.branch("main", c2);
    f.set_head("refs/heads/main");
    let repo = GitRepo::discover(f.path()).unwrap();
    let lines = repo.commit_file_diff(&c2.to_string(), "new.txt").unwrap();
    assert!(
        lines.iter().all(|line| !matches!(line.origin, '+' | '-')),
        "an identical rename must not render as a whole-file delete/add"
    );
}

#[test]
fn metacharacters_in_a_filename_are_matched_literally() {
    let f = Fixture::new();
    let c1 = f.commit(
        "base",
        &[("a1.txt", "plain\n"), ("a[1].txt", "old\n")],
        &[],
        &[],
        1_000,
    );
    let c2 = f.commit(
        "edit",
        &[("a1.txt", "other\n"), ("a[1].txt", "literal\n")],
        &[],
        &[c1],
        2_000,
    );
    f.branch("main", c2);
    f.set_head("refs/heads/main");
    let repo = GitRepo::discover(f.path()).unwrap();
    let lines = repo.commit_file_diff(&c2.to_string(), "a[1].txt").unwrap();
    assert!(
        lines
            .iter()
            .any(|line| line.origin == '+' && line.content == "literal")
    );
    assert!(!lines.iter().any(|line| line.content == "other"));
}

#[test]
fn diff_preserves_the_no_newline_marker() {
    let f = Fixture::new();
    let c1 = f.commit("base", &[("a.txt", "old")], &[], &[], 1_000);
    let c2 = f.commit("edit", &[("a.txt", "new")], &[], &[c1], 2_000);
    f.branch("main", c2);
    f.set_head("refs/heads/main");
    let repo = GitRepo::discover(f.path()).unwrap();
    let lines = repo.commit_file_diff(&c2.to_string(), "a.txt").unwrap();
    assert!(lines.iter().any(|line| line.origin == '\\'));
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
    let paths: Vec<&str> = files.iter().filter_map(|c| c.path.to_str()).collect();
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
    let paths: Vec<&str> = files.iter().filter_map(|c| c.path.to_str()).collect();
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

#[test]
fn branch_changes_split_staged_and_unstaged_worktree_changes() {
    let f = Fixture::new();
    let base = f.commit(
        "base",
        &[("staged.txt", "old\n"), ("unstaged.txt", "before\n")],
        &[],
        &[],
        1_000,
    );
    f.branch("feature", base);
    f.set_head("refs/heads/feature");
    f.write_file("staged.txt", "new staged\n");
    {
        let mut index = f.repo.index().unwrap();
        index.add_path(std::path::Path::new("staged.txt")).unwrap();
        index.write().unwrap();
    }
    f.write_file("unstaged.txt", "new unstaged\n");
    f.write_file("untracked.txt", "hello\n");
    let repo = GitRepo::discover(f.path()).unwrap();
    let branch = repo
        .refs()
        .unwrap()
        .into_iter()
        .find(|r| r.refname == "refs/heads/feature")
        .unwrap();

    let changes = repo.branch_changes(&branch).unwrap();
    assert_eq!(changes.branch_name, "feature");
    assert_eq!(changes.staged.len(), 1);
    assert_eq!(changes.staged[0].path, std::path::Path::new("staged.txt"));
    assert!(
        changes
            .unstaged
            .iter()
            .any(|f| f.path == std::path::Path::new("unstaged.txt"))
    );
    assert!(
        changes
            .unstaged
            .iter()
            .any(|f| f.path == std::path::Path::new("untracked.txt"))
    );
}

#[test]
fn branch_file_diff_reads_staged_and_unstaged_sources_separately() {
    let f = Fixture::new();
    let base = f.commit("base", &[("a.txt", "old\n")], &[], &[], 1_000);
    f.branch("feature", base);
    f.set_head("refs/heads/feature");
    f.write_file("a.txt", "staged\n");
    {
        let mut index = f.repo.index().unwrap();
        index.add_path(std::path::Path::new("a.txt")).unwrap();
        index.write().unwrap();
    }
    f.write_file("a.txt", "unstaged\n");
    let repo = GitRepo::discover(f.path()).unwrap();
    let branch = repo
        .refs()
        .unwrap()
        .into_iter()
        .find(|r| r.refname == "refs/heads/feature")
        .unwrap();

    let staged_lines = repo.branch_file_diff(&branch, true, "a.txt").unwrap();
    assert!(staged_lines.iter().any(|l| l.origin == '+' && l.content == "staged"));
    let unstaged_lines = repo.branch_file_diff(&branch, false, "a.txt").unwrap();
    assert!(unstaged_lines.iter().any(|l| l.origin == '-' && l.content == "staged"));
    assert!(unstaged_lines.iter().any(|l| l.origin == '+' && l.content == "unstaged"));
}

#[cfg(unix)]
#[test]
fn non_utf8_worktree_path_round_trips_into_the_diff() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;
    use std::path::PathBuf;

    let f = Fixture::new();
    let path = PathBuf::from(OsString::from_vec(b"invalid-\xff.txt".to_vec()));
    std::fs::write(f.path().join(&path), "content\n").unwrap();
    let repo = GitRepo::discover(f.path()).unwrap();

    let files = repo.worktree_status().unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].path, path);
    let lines = repo.worktree_file_diff(&files[0].path).unwrap();
    assert!(
        lines
            .iter()
            .any(|line| line.origin == '+' && line.content == "content")
    );
}
