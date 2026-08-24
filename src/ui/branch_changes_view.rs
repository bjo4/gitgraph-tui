//! Split-pane branch changes view: changed files on the left, diff on the right.
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph},
};

use crate::app::{App, BranchChangesFocus};
use crate::git::types::ChangeKind;

pub fn render(frame: &mut Frame, area: Rect, app: &mut App) {
    let Some(changes) = app.branch_changes.as_mut() else {
        return;
    };
    let [files_area, diff_area] =
        Layout::horizontal([Constraint::Percentage(40), Constraint::Percentage(60)]).areas(area);

    let rows = file_rows(changes);
    let selected_row = rows
        .iter()
        .position(|row| row.file_index == Some(changes.selected))
        .unwrap_or(0);
    let visible_height = files_area.height.saturating_sub(2) as usize;
    let mut offset = 0;
    if selected_row >= visible_height && visible_height > 0 {
        offset = selected_row + 1 - visible_height;
    }
    let lines: Vec<Line> = rows
        .into_iter()
        .skip(offset)
        .take(visible_height)
        .map(|row| row.line)
        .collect();
    let files_title = format!(" changes {} ", changes.branch_name);
    let files = Paragraph::new(lines).block(Block::bordered().title(files_title));
    frame.render_widget(files, files_area);

    let diff_title = changes
        .entries
        .get(changes.selected)
        .map(|entry| format!(" {} ", entry.file.path.to_string_lossy()))
        .unwrap_or_else(|| " no changes ".to_string());
    changes.diff_viewport_height = diff_area.height.saturating_sub(2) as usize;
    changes.diff_scroll = changes
        .diff_scroll
        .min(changes.diff_lines.len().saturating_sub(changes.diff_viewport_height));
    let lines: Vec<Line> = if changes.diff_lines.is_empty() {
        vec![Line::from(Span::styled(
            "No changed content for this file",
            Style::new().fg(Color::DarkGray),
        ))]
    } else {
        changes
            .diff_lines
            .iter()
            .skip(changes.diff_scroll)
            .take(changes.diff_viewport_height)
            .map(diff_line)
            .collect()
    };
    let para = Paragraph::new(lines).block(Block::bordered().title(diff_title).border_style(
        if changes.focus == BranchChangesFocus::Diff {
            Style::new().fg(Color::White)
        } else {
            Style::new()
        },
    ));
    frame.render_widget(para, diff_area);
}

fn file_line(f: &crate::git::types::FileChange) -> Line<'static> {
    let (letter, color) = match &f.kind {
        ChangeKind::Added => ("A", Color::Green),
        ChangeKind::Modified => ("M", Color::Rgb(255, 165, 0)),
        ChangeKind::Deleted => ("D", Color::Red),
        ChangeKind::Renamed { .. } => ("R", Color::Cyan),
    };
    let mut spans = vec![
        Span::styled(format!(" {letter} "), Style::new().fg(color)),
        Span::raw(f.path.to_string_lossy().into_owned()),
    ];
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

struct FileRow {
    file_index: Option<usize>,
    line: Line<'static>,
}

fn file_rows(changes: &crate::app::BranchChangesState) -> Vec<FileRow> {
    let mut rows = Vec::new();
    push_section(
        &mut rows,
        "Added",
        changes
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.staged),
        changes,
    );
    push_section(
        &mut rows,
        "Not added",
        changes
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| !entry.staged),
        changes,
    );
    rows
}

fn push_section<'a, I>(
    rows: &mut Vec<FileRow>,
    title: &'static str,
    files: I,
    changes: &crate::app::BranchChangesState,
) where
    I: Iterator<Item = (usize, &'a crate::app::BranchChangeEntry)>,
{
    let files: Vec<(usize, &'a crate::app::BranchChangeEntry)> = files.collect();
    rows.push(FileRow {
        file_index: None,
        line: Line::from(Span::styled(
            format!(" {title} "),
            Style::new()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        )),
    });
    if files.is_empty() {
        rows.push(FileRow {
            file_index: None,
            line: Line::from(Span::styled(
                "  (none)",
                Style::new().fg(Color::DarkGray),
            )),
        });
        return;
    }
    for (index, entry) in files {
        let style = if changes.focus == BranchChangesFocus::Files && changes.selected == index {
            Style::new().add_modifier(Modifier::REVERSED)
        } else {
            Style::new()
        };
        rows.push(FileRow {
            file_index: Some(index),
            line: styled_line(file_line(&entry.file), style),
        });
    }
}

fn styled_line(line: Line<'static>, style: Style) -> Line<'static> {
    let spans = line
        .spans
        .into_iter()
        .map(|span| span.patch_style(style))
        .collect::<Vec<_>>();
    Line::from(spans)
}

fn diff_line(l: &crate::git::types::DiffLine) -> Line<'static> {
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
}
