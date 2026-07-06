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
