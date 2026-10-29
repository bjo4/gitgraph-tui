//! Terminal input, decoupled from the main loop for testability.
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event, KeyEvent, KeyEventKind};

/// What one wait on the terminal produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    /// A key press.
    Key(KeyEvent),
    /// Any other event (resize, key release, focus, …): redraw, nothing else.
    Other,
    /// Nothing arrived within the timeout — an idle tick.
    Idle,
}

/// Wait up to `timeout` for the next terminal event.
pub fn poll_input(timeout: Duration) -> Result<Input> {
    if !event::poll(timeout)? {
        return Ok(Input::Idle);
    }
    Ok(match event::read()? {
        Event::Key(k) if k.kind == KeyEventKind::Press => Input::Key(k),
        _ => Input::Other,
    })
}
