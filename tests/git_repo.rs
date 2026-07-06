mod common;

use common::Fixture;
use gitgraph_tui::git::GitRepo;
use gitgraph_tui::git::types::RefKind;

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
    let find = |n: &str| {
        refs.iter()
            .find(|r| r.name == n)
            .unwrap_or_else(|| panic!("missing ref {n}"))
    };
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
