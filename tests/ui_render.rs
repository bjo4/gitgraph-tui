mod common;

use common::Fixture;
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
    let lines = render_app(&mut app, 90, 16);
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
