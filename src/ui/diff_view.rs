//! Full-screen single-file diff.
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph},
};

use crate::app::App;

pub fn render(frame: &mut Frame, area: Rect, app: &mut App) {
    let Some(diff) = app.diff.as_mut() else {
        return;
    };
    diff.viewport_height = area.height.saturating_sub(2) as usize;
    diff.scroll = diff
        .scroll
        .min(diff.lines.len().saturating_sub(diff.viewport_height));
    let lines: Vec<Line> = diff
        .lines
        .iter()
        .skip(diff.scroll)
        .take(diff.viewport_height)
        .map(|l| {
            let style = match l.origin {
                '+' => Style::new().fg(Color::Green),
                '-' => Style::new().fg(Color::Red),
                '@' => Style::new().fg(Color::Cyan),
                'B' => Style::new()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::ITALIC),
                _ => Style::new(),
            };
            let prefix = if matches!(l.origin, '@' | 'B' | '\\') {
                String::new()
            } else {
                l.origin.to_string()
            };
            Line::from(Span::styled(format!("{prefix}{}", l.content), style))
        })
        .collect();
    let para = Paragraph::new(lines).block(Block::bordered().title(format!(" {} ", diff.title)));
    frame.render_widget(para, area);
}
