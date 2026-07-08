use std::process::ExitCode;
use std::time::Duration;

use clap::Parser;
use gitgraph_tui::{app::App, event, git::GitRepo, ui};
use ratatui::DefaultTerminal;

/// A read-only git history graph viewer for the terminal.
#[derive(Parser)]
#[command(name = "gitgraph-tui", version, about)]
struct Cli {
    /// Path to a git repository (defaults to the current directory)
    path: Option<std::path::PathBuf>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let path = cli.path.unwrap_or_else(|| ".".into());
    // Fail before touching the terminal so the error stays readable.
    let app = match GitRepo::discover(&path).and_then(App::new) {
        Ok(app) => app,
        Err(e) => {
            eprintln!("gitgraph-tui: {e:#}");
            return ExitCode::FAILURE;
        }
    };
    let terminal = ratatui::init(); // installs a panic hook that restores the terminal
    let result = run(terminal, app);
    ratatui::restore();
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("gitgraph-tui: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(mut terminal: DefaultTerminal, mut app: App) -> anyhow::Result<()> {
    while !app.should_quit {
        terminal.draw(|frame| ui::render(frame, &mut app))?;
        // A key press is handled; an idle timeout drives auto-refresh, so an
        // external git change (or an unsaved edit) shows up without a keystroke.
        match event::poll_key(Duration::from_millis(250))? {
            Some(key) => app.handle_key(key),
            None => app.on_tick(),
        }
    }
    Ok(())
}
