mod app;
mod domain;
mod repo;
mod ui;
mod usecase;

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Result, anyhow};
use clap::Parser;

use app::{App, GithubConfig};
use domain::todo::{Priority, Todo};
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
    let now = std::time::SystemTime::now();
    let days_from_now = |d: u64| {
        now.checked_add(Duration::from_secs(d * 86_400))
            .unwrap_or(now)
    };

    vec![
        Todo::with_meta("Hotfix production error", Priority::High, Some(now)),
        Todo::with_meta("Update API spec", Priority::Medium, Some(days_from_now(3))),
        Todo::with_meta("Draft release notes", Priority::Low, Some(days_from_now(7))),
        Todo::with_meta("Refactor backlog grooming", Priority::Low, None),
        Todo::with_meta(
            "Prepare onboarding deck",
            Priority::Medium,
            Some(days_from_now(14)),
        ),
        Todo::with_meta(
            "Security audit follow-up",
            Priority::High,
            Some(days_from_now(2)),
        ),
    ]
}

pub fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn github_token() -> Result<String> {
    repo::github::auth::resolve_github_token_env_then_gh().map_err(|e| {
        anyhow!(
            "GitHub token is required (env GITHUB_TOKEN, or `gh auth login` then `gh auth token`): {e}"
        )
    })
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
