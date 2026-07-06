pub mod util;

use ratatui::style::{Color, Modifier, Style};

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
