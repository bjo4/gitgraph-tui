//! Centered branch-filter popup.
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::Line,
    widgets::{Block, Clear, List, ListItem, ListState},
};

use crate::app::App;

pub fn render(frame: &mut Frame, app: &App) {
    let h = (app.filter_choices.len() as u16 + 2).min(15);
    let area = centered(frame.area(), 40, h);
    frame.render_widget(Clear, area);
    let items: Vec<ListItem> = app
        .filter_choices
        .iter()
        .map(|c| {
            let label = match c {
                None => "All branches".to_string(),
                Some(r) => r.name.clone(),
            };
            ListItem::new(Line::from(format!(" {label}")))
        })
        .collect();
    let list = List::new(items)
        .block(Block::bordered().title(" filter by branch "))
        .highlight_style(Style::new().add_modifier(Modifier::REVERSED));
    let mut state = ListState::default().with_selected(Some(app.filter_selected));
    frame.render_stateful_widget(list, area, &mut state);
}

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
