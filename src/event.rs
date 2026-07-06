//! Terminal input, decoupled from the main loop for testability.
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event, KeyEvent, KeyEventKind};

/// Next key press within `timeout`; None doubles as a redraw tick.
pub fn poll_key(timeout: Duration) -> Result<Option<KeyEvent>> {
    if event::poll(timeout)?
        && let Event::Key(k) = event::read()?
        && k.kind == KeyEventKind::Press
    {
        return Ok(Some(k));
    }
    Ok(None)
}
