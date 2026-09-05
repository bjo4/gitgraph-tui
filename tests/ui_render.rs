mod common;

use common::Fixture;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use gitgraph_tui::update::{InstallStep, UpdateAction, UpdateInfo, UpdateMessage, UpdateState};
use gitgraph_tui::{app::App, git::GitRepo, ui};
use ratatui::{Terminal, backend::TestBackend};
use unicode_width::UnicodeWidthStr;

/// Render into a TestBackend and return the buffer as row strings.
///
/// Deviation from the brief: a naive `c.symbol()` join duplicates every
/// double-width (e.g. CJK) glyph as `"<glyph> "` — ratatui's buffer stores an
/// explicit `" "` placeholder in the cell a wide glyph's second column
/// occupies (verified against ratatui-core 0.1.2's `Buffer::set_stringn`,
/// which calls `Cell::reset()` on that trailing cell, and `Cell::symbol()`
/// renders a reset/empty cell as `" "`). Left as-is, `cjk_summaries_render_
/// without_panicking` could never match `"加入中文"` regardless of the
/// summary column's width budget. Skipping the placeholder cell that follows
/// a width-2 glyph reproduces what a real terminal shows.
pub fn render_app(app: &mut App, w: u16, h: u16) -> Vec<String> {
    let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
    term.draw(|f| ui::render(f, app)).unwrap();
    let buf = term.backend().buffer().clone();
    let width = buf.area.width as usize;
    buf.content()
        .chunks(width)
        .map(|row| {
            let mut out = String::new();
            let mut skip_next = false;
            for cell in row {
                if skip_next {
                    skip_next = false;
                    continue;
                }
                let symbol = cell.symbol();
                skip_next = symbol.width() > 1;
                out.push_str(symbol);
            }
            out
        })
        .collect()
}

fn merge_fixture() -> Fixture {
    let f = Fixture::new();
    let c1 = f.commit("init", &[("a.txt", "1\n")], &[], &[], 1_000);
    let c2 = f.commit("main work", &[("a.txt", "2\n")], &[], &[c1], 2_000);
    let c3 = f.commit("feature work", &[("b.txt", "3\n")], &[], &[c1], 3_000);
    let c4 = f.commit("merge feature", &[], &[], &[c2, c3], 4_000);
    f.branch("main", c4);
    f.branch("feature", c3);
    f.tag("v1.0", c1);
    f.remote_branch("feature", c3);
    f.set_head("refs/heads/main");
    f
}

fn app_of(f: &Fixture) -> App {
    let repo = GitRepo::discover(f.path()).unwrap();
    App::new_at(repo, 10_000).unwrap()
}

#[test]
fn graph_rows_show_dots_labels_summary_author_and_age() {
    let f = merge_fixture();
    let mut app = app_of(&f);
    let lines = render_app(&mut app, 90, 16);
    let all = lines.join("\n");
    assert!(all.contains('●'), "graph dots render");
    assert!(
        all.contains('╮') || all.contains('╯'),
        "merge curves render"
    );
    assert!(all.contains("[main]"), "branch label renders");
    assert!(all.contains("[v1.0]"), "tag label renders");
    assert!(
        all.contains("[origin/feature]"),
        "remote branch label renders"
    );
    assert!(all.contains("merge feature"), "summary renders");
    assert!(all.contains("Test Author"), "author renders");
    assert!(
        all.contains(&app.commits[0].short_id),
        "short hash renders in the list"
    );
    assert!(
        all.contains("2h"),
        "relative age renders (c2/c1 rows are 8_000-9_000s old = 2h)"
    );
}

#[test]
fn title_shows_repo_name_filter_and_counts() {
    let f = merge_fixture();
    let mut app = app_of(&f);
    let lines = render_app(&mut app, 90, 16);
    assert!(lines[0].contains("all branches"));
    assert!(lines[0].contains("4/4"));
}

#[test]
fn uncommitted_row_renders_at_the_top() {
    let f = merge_fixture();
    f.write_file("a.txt", "dirty\n");
    let mut app = app_of(&f);
    let lines = render_app(&mut app, 90, 16);
    assert!(lines.join("\n").contains("Uncommitted changes (1 files)"));
}

#[test]
fn help_line_lists_the_key_bindings() {
    let f = merge_fixture();
    let mut app = app_of(&f);
    let lines = render_app(&mut app, 100, 16);
    let last = lines.last().unwrap();
    assert!(last.contains("q:quit"));
    assert!(last.contains("/:search"));
}

#[test]
fn cjk_summaries_render_without_panicking() {
    let f = Fixture::new();
    let c1 = f.commit("加入中文訊息的提交", &[("a.txt", "1\n")], &[], &[], 1_000);
    f.branch("main", c1);
    f.set_head("refs/heads/main");
    let mut app = app_of(&f);
    // 60 cols: after graph column, [HEAD] [main] labels, author and age,
    // the summary budget still fits 加入中文… (verified: labels eat 14 cols).
    let lines = render_app(&mut app, 60, 10);
    assert!(lines.join("\n").contains("加入中文"));
}

#[test]
fn detail_panel_shows_commit_meta_and_file_list() {
    let f = merge_fixture();
    let mut app = app_of(&f);
    // select "main work" (row with a real file change)
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('j'),
        crossterm::event::KeyModifiers::NONE,
    ));
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('j'),
        crossterm::event::KeyModifiers::NONE,
    ));
    let lines = render_app(&mut app, 90, 20);
    let all = lines.join("\n");
    assert!(all.contains("Test Author <test@example.com>"));
    assert!(all.contains("1970-01-01 00:"), "absolute date renders");
    assert!(
        all.contains("M a.txt"),
        "file list renders with change kind"
    );
    assert!(all.contains("+1"), "addition count renders");
}

#[test]
fn detail_panel_shows_the_full_message_body() {
    let f = Fixture::new();
    let c1 = f.commit(
        "subject line\n\nbody first line\nbody second line",
        &[("a.txt", "1\n")],
        &[],
        &[],
        1_000,
    );
    f.branch("main", c1);
    f.set_head("refs/heads/main");
    let mut app = app_of(&f);
    let lines = render_app(&mut app, 90, 24);
    let all = lines.join("\n");
    assert!(all.contains("body first line"));
    assert!(all.contains("body second line"));
}

#[test]
fn detail_panel_for_uncommitted_row() {
    let f = merge_fixture();
    f.write_file("z.txt", "wip\n");
    let mut app = app_of(&f);
    let lines = render_app(&mut app, 90, 20);
    let all = lines.join("\n");
    assert!(all.contains("Uncommitted changes"));
    assert!(all.contains("A z.txt"));
}

#[test]
fn search_mode_shows_the_live_input_in_the_help_line() {
    let f = merge_fixture();
    let mut app = app_of(&f);
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('/'),
        crossterm::event::KeyModifiers::NONE,
    ));
    for c in "feat".chars() {
        app.handle_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char(c),
            crossterm::event::KeyModifiers::NONE,
        ));
    }
    let lines = render_app(&mut app, 80, 12);
    assert!(lines.last().unwrap().contains("/feat"));
}

#[test]
fn diff_view_renders_colored_lines_full_screen() {
    let f = Fixture::new();
    let c1 = f.commit("base", &[("a.txt", "one\n")], &[], &[], 1_000);
    let c2 = f.commit("edit", &[("a.txt", "one\ntwo\n")], &[], &[c1], 2_000);
    f.branch("main", c2);
    f.set_head("refs/heads/main");
    let mut app = app_of(&f);
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Tab,
        crossterm::event::KeyModifiers::NONE,
    ));
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Enter,
        crossterm::event::KeyModifiers::NONE,
    ));
    let lines = render_app(&mut app, 60, 12);
    let all = lines.join("\n");
    assert!(all.contains("a.txt"), "title shows the file path");
    assert!(all.contains("@@"), "hunk header renders");
    assert!(all.contains("+two"), "addition renders");
    assert!(!all.contains("all branches"), "graph view is fully covered");
}

#[test]
fn diff_bottom_scroll_accounts_for_the_visible_viewport() {
    let f = Fixture::new();
    let many: String = (0..50).map(|i| format!("line{i}\n")).collect();
    let c1 = f.commit("big", &[("a.txt", &many)], &[], &[], 1_000);
    f.branch("main", c1);
    f.set_head("refs/heads/main");
    let mut app = app_of(&f);
    for code in [
        crossterm::event::KeyCode::Tab,
        crossterm::event::KeyCode::Enter,
    ] {
        app.handle_key(crossterm::event::KeyEvent::new(
            code,
            crossterm::event::KeyModifiers::NONE,
        ));
    }
    render_app(&mut app, 60, 12);
    let diff = app.diff.as_ref().unwrap();
    let expected_bottom = diff.lines.len().saturating_sub(diff.viewport_height);
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('G'),
        crossterm::event::KeyModifiers::NONE,
    ));
    assert_eq!(app.diff.as_ref().unwrap().scroll, expected_bottom);
}

#[test]
fn empty_repo_shows_a_placeholder_message_in_the_graph_panel() {
    let f = Fixture::new();
    let mut app = app_of(&f);
    let lines = render_app(&mut app, 60, 12);
    // Row 1 is the first graph-list row (row 0 is the border/title).
    // Asserting the graph panel specifically: the detail panel's own
    // "No commits yet" fallback (Task 9) would make a whole-buffer
    // assertion pass before this task's change.
    assert!(lines[1].contains("No commits yet"));
}

#[test]
fn branch_changes_view_renders_split_panes() {
    let f = Fixture::new();
    let base = f.commit("base", &[("a.txt", "base\n")], &[], &[], 1_000);
    f.branch("main", base);
    f.branch("feature", base);
    f.set_head("refs/heads/main");
    f.write_file("a.txt", "staged\n");
    {
        let mut index = f.repo.index().unwrap();
        index.add_path(std::path::Path::new("a.txt")).unwrap();
        index.write().unwrap();
    }
    f.write_file("b.txt", "new\n");
    let mut app = app_of(&f);

    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('b'),
        crossterm::event::KeyModifiers::NONE,
    ));
    let pos = app
        .filter_choices
        .iter()
        .position(|c| c.as_ref().is_some_and(|r| r.name == "feature"))
        .unwrap();
    for _ in 0..pos {
        app.handle_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('j'),
            crossterm::event::KeyModifiers::NONE,
        ));
    }
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Enter,
        crossterm::event::KeyModifiers::NONE,
    ));
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('c'),
        crossterm::event::KeyModifiers::NONE,
    ));
    let b_pos = app
        .branch_changes
        .as_ref()
        .unwrap()
        .entries
        .iter()
        .position(|entry| entry.file.path == std::path::Path::new("b.txt"))
        .unwrap();
    for _ in 0..b_pos {
        app.handle_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('j'),
            crossterm::event::KeyModifiers::NONE,
        ));
    }

    let lines = render_app(&mut app, 90, 18);
    let all = lines.join("\n");
    assert!(all.contains("changes feature"));
    assert!(all.contains("Added"));
    assert!(all.contains("Not added"));
    assert!(all.contains("b.txt"));
    assert!(all.contains("a.txt"));
    assert!(all.contains("+new"));
    assert!(
        !all.contains("all branches"),
        "branch changes view covers the graph"
    );
}

#[test]
fn tiny_terminal_does_not_panic() {
    let f = merge_fixture();
    let mut app = app_of(&f);
    for (width, height) in [(1, 1), (2, 2), (5, 3)] {
        let _ = render_app(&mut app, width, height);
    }
}

#[test]
fn empty_repo_with_untracked_files_shows_hint_and_uncommitted_row() {
    let f = Fixture::new();
    f.write_file("first.txt", "hi\n");
    let mut app = app_of(&f);
    let lines = render_app(&mut app, 60, 12);
    assert!(lines[1].contains("Uncommitted changes (1 files)"));
    assert!(lines[2].contains("No commits yet"));
}

#[test]
fn graph_virtualization_keeps_the_bottom_selection_visible() {
    let f = Fixture::new();
    let mut parents = Vec::new();
    for i in 0..30 {
        let oid = f.commit(
            &format!("commit {i}"),
            &[("a.txt", &format!("{i}\n"))],
            &[],
            &parents,
            1_000 + i,
        );
        parents = vec![oid];
    }
    f.branch("main", parents[0]);
    f.set_head("refs/heads/main");
    let mut app = app_of(&f);
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('G'),
        crossterm::event::KeyModifiers::NONE,
    ));
    let lines = render_app(&mut app, 70, 12);
    assert!(lines.join("\n").contains("commit 0"));
    assert!(app.list_state.offset() > 0);
}

#[test]
fn detail_virtualization_keeps_the_selected_file_visible() {
    let f = Fixture::new();
    let names: Vec<String> = (0..20).map(|i| format!("f{i:02}.txt")).collect();
    let adds: Vec<(&str, &str)> = names.iter().map(|name| (name.as_str(), "x\n")).collect();
    let c1 = f.commit("many files", &adds, &[], &[], 1_000);
    f.branch("main", c1);
    f.set_head("refs/heads/main");
    let mut app = app_of(&f);
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Tab,
        crossterm::event::KeyModifiers::NONE,
    ));
    for _ in 0..19 {
        app.handle_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('j'),
            crossterm::event::KeyModifiers::NONE,
        ));
    }
    let lines = render_app(&mut app, 70, 20);
    assert!(lines.join("\n").contains("f19.txt"));
}

#[test]
fn branch_filter_popup_renders_over_the_graph() {
    let f = merge_fixture();
    let mut app = app_of(&f);
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('b'),
        crossterm::event::KeyModifiers::NONE,
    ));
    let lines = render_app(&mut app, 80, 16);
    let all = lines.join("\n");
    assert!(all.contains("filter by branch"));
    assert!(all.contains("All branches"));
    assert!(all.contains("feature"));
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn available_app(f: &Fixture) -> App {
    let mut app = app_of(f);
    app.update = UpdateState::Available(UpdateInfo {
        current: "0.2.1".to_string(),
        latest: "v0.3.0".to_string(),
        url: "https://github.com/bjo4/gitgraph-tui/releases/tag/v0.3.0".to_string(),
        action: UpdateAction::SelfUpdate,
    });
    app
}

#[test]
fn an_available_update_swaps_the_top_bottom_hint_for_the_update_hint() {
    // The help line has ~3 columns of slack at width 100 (see commit bc6221f),
    // so `u:update` has to take a seat rather than claim a new one.
    let f = merge_fixture();
    let mut app = available_app(&f);
    let lines = render_app(&mut app, 100, 16);
    let last = lines.last().unwrap();
    assert!(last.contains("u:update"), "the update hint is shown");
    assert!(last.contains("q:quit"), "quit must stay visible");
    assert!(last.contains("/:search"), "search must stay visible");
    assert!(
        !last.contains("g/G:top/bot"),
        "g/G gives up its slot while an update is pending"
    );
}

#[test]
fn an_idle_app_keeps_the_help_line_exactly_as_it_was() {
    let f = merge_fixture();
    let mut app = app_of(&f);
    let lines = render_app(&mut app, 100, 16);
    let last = lines.last().unwrap();
    assert!(last.contains("g/G:top/bot"));
    assert!(!last.contains("u:update"));
}

#[test]
fn the_update_popup_shows_both_versions_and_the_keys() {
    let f = merge_fixture();
    let mut app = available_app(&f);
    app.handle_key(key(KeyCode::Char('u')));
    let all = render_app(&mut app, 100, 16).join("\n");
    assert!(all.contains("0.2.1"), "the current version is shown");
    assert!(all.contains("v0.3.0"), "the new version is shown");
    assert!(all.contains("update now"));
    assert!(all.contains("not now"));
}

#[test]
fn a_manual_action_shows_the_command_instead_of_an_update_button() {
    let f = merge_fixture();
    let mut app = app_of(&f);
    app.update = UpdateState::Available(UpdateInfo {
        current: "0.2.1".to_string(),
        latest: "v0.3.0".to_string(),
        url: "https://example.com".to_string(),
        action: UpdateAction::Manual {
            command: "cargo install --git https://github.com/bjo4/gitgraph-tui".to_string(),
        },
    });
    app.handle_key(key(KeyCode::Char('u')));
    let all = render_app(&mut app, 100, 16).join("\n");
    assert!(all.contains("cargo install"));
    assert!(!all.contains("update now"), "no button that would fail");
}

#[test]
fn install_progress_and_completion_render() {
    let f = merge_fixture();
    let mut app = available_app(&f);
    app.handle_key(key(KeyCode::Char('u')));
    app.apply_update_message(UpdateMessage::Step(InstallStep::Verifying));
    assert!(
        render_app(&mut app, 100, 16)
            .join("\n")
            .contains("verifying")
    );
    app.apply_update_message(UpdateMessage::Done {
        latest: "v0.3.0".to_string(),
    });
    let all = render_app(&mut app, 100, 16).join("\n");
    assert!(all.contains("restart"), "the user is told to restart");
}

#[test]
fn a_failed_install_shows_its_reason() {
    let f = merge_fixture();
    let mut app = available_app(&f);
    app.handle_key(key(KeyCode::Char('u')));
    app.apply_update_message(UpdateMessage::Failed {
        message: "checksum mismatch — aborting".to_string(),
    });
    let all = render_app(&mut app, 100, 16).join("\n");
    assert!(all.contains("checksum mismatch"));
}

#[test]
fn the_popup_survives_a_narrow_terminal_without_panicking() {
    let f = merge_fixture();
    let mut app = available_app(&f);
    app.handle_key(key(KeyCode::Char('u')));
    for width in [20u16, 32, 48, 80] {
        let _ = render_app(&mut app, width, 10);
    }
}

#[test]
fn a_wrapping_url_does_not_push_the_dismiss_line_out_of_the_popup() {
    // The URL is wider than the popup's inner width, so the popup has to grow
    // by the wrapped rows. If height is computed before wrapping, "not now"
    // falls outside the border and disappears.
    let f = merge_fixture();
    let mut app = app_of(&f);
    app.update = UpdateState::Available(UpdateInfo {
        current: "0.2.1".to_string(),
        latest: "v0.3.0".to_string(),
        url: "https://github.com/bjo4/gitgraph-tui/releases/tag/v0.3.0-with-a-very-long-suffix"
            .to_string(),
        action: UpdateAction::SelfUpdate,
    });
    app.handle_key(key(KeyCode::Char('u')));
    let all = render_app(&mut app, 100, 24).join("\n");
    assert!(
        all.contains("not now"),
        "the dismiss line was pushed out of the popup"
    );
    assert!(all.contains("update now"));
}

#[test]
fn a_status_message_still_takes_the_whole_help_line_when_an_update_is_pending() {
    // `status` is transient and always wins the line; the update hint returns
    // on the next render once the status clears.
    let f = merge_fixture();
    let mut app = app_of(&f);
    app.update = UpdateState::Available(UpdateInfo {
        current: "0.2.1".to_string(),
        latest: "v0.3.0".to_string(),
        url: "https://example.com".to_string(),
        action: UpdateAction::SelfUpdate,
    });
    app.status = "reloaded".to_string();
    let lines = render_app(&mut app, 100, 16);
    let last = lines.last().unwrap();
    assert!(last.contains("reloaded"));
    assert!(!last.contains("u:update"));
}

#[test]
fn the_help_line_tells_the_three_update_outcomes_apart() {
    // The popup can be dismissed with esc while the install runs, so the help
    // line is where the outcome has to surface. Showing "u:update" after a
    // failure would look identical to "a new version exists".
    let f = merge_fixture();

    let mut app = app_of(&f);
    app.update = UpdateState::Available(UpdateInfo {
        current: "0.2.1".to_string(),
        latest: "v0.3.0".to_string(),
        url: "https://example.com".to_string(),
        action: UpdateAction::SelfUpdate,
    });
    let pending = render_app(&mut app, 100, 16).last().unwrap().clone();
    assert!(pending.contains("u:update"));
    assert!(pending.contains("q:quit"), "quit must stay visible");

    let mut app = app_of(&f);
    app.update = UpdateState::Done {
        latest: "v0.3.0".to_string(),
    };
    let done = render_app(&mut app, 100, 16).last().unwrap().clone();
    assert!(
        done.contains("u:restart"),
        "a finished update must not still say update"
    );
    assert!(!done.contains("u:update"));
    assert!(done.contains("q:quit"));

    let mut app = app_of(&f);
    app.update = UpdateState::Failed {
        message: "checksum mismatch".to_string(),
    };
    let failed = render_app(&mut app, 100, 16).last().unwrap().clone();
    assert!(
        failed.contains("u:failed"),
        "a failed update must be distinguishable"
    );
    assert!(!failed.contains("u:update"));
    assert!(failed.contains("q:quit"));
}
