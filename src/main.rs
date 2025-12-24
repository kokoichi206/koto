mod app;
mod domain;
mod repo;
mod ui;
mod usecase;

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Result, anyhow};
use clap::Parser;

use app::{App, GithubConfig};
use domain::todo::Todo;
use repo::memory::InMemoryTodoRepo;
use repo::sqlite::SqliteTodoRepo;

#[derive(Parser, Debug)]
#[command(author, version, about = "koto — minimal GitHub-aware todo TUI", long_about = None)]
struct Args {
    /// Tick interval of render loop in milliseconds
    #[arg(long, default_value_t = 120)]
    tick_ms: u64,

    /// Start with demo tasks
    #[arg(long, default_value_t = false)]
    demo: bool,

    /// Use in-memory store instead of SQLite
    #[arg(long, default_value_t = false)]
    memory: bool,

    /// Path to SQLite DB file (default: OS data dir)
    #[arg(long)]
    db_path: Option<std::path::PathBuf>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let repo: Box<dyn repo::TodoRepository> = if args.demo {
        Box::new(InMemoryTodoRepo::with_seed(seed_todos()))
    } else if args.memory {
        Box::new(InMemoryTodoRepo::default())
    } else if let Some(path) = args.db_path.as_ref() {
        Box::new(SqliteTodoRepo::open(path)?)
    } else {
        Box::new(SqliteTodoRepo::open_default()?)
    };

    let github_cfg = build_github_config()?;

    let mut app = App::new(repo, github_cfg);
    if app.github.is_some() {
        app.set_status("Press 'g' to sync GitHub PRs");
    }
    ui::run(app, Duration::from_millis(args.tick_ms))
}

fn seed_todos() -> Vec<Todo> {
    vec![
        Todo::new("Write documentation"),
        Todo::new("Check PRs waiting for review"),
        Todo::new("Draft release notes"),
    ]
}

pub fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn github_token() -> Result<String> {
    let raw = std::env::var("GITHUB_TOKEN")
        .map_err(|_| anyhow!("GitHub token is required (env GITHUB_TOKEN)"))?;
    let trimmed = raw.trim().to_string();
    if trimmed.is_empty() {
        return Err(anyhow!(
            "GitHub token is empty after trimming; please re-export"
        ));
    }
    Ok(trimmed)
}

fn build_github_config() -> Result<Option<GithubConfig>> {
    match github_token() {
        Ok(token) => Ok(Some(GithubConfig {
            token,
            api_base: None,
            days: 30,
            include_team_requests: false,
        })),
        Err(_) => Ok(None), // no token in env/flag: operate without GitHub
    }
}
