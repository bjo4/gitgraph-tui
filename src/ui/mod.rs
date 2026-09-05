pub mod branch_changes_view;
pub mod detail_view;
pub mod diff_view;
pub mod graph_view;
pub mod popup;
pub mod update_view;
pub mod util;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};

use crate::app::App;
use crate::app::Mode;
use crate::git::types::RefKind;
use crate::graph::layout::PALETTE_SIZE;
use crate::update::UpdateState;

/// Lane palette; index comes from the layout engine (0..PALETTE_SIZE).
pub const PALETTE: [Color; PALETTE_SIZE] = [
    Color::Cyan,
    Color::Magenta,
    Color::Blue,
    Color::Green,
    Color::Yellow,
    Color::Red,
    Color::LightCyan,
    Color::LightMagenta,
];

pub fn lane_color(idx: usize) -> Color {
    PALETTE[idx % PALETTE.len()]
}

pub fn ref_style(kind: RefKind) -> Style {
    match kind {
        RefKind::Head => Style::new()
            .fg(Color::LightGreen)
            .add_modifier(Modifier::BOLD),
        RefKind::LocalBranch => Style::new().fg(Color::Green),
        RefKind::RemoteBranch => Style::new().fg(Color::Blue),
        RefKind::Tag => Style::new().fg(Color::Yellow),
    }
}

/// Top-level draw: graph (70%) / detail (30%) / one help line.
/// In `Mode::Diff` the main area is fully replaced by the full-screen diff.
pub fn render(frame: &mut Frame, app: &mut App) {
    let [main_area, help_area] =
        Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(frame.area());
    if app.mode == Mode::Diff {
        diff_view::render(frame, main_area, app);
        render_help(frame, help_area, app);
        return;
    }
    if app.mode == Mode::BranchChanges {
        branch_changes_view::render(frame, main_area, app);
        render_help(frame, help_area, app);
        return;
    }
    let [graph_area, detail_area] =
        Layout::vertical([Constraint::Percentage(70), Constraint::Percentage(30)]).areas(main_area);
    graph_view::render(frame, graph_area, app);
    detail_view::render(frame, detail_area, app);
    render_help(frame, help_area, app);
    if app.mode == Mode::BranchFilter {
        popup::render(frame, app);
    }
    if app.mode == Mode::Update {
        update_view::render(frame, app);
    }
}

/// Normal-mode bindings when nothing is pending. 97 columns; the render tests
/// assert `q:quit` stays visible at width 100, so this line has almost no
/// slack — see commit bc6221f.
const NORMAL_HELP: &str = " j/k:move g/G:top/bot tab:focus enter:diff /:search n/N:next \
b:branches c:changes r:reload q:quit";

/// The same line with `g/G:top/bot` giving up its slot to `u:update`. 94
/// columns — three *fewer* than the idle line, so it can never push `q:quit`
/// off the edge. `g`/`G` still work; they are just not advertised here, and
/// the Diff and BranchChanges help lines still name them.
const UPDATE_HELP_HEAD: &str = " j/k:move tab:focus enter:diff /:search n/N:next \
b:branches c:changes r:reload ";
const UPDATE_HELP_TAIL: &str = " q:quit";

/// The key hint and style for the non-idle update states. `esc` closes the
/// popup but the background work (and its outcome) keeps going, so this is
/// the only place the result of an install ever surfaces — see spec §6.2.
fn update_help_key(state: &UpdateState) -> (&'static str, Style) {
    match state {
        UpdateState::Done { .. } => (
            "u:restart",
            Style::new().fg(Color::Green).add_modifier(Modifier::BOLD),
        ),
        UpdateState::Failed { .. } => (
            "u:failed",
            Style::new().fg(Color::Red).add_modifier(Modifier::BOLD),
        ),
        _ => (
            "u:update",
            Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        ),
    }
}

fn render_help(frame: &mut Frame, area: Rect, app: &App) {
    // An available update is the only case that needs more than one style, so
    // it is handled first and everything else stays a single dim line.
    if app.mode == Mode::Normal
        && app.status.is_empty()
        && !matches!(app.update, UpdateState::Idle | UpdateState::Checking)
    {
        let (key, style) = update_help_key(&app.update);
        let line = Line::from(vec![
            Span::from(UPDATE_HELP_HEAD).dim(),
            // Undimmed: the signal that something is new costs no columns.
            Span::styled(key, style),
            Span::from(UPDATE_HELP_TAIL).dim(),
        ]);
        frame.render_widget(line, area);
        return;
    }
    let text = match app.mode {
        Mode::Search => format!(" /{}▌  enter:confirm  esc:cancel", app.search.input),
        Mode::Diff => " j/k:scroll  g/G:top/bottom  esc:back".to_string(),
        Mode::BranchFilter => " j/k:choose  enter:apply  esc:close".to_string(),
        Mode::BranchChanges => " j/k:move  tab:focus  g/G:top/bottom  esc:back".to_string(),
        Mode::Update => " y:update  n/esc:close".to_string(),
        Mode::Normal if !app.status.is_empty() => format!(" {}", app.status),
        Mode::Normal => NORMAL_HELP.to_string(),
    };
    frame.render_widget(Line::from(text.dim()), area);
}
