pub mod detail_view;
pub mod graph_view;
pub mod util;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::Line;

use crate::app::App;
use crate::app::Mode;
use crate::git::types::RefKind;
use crate::graph::layout::PALETTE_SIZE;

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
pub fn render(frame: &mut Frame, app: &mut App) {
    let [main_area, detail_area, help_area] = Layout::vertical([
        Constraint::Percentage(70),
        Constraint::Percentage(30),
        Constraint::Length(1),
    ])
    .areas(frame.area());
    graph_view::render(frame, main_area, app);
    detail_view::render(frame, detail_area, app);
    render_help(frame, help_area, app);
}

fn render_help(frame: &mut Frame, area: Rect, app: &App) {
    let text = match app.mode {
        Mode::Search => format!(" /{}▌  enter:confirm  esc:cancel", app.search.input),
        Mode::Diff => " j/k:scroll  g/G:top/bottom  esc:back".to_string(),
        Mode::BranchFilter => " j/k:choose  enter:apply  esc:close".to_string(),
        Mode::Normal if !app.status.is_empty() => format!(" {}", app.status),
        Mode::Normal => {
            " j/k:move g/G:top/bot tab:focus enter:diff /:search n/N:next b:branches r:reload q:quit"
                .to_string()
        }
    };
    frame.render_widget(Line::from(text.dim()), area);
}
