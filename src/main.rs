mod app;
mod domain;
mod repo;
mod ui;

use std::time::Duration;

use anyhow::Result;
use clap::Parser;

use app::App;
use domain::todo::Todo;
use repo::memory::InMemoryTodoRepo;

#[derive(Parser, Debug)]
#[command(author, version, about = "koto — minimal needle-style todo TUI", long_about = None)]
struct Args {
    /// Tick interval of render loop in milliseconds
    #[arg(long, default_value_t = 120)]
    tick_ms: u64,

    /// Start with demo tasks
    #[arg(long, default_value_t = false)]
    demo: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let repo = if args.demo {
        InMemoryTodoRepo::with_seed(seed_todos())
    } else {
        InMemoryTodoRepo::default()
    };

    let app = App::new(repo);
    ui::run(app, Duration::from_millis(args.tick_ms))
}

fn seed_todos() -> Vec<Todo> {
    vec![
        Todo::new("Write documentation"),
        Todo::new("Check PRs waiting for review"),
        Todo::new("Draft release notes"),
    ]
}
