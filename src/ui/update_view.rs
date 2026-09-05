//! Centered update popup. Deliberately mirrors `popup.rs` so the two feel
//! like one thing: `Clear`, a bordered block, centered in the frame.
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Clear, Paragraph},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::app::App;
use crate::update::{UpdateAction, UpdateState};

pub fn render(frame: &mut Frame, app: &App) {
    let width = 56.min(frame.area().width);
    let inner_width = width.saturating_sub(2).max(1) as usize;
    // Wrapped ourselves (rather than via `Paragraph::wrap`) so the popup
    // height always matches what actually gets drawn — a long release URL on
    // a narrow terminal must not silently push later lines (e.g. "not now")
    // past the bottom border.
    let styled_lines = restyle(body(app), inner_width);
    let height = (styled_lines.len() as u16 + 2).min(frame.area().height);
    let area = centered(frame.area(), width, height);
    frame.render_widget(Clear, area);
    let paragraph = Paragraph::new(styled_lines).block(Block::bordered().title(" update "));
    frame.render_widget(paragraph, area);
}

/// Wrap each `(text, style)` line to `inner_width` columns, keeping every
/// resulting row in the original line's style.
fn restyle(pieces: Vec<(String, Style)>, inner_width: usize) -> Vec<Line<'static>> {
    pieces
        .into_iter()
        .flat_map(|(text, style)| {
            wrap_text(&text, inner_width)
                .into_iter()
                .map(move |row| Line::styled(row, style))
        })
        .collect()
}

fn body(app: &App) -> Vec<(String, Style)> {
    let dim = Style::new().add_modifier(Modifier::DIM);
    match &app.update {
        UpdateState::Available(info) => {
            let mut lines = vec![
                (
                    format!(" gitgraph-tui {} → {}", info.current, info.latest),
                    Style::new().add_modifier(Modifier::BOLD),
                ),
                (String::new(), Style::default()),
                (format!(" {}", info.url), dim),
                (String::new(), Style::default()),
            ];
            match &info.action {
                UpdateAction::SelfUpdate => {
                    lines.push((" y  update now".to_string(), Style::new().fg(Color::Yellow)));
                    lines.push((" n  not now".to_string(), Style::default()));
                }
                UpdateAction::Manual { command } => {
                    lines.push((" run this to update:".to_string(), dim));
                    lines.push((format!(" {command}"), Style::default()));
                    lines.push((String::new(), Style::default()));
                    lines.push((" esc  close".to_string(), Style::default()));
                }
            }
            lines
        }
        UpdateState::Installing(step) => vec![
            (format!(" {}", step.label()), Style::default()),
            (String::new(), Style::default()),
            (" esc  close (this keeps running)".to_string(), dim),
        ],
        UpdateState::Done { latest } => vec![
            (
                format!(" updated to {latest}"),
                Style::new().fg(Color::Green).add_modifier(Modifier::BOLD),
            ),
            (String::new(), Style::default()),
            (
                " restart gitgraph-tui to use it".to_string(),
                Style::default(),
            ),
            (String::new(), Style::default()),
            (" esc  close".to_string(), dim),
        ],
        UpdateState::Failed { message } => vec![
            (" update failed".to_string(), Style::new().fg(Color::Red)),
            (String::new(), Style::default()),
            (format!(" {message}"), Style::default()),
            (String::new(), Style::default()),
            (" esc  close".to_string(), dim),
        ],
        UpdateState::Idle | UpdateState::Checking => {
            vec![(" checking…".to_string(), Style::default())]
        }
    }
}

/// Greedy word wrap to `width` columns (display width, not byte count).
/// A single "word" wider than `width` (e.g. a long URL) is hard-split
/// character by character rather than overflowing the line. A leading space
/// on `text` is preserved on the first output row so indentation survives.
fn wrap_text(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let leading_space = text.starts_with(' ');
    let trimmed = text.trim_start();
    if trimmed.is_empty() {
        return vec![text.to_string()];
    }
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    for word in trimmed.split(' ').filter(|w| !w.is_empty()) {
        let word_width = UnicodeWidthStr::width(word);
        if word_width > width {
            if !current.is_empty() {
                lines.push(std::mem::take(&mut current));
            }
            let mut piece_width = 0usize;
            for ch in word.chars() {
                let cw = UnicodeWidthChar::width(ch).unwrap_or(1);
                // Only close out the current row once it is non-empty and
                // adding `ch` would overflow it. A character is appended to
                // an *empty* row unconditionally: an indivisible glyph wider
                // than the whole width can't be split any further, so it is
                // emitted alone by design rather than dropped or truncated.
                if !current.is_empty() && piece_width + cw > width {
                    lines.push(std::mem::take(&mut current));
                    piece_width = 0;
                }
                current.push(ch);
                piece_width += cw;
            }
            continue;
        }
        if current.is_empty() {
            current = word.to_string();
        } else if UnicodeWidthStr::width(current.as_str()) + 1 + word_width <= width {
            current.push(' ');
            current.push_str(word);
        } else {
            lines.push(std::mem::take(&mut current));
            current = word.to_string();
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    if leading_space {
        lines[0] = format!(" {}", lines[0]);
    }
    lines
}

/// Same geometry helper as `popup::centered`; kept local so neither popup can
/// change the other's layout by accident.
fn centered(outer: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(outer.width);
    let h = h.min(outer.height);
    Rect {
        x: outer.x + (outer.width - w) / 2,
        y: outer.y + (outer.height - h) / 2,
        width: w,
        height: h,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_line_is_wrapped_to_the_given_width() {
        let wrapped = wrap_text(
            "https://github.com/bjo4/gitgraph-tui/releases/tag/v0.3.0",
            20,
        );
        assert!(
            wrapped.len() > 1,
            "a 56-column URL must not stay on one row"
        );
        for row in &wrapped {
            assert!(
                UnicodeWidthStr::width(row.as_str()) <= 20,
                "row {row:?} exceeds the width it was wrapped to"
            );
        }
    }

    #[test]
    fn an_unbreakable_token_is_hard_split_rather_than_overflowing() {
        let long = "a".repeat(200);
        let wrapped = wrap_text(&long, 10);
        assert_eq!(wrapped.len(), 20);
        for row in &wrapped {
            assert!(UnicodeWidthStr::width(row.as_str()) <= 10);
        }
    }

    #[test]
    fn wide_glyphs_never_produce_a_row_wider_than_asked_for() {
        // Two-column glyphs are where a naive character-at-a-time split
        // overflows: the first character used to be pushed without checking.
        for width in [1usize, 2, 3, 5] {
            for row in wrap_text("測試更新視窗換行", width) {
                let w = UnicodeWidthStr::width(row.as_str());
                assert!(
                    w <= width.max(2),
                    "width {width}: row {row:?} is {w} columns"
                );
            }
        }
    }

    #[test]
    fn degenerate_widths_and_empty_input_do_not_hang_or_panic() {
        assert_eq!(wrap_text("", 10), vec![String::new()]);
        let _ = wrap_text("hello world", 0);
        let _ = wrap_text("hello world", 1);
    }
}
