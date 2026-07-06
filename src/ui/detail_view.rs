//! Bottom panel: metadata + changed-file list for the selected row.
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, List, ListItem, ListState, Paragraph},
};

use crate::app::{App, Focus};
use crate::git::types::ChangeKind;
use crate::ui::util::absolute_time;

pub fn render(frame: &mut Frame, area: Rect, app: &mut App) {
    let files = app.current_files();
    let (title, meta) = match app.selected_commit() {
        Some(c) => {
            let mut meta = vec![
                Line::from(Span::styled(
                    c.summary.clone(),
                    Style::new().add_modifier(Modifier::BOLD),
                )),
                Line::from(format!("{} <{}>", c.author_name, c.author_email)),
                Line::from(format!(
                    "{}  ·  {} parent(s)",
                    absolute_time(c.timestamp),
                    c.parents.len()
                )),
            ];
            // Full message body (spec: 完整訊息), capped so the file list
            // below keeps some room.
            let body = c.message.lines().skip(1).skip_while(|l| l.is_empty());
            for line in body.take(4) {
                meta.push(Line::from(line.to_string()));
            }
            (format!(" commit {} ", c.short_id), meta)
        }
        None if !files.is_empty() => (
            " Uncommitted changes ".to_string(),
            vec![Line::from(format!("{} files changed", files.len()))],
        ),
        None => (" commit ".to_string(), vec![Line::from("No commits yet")]),
    };
    let block = Block::bordered().title(title);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let meta_h = (meta.len() as u16).min(inner.height);
    let meta_area = Rect {
        height: meta_h,
        ..inner
    };
    frame.render_widget(Paragraph::new(meta), meta_area);
    let files_area = Rect {
        y: inner.y + meta_h,
        height: inner.height.saturating_sub(meta_h),
        ..inner
    };
    let items: Vec<ListItem> = files.iter().map(|f| ListItem::new(file_line(f))).collect();
    let focused = app.focus == Focus::Files;
    let list = List::new(items).highlight_style(if focused {
        Style::new().add_modifier(Modifier::REVERSED)
    } else {
        Style::new()
    });
    let mut state = ListState::default().with_selected(Some(app.file_selected));
    frame.render_stateful_widget(list, files_area, &mut state);
}

fn file_line(f: &crate::git::types::FileChange) -> Line<'static> {
    let (letter, color) = match &f.kind {
        ChangeKind::Added => ("A", Color::Green),
        ChangeKind::Modified => ("M", Color::Yellow),
        ChangeKind::Deleted => ("D", Color::Red),
        ChangeKind::Renamed { .. } => ("R", Color::Cyan),
    };
    let mut spans = vec![
        Span::styled(format!(" {letter} "), Style::new().fg(color)),
        Span::raw(f.path.clone()),
    ];
    if let ChangeKind::Renamed { from } = &f.kind {
        spans.push(Span::styled(
            format!(" (from {from})"),
            Style::new().fg(Color::DarkGray),
        ));
    }
    if f.is_binary {
        spans.push(Span::styled(" (binary)", Style::new().fg(Color::Magenta)));
    } else {
        spans.push(Span::styled(
            format!("  +{}", f.additions),
            Style::new().fg(Color::Green),
        ));
        spans.push(Span::styled(
            format!(" -{}", f.deletions),
            Style::new().fg(Color::Red),
        ));
    }
    Line::from(spans)
}
