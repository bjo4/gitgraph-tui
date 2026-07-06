mod common;

use common::Fixture;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use gitgraph_tui::app::App;
use gitgraph_tui::app::Focus;
use gitgraph_tui::git::GitRepo;
use gitgraph_tui::git::types::ChangeKind;

pub fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

pub fn ch(c: char) -> KeyEvent {
    key(KeyCode::Char(c))
}

/// Linear history of `n` commits on main, clean worktree.
/// Returns the fixture too: its TempDir must outlive the App.
fn linear_app(n: usize, chunk: usize) -> (Fixture, App) {
    let f = Fixture::new();
    let mut prev: Vec<git2::Oid> = vec![];
    for i in 0..n {
        let oid = f.commit(
            &format!("commit {i}"),
            &[("a.txt", &format!("{i}\n"))],
            &[],
            &prev,
            1_000 + i as i64,
        );
        prev = vec![oid];
    }
    f.branch("main", prev[0]);
    f.set_head("refs/heads/main");
    let repo = GitRepo::discover(f.path()).unwrap();
    let mut app = App::new_at(repo, 10_000).unwrap();
    app.chunk_size = chunk;
    app.load_margin = 1;
    (f, app)
}

#[test]
fn new_app_loads_only_the_first_chunk() {
    let (_f, mut app) = linear_app(10, 3);
    // new_at ran with the default chunk; rebuild state with the small one
    app.reload().unwrap();
    assert_eq!(app.commits.len(), 3);
    assert_eq!(app.total_len(), 10);
    assert!(!app.all_loaded());
    assert_eq!(app.selected, 0);
}

#[test]
fn moving_down_lazily_loads_more_chunks() {
    let (_f, mut app) = linear_app(10, 3);
    app.reload().unwrap();
    for _ in 0..5 {
        app.handle_key(ch('j'));
    }
    assert_eq!(app.selected, 5);
    assert!(app.commits.len() > 3, "moving near the end must load more");
}

#[test]
fn selection_clamps_at_both_ends() {
    let (_f, mut app) = linear_app(3, 300);
    app.handle_key(ch('k'));
    assert_eq!(app.selected, 0);
    for _ in 0..99 {
        app.handle_key(ch('j'));
    }
    assert_eq!(app.selected, 2);
}

#[test]
fn capital_g_loads_everything_and_jumps_to_bottom() {
    let (_f, mut app) = linear_app(10, 3);
    app.reload().unwrap();
    app.handle_key(ch('G'));
    assert!(app.all_loaded());
    assert_eq!(app.selected, app.display_len() - 1);
    app.handle_key(ch('g'));
    assert_eq!(app.selected, 0);
}

#[test]
fn q_quits() {
    let (_f, mut app) = linear_app(2, 300);
    app.handle_key(ch('q'));
    assert!(app.should_quit);
}

#[test]
fn dirty_worktree_adds_a_synthetic_first_row() {
    let f = Fixture::new();
    let c1 = f.commit("base", &[("a.txt", "1\n")], &[], &[], 1_000);
    f.branch("main", c1);
    f.set_head("refs/heads/main");
    f.write_file("a.txt", "dirty\n");
    let repo = GitRepo::discover(f.path()).unwrap();
    let app = App::new_at(repo, 10_000).unwrap();
    assert_eq!(app.uncommitted_offset(), 1);
    assert_eq!(app.display_len(), 2);
    assert!(
        app.selected_commit().is_none(),
        "row 0 is the uncommitted row"
    );
}

#[test]
fn selected_commit_maps_display_index_to_commit() {
    let (_f, mut app) = linear_app(3, 300);
    app.handle_key(ch('j'));
    let c = app.selected_commit().unwrap();
    assert_eq!(c.summary, "commit 1"); // topo order: newest (2) first
}

#[test]
fn empty_repo_yields_an_empty_app() {
    let f = Fixture::new();
    let repo = GitRepo::discover(f.path()).unwrap();
    let app = App::new_at(repo, 10_000).unwrap();
    assert_eq!(app.display_len(), 0);
    assert!(app.selected_commit().is_none());
}

#[test]
fn load_errors_surface_in_status_instead_of_vanishing() {
    let (_f, mut app) = linear_app(6, 2);
    app.reload().unwrap();
    // Inject an unparseable id into a not-yet-loaded chunk (oids is pub):
    // Oid::from_str rejects non-hex, so the next chunk load must fail.
    app.oids[4] = "z".repeat(40);
    for _ in 0..5 {
        app.handle_key(ch('j'));
    }
    assert!(
        app.status.contains("load failed"),
        "status was: {:?}",
        app.status
    );
}

#[test]
fn current_files_returns_commit_changes_and_caches_them() {
    let (_f, mut app) = linear_app(3, 300);
    let files = app.current_files();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].path, "a.txt");
    assert_eq!(files[0].kind, ChangeKind::Modified); // commit 2 rewrites a.txt
    let id = app.selected_commit().unwrap().id.clone();
    assert!(app.detail_cache.contains(&id), "result must be cached");
}

#[test]
fn current_files_for_the_uncommitted_row_lists_worktree_changes() {
    let f = Fixture::new();
    let c1 = f.commit("base", &[("a.txt", "1\n")], &[], &[], 1_000);
    f.branch("main", c1);
    f.set_head("refs/heads/main");
    f.write_file("b.txt", "new\n");
    let repo = GitRepo::discover(f.path()).unwrap();
    let mut app = App::new_at(repo, 10_000).unwrap();
    let files = app.current_files();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].path, "b.txt");
}

#[test]
fn tab_toggles_focus_and_j_k_move_the_file_cursor() {
    let f = Fixture::new();
    let c1 = f.commit(
        "two files",
        &[("a.txt", "1\n"), ("b.txt", "2\n")],
        &[],
        &[],
        1_000,
    );
    f.branch("main", c1);
    f.set_head("refs/heads/main");
    let repo = GitRepo::discover(f.path()).unwrap();
    let mut app = App::new_at(repo, 10_000).unwrap();
    assert_eq!(app.focus, Focus::Commits);
    app.handle_key(key(KeyCode::Tab));
    assert_eq!(app.focus, Focus::Files);
    app.handle_key(ch('j'));
    assert_eq!(app.file_selected, 1);
    app.handle_key(ch('j'));
    assert_eq!(app.file_selected, 1, "clamps at the last file");
    app.handle_key(ch('k'));
    assert_eq!(app.file_selected, 0);
    app.handle_key(key(KeyCode::Tab));
    assert_eq!(app.focus, Focus::Commits);
}

#[test]
fn moving_the_commit_cursor_resets_file_focus_state() {
    let (_f, mut app) = linear_app(3, 300);
    app.handle_key(key(KeyCode::Tab));
    app.handle_key(ch('j')); // file cursor (only 1 file → stays 0)
    app.handle_key(key(KeyCode::Tab));
    app.handle_key(ch('j')); // commit cursor
    assert_eq!(app.file_selected, 0);
}
