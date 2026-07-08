//! Auto-refresh: an idle `on_tick` reflects external git changes and unsaved
//! worktree edits without a keypress, keeping the user's place.
mod common;

use common::Fixture;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use gitgraph_tui::app::App;
use gitgraph_tui::git::GitRepo;

/// Must match `WORKTREE_POLL_TICKS` in app.rs — the idle ticks between
/// worktree-status polls.
const WORKTREE_POLL_TICKS: usize = 8;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}
fn ch(c: char) -> KeyEvent {
    key(KeyCode::Char(c))
}

/// Linear history of `n` commits on `main`, HEAD at the tip, clean worktree.
/// Returns the fixture (its TempDir must outlive the App) and the commit oids
/// oldest→newest.
fn linear_app(n: usize) -> (Fixture, App, Vec<git2::Oid>) {
    let f = Fixture::new();
    let mut oids = Vec::new();
    let mut prev: Vec<git2::Oid> = vec![];
    for i in 0..n {
        let oid = f.commit(
            &format!("commit {i}"),
            &[("a.txt", &format!("{i}\n"))],
            &[],
            &prev,
            1_000 + i as i64,
        );
        oids.push(oid);
        prev = vec![oid];
    }
    f.branch("main", *oids.last().unwrap());
    f.set_head("refs/heads/main");
    let repo = GitRepo::discover(f.path()).unwrap();
    let app = App::new_at(repo, 10_000).unwrap();
    (f, app, oids)
}

#[test]
fn on_tick_picks_up_a_new_commit_and_keeps_the_cursor_put() {
    let (f, mut app, oids) = linear_app(5);
    app.handle_key(ch('j'));
    app.handle_key(ch('j')); // topo order newest-first → "commit 2"
    let anchored = app.selected_commit().unwrap().id.clone();
    assert_eq!(app.selected_commit().unwrap().summary, "commit 2");
    let before = app.total_len();

    // Another terminal commits on top of main.
    let tip = *oids.last().unwrap();
    let fresh = f.commit(
        "commit 5 fresh",
        &[("a.txt", "5\n"), ("new.txt", "x\n")],
        &[],
        &[tip],
        6_000,
    );
    f.branch("main", fresh);

    app.on_tick();

    assert_eq!(app.total_len(), before + 1, "new commit shows up");
    assert_eq!(
        app.selected_commit().unwrap().id,
        anchored,
        "the same commit stays under the cursor after it shifts down a row"
    );
    assert_eq!(app.selected_commit().unwrap().summary, "commit 2");
}

#[test]
fn auto_refresh_keeps_a_confirmed_search() {
    let (f, mut app, oids) = linear_app(5);
    app.handle_key(ch('/'));
    for c in "commit".chars() {
        app.handle_key(ch(c));
    }
    app.handle_key(key(KeyCode::Enter));
    assert_eq!(app.search.matches.len(), 5);

    let tip = *oids.last().unwrap();
    let fresh = f.commit(
        "commit 5 fresh",
        &[("a.txt", "5\n"), ("z.txt", "z\n")],
        &[],
        &[tip],
        6_000,
    );
    f.branch("main", fresh);
    app.on_tick();

    assert_eq!(app.search.query, "commit", "query survives auto-refresh");
    assert_eq!(
        app.search.matches.len(),
        6,
        "the new commit is folded into the match set"
    );
}

#[test]
fn an_unsaved_edit_appears_after_the_worktree_poll() {
    let (f, mut app, _oids) = linear_app(3);
    assert_eq!(app.uncommitted_offset(), 0);
    let newest = app.selected_commit().unwrap().id.clone();

    // Worktree-only change: no git command, so `.git` is untouched and the
    // fingerprint stays put — this must be caught by the periodic poll.
    f.write_file("untracked.txt", "hello\n");

    for _ in 0..(WORKTREE_POLL_TICKS - 1) {
        app.on_tick();
    }
    assert_eq!(app.uncommitted_offset(), 0, "not polled yet");

    app.on_tick(); // the poll fires on this tick
    assert_eq!(app.uncommitted_offset(), 1, "uncommitted row appears");
    assert_eq!(
        app.selected, 1,
        "cursor shifted past the new synthetic row 0"
    );
    assert_eq!(
        app.selected_commit().unwrap().id,
        newest,
        "still on the same commit"
    );
}

#[test]
fn auto_refresh_drops_a_filter_whose_branch_was_deleted() {
    let f = Fixture::new();
    let c1 = f.commit("c1", &[("a.txt", "1")], &[], &[], 1_000);
    let c2 = f.commit("c2 main", &[("a.txt", "2")], &[], &[c1], 2_000);
    let c3 = f.commit("c3 feature", &[("b.txt", "3")], &[], &[c1], 3_000);
    f.branch("main", c2);
    f.branch("feature", c3);
    f.set_head("refs/heads/main");
    let repo = GitRepo::discover(f.path()).unwrap();
    let mut app = App::new_at(repo, 10_000).unwrap();

    // Narrow to the feature branch.
    app.handle_key(ch('b'));
    let pos = app
        .filter_choices
        .iter()
        .position(|c| c.as_ref().is_some_and(|r| r.name == "feature"))
        .unwrap();
    for _ in 0..pos {
        app.handle_key(ch('j'));
    }
    app.handle_key(key(KeyCode::Enter));
    assert!(app.branch_filter.is_some());

    // Another terminal deletes that branch.
    let mut r = f.repo.find_reference("refs/heads/feature").unwrap();
    r.delete().unwrap();

    app.on_tick();
    assert!(
        app.branch_filter.is_none(),
        "a filter on a vanished branch falls back to all branches"
    );
    assert!(app.status.contains("gone"), "status was: {:?}", app.status);
    assert!(app.total_len() >= 2, "history is visible again");
}

#[test]
fn on_tick_is_a_no_op_while_git_is_unchanged() {
    let (_f, mut app, _oids) = linear_app(4);
    app.handle_key(ch('j'));
    let selected = app.selected;
    let total = app.total_len();

    // Enough ticks to trigger several worktree polls; the repo is clean, so
    // nothing should move and no status noise should appear.
    for _ in 0..(WORKTREE_POLL_TICKS * 3) {
        app.on_tick();
    }

    assert_eq!(app.selected, selected);
    assert_eq!(app.total_len(), total);
    assert!(app.status.is_empty(), "status was: {:?}", app.status);
}
