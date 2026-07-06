//! The main graph + commit list.
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, List, ListItem},
};
use unicode_width::UnicodeWidthStr;

use crate::app::App;
use crate::ui::util::{pad_to_width, relative_time, truncate_width};
use crate::ui::{lane_color, ref_style};

const AUTHOR_W: usize = 12;
const TIME_W: usize = 5;
/// Graph column cap: beyond 16 lanes the graph is unreadable anyway.
const MAX_GRAPH_W: usize = 32;

pub fn render(frame: &mut Frame, area: Rect, app: &mut App) {
    let graph_w = graph_width(app);
    let text_w = (area.width as usize).saturating_sub(2 + graph_w + 1); // borders + gap
    let items: Vec<ListItem> = (0..app.display_len())
        .map(|i| ListItem::new(row_line(app, i, graph_w, text_w)))
        .collect();
    let filter = match &app.branch_filter {
        Some(r) => r.name.clone(),
        None => "all branches".to_string(),
    };
    let title = format!(
        " {} — {} — {}/{} commits ",
        app.repo_name,
        filter,
        app.display_len(),
        app.total_len()
    );
    let list = List::new(items)
        .block(Block::bordered().title(title))
        .highlight_style(Style::new().add_modifier(Modifier::REVERSED));
    frame.render_stateful_widget(list, area, &mut app.list_state);
}

fn graph_width(app: &App) -> usize {
    app.rows
        .iter()
        .map(|r| r.cells.len())
        .max()
        .unwrap_or(2)
        .clamp(2, MAX_GRAPH_W)
}

fn row_line(app: &App, i: usize, graph_w: usize, text_w: usize) -> Line<'static> {
    let off = app.uncommitted_offset();
    if off == 1 && i == 0 {
        let style = Style::new().fg(Color::Yellow);
        return Line::from(vec![
            Span::styled(pad_to_width("●", graph_w), style),
            Span::raw(" "),
            Span::styled(
                format!("Uncommitted changes ({} files)", app.uncommitted.len()),
                style,
            ),
        ]);
    }
    let idx = i - off;
    let commit = &app.commits[idx];
    let row = &app.rows[idx];
    let mut spans: Vec<Span> = row
        .cells
        .iter()
        .take(graph_w)
        .map(|c| Span::styled(c.glyph.to_string(), Style::new().fg(lane_color(c.color))))
        .collect();
    if row.cells.len() < graph_w {
        spans.push(Span::raw(" ".repeat(graph_w - row.cells.len())));
    }
    spans.push(Span::raw(" "));
    // Ref labels, then the summary in whatever width is left.
    let mut left = text_w.saturating_sub(AUTHOR_W + 1 + TIME_W);
    if let Some(refs) = app.ref_map.get(&commit.id) {
        for r in refs {
            let label = format!("[{}] ", r.name);
            if label.width() > left {
                break;
            }
            left -= label.width();
            spans.push(Span::styled(label, ref_style(r.kind)));
        }
    }
    let summary = truncate_width(&commit.summary, left.saturating_sub(1));
    let pad = left.saturating_sub(summary.width());
    let sum_style = if app.search.matches.contains(&i) {
        Style::new().add_modifier(Modifier::UNDERLINED)
    } else {
        Style::new()
    };
    spans.push(Span::styled(summary, sum_style));
    spans.push(Span::raw(" ".repeat(pad)));
    let dim = Style::new().fg(Color::DarkGray);
    spans.push(Span::styled(
        pad_to_width(&truncate_width(&commit.author_name, AUTHOR_W), AUTHOR_W),
        dim,
    ));
    spans.push(Span::raw(" "));
    spans.push(Span::styled(relative_time(commit.timestamp, app.now), dim));
    Line::from(spans)
}
