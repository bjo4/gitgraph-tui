use clap::Parser;

/// A read-only git history graph viewer for the terminal.
#[derive(Parser)]
#[command(name = "gitgraph-tui", version, about)]
struct Cli {
    /// Path to a git repository (defaults to the current directory)
    path: Option<std::path::PathBuf>,
}

fn main() {
    let _cli = Cli::parse();
    println!("gitgraph-tui: TUI not yet implemented");
}
