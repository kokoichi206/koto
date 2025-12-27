use crate::domain::todo::{Priority, Todo, TodoId};
use crate::repo::TodoRepository;
use crate::repo::github::model::Pr;
use crate::usecase::attention;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration as StdDuration, SystemTime, UNIX_EPOCH};

use time::{Date, Duration, OffsetDateTime, macros::format_description};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Editing,
    EditingDue,
}

pub struct App {
    repo: Box<dyn TodoRepository>,
    pub todos: Vec<Todo>,
    pub selected: usize,
    pub mode: InputMode,
    pub input: String,
    pub status: Option<String>,
    pub github: Option<GithubConfig>,
    pub is_syncing: bool,
    pub sync_rx: Option<Receiver<SyncOutcome>>,
}

#[derive(Debug, Clone)]
pub struct GithubConfig {
    pub token: String,
    pub api_base: Option<String>,
    pub days: u64,
    pub include_team_requests: bool,
}

#[derive(Debug)]
pub struct SyncOutcome {
    pub result: Result<Vec<Pr>, String>,
}

impl App {
    pub fn new(repo: Box<dyn TodoRepository>, github: Option<GithubConfig>) -> Self {
        let todos = repo.all();
        let mut app = Self {
            repo,
            todos,
            selected: 0,
            mode: InputMode::Normal,
            input: String::new(),
            status: None,
            github,
            is_syncing: false,
            sync_rx: None,
        };
        app.sort_todos();
        app
    }

    pub fn reload(&mut self) {
        self.todos = self.repo.all();
        self.sort_todos();
        if self.selected >= self.todos.len() && !self.todos.is_empty() {
            self.selected = self.todos.len() - 1;
        }
    }

    pub fn select_next(&mut self) {
        if !self.todos.is_empty() {
            self.selected = (self.selected + 1).min(self.todos.len() - 1);
        }
    }

    pub fn select_previous(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn cycle_priority_selected(&mut self) {
        let Some(id) = self.selected_id() else { return };
        let current = self.todos[self.selected].priority;
        let next = match current {
            Priority::High => Priority::Medium,
            Priority::Medium => Priority::Low,
            Priority::Low => Priority::High,
        };
        self.repo
            .update_meta(id, next, self.todos[self.selected].due);
        self.reload();
        self.set_status("Priority cycled");
    }

    pub fn shift_due_selected(&mut self, days: i64) {
        let Some(id) = self.selected_id() else { return };
        let current_due = self.todos[self.selected].due;
        let new_due = match current_due {
            Some(ts) => Some(shift_days(ts, days)),
            None => Some(shift_days(SystemTime::now(), days.max(0))), // when none, start from today
        };
        self.repo
            .update_meta(id, self.todos[self.selected].priority, new_due);
        self.reload();
        self.set_status(&format!(
            "Due {} by {}d",
            if days >= 0 { "moved" } else { "moved back" },
            days.abs()
        ));
    }

    pub fn clear_due_selected(&mut self) {
        let Some(id) = self.selected_id() else { return };
        self.repo
            .update_meta(id, self.todos[self.selected].priority, None);
        self.reload();
        self.set_status("Due cleared");
    }

    fn selected_id(&self) -> Option<TodoId> {
        self.todos.get(self.selected).map(|t| t.id)
    }

    pub fn toggle_selected(&mut self) {
        if let Some(id) = self.selected_id() {
            self.repo.toggle(id);
            self.reload();
            self.set_status("Toggled completion");
        }
    }

    pub fn delete_selected(&mut self) {
        if let Some(id) = self.selected_id() {
            self.repo.delete(id);
            if self.selected > 0 {
                self.selected -= 1;
            }
            self.reload();
            self.set_status("Deleted");
        }
    }

    pub fn add_todo(&mut self) {
        let input = self.input.trim();
        if input.is_empty() {
            self.set_status("Cannot add an empty task");
            return;
        }
        let parse = parse_inline_meta(input);
        let (title, priority, due) = match parse {
            Ok(v) => v,
            Err(msg) => {
                self.set_status(&msg);
                return;
            }
        };
        self.repo.add(title, priority, due, None, None);
        self.input.clear();
        self.mode = InputMode::Normal;
        self.reload();
        if !self.todos.is_empty() {
            self.selected = self.todos.len() - 1;
        }
        self.set_status("Added");
    }

    pub fn edit_due(&mut self) {
        self.mode = InputMode::EditingDue;
        self.input.clear();
        self.set_status("Enter due (e.g. d:+3 / today / 2025-01-05)");
    }

    pub fn apply_due_edit(&mut self) {
        let val = self.input.trim();
        if val.is_empty() {
            self.set_status("Input is empty");
            return;
        }
        let Some(id) = self.selected_id() else {
            self.set_status("No task selected");
            return;
        };
        match parse_due_token(val) {
            Ok(Some(due)) => {
                let pri = self.todos[self.selected].priority;
                self.repo.update_meta(id, pri, Some(due));
                self.mode = InputMode::Normal;
                self.input.clear();
                self.reload();
                self.set_status("Due date updated");
            }
            Ok(None) => self.set_status("Could not parse due token"),
            Err(e) => self.set_status(&e),
        }
    }

    pub fn clear_done(&mut self) {
        let removed = self.repo.clear_done();
        self.reload();
        if removed > 0 {
            self.set_status(&format!("Cleared {removed} completed"));
        } else {
            self.set_status("No completed items");
        }
    }

    pub fn set_status(&mut self, msg: &str) {
        self.status = Some(msg.to_string());
    }

    pub fn open_selected_link(&mut self) -> bool {
        let Some(url) = self
            .todos
            .get(self.selected)
            .and_then(|t| t.external_url.as_deref())
        else {
            return false;
        };

        match open::that(url) {
            Ok(_) => self.set_status("Opened link"),
            Err(e) => self.set_status(&format!("Failed to open link: {e}")),
        }
        true
    }

    pub fn start_sync_github(&mut self) {
        let Some(cfg) = self.github.clone() else {
            self.set_status("GitHub sync not configured");
            return;
        };
        if self.is_syncing {
            self.set_status("Sync already in progress");
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.sync_rx = Some(rx);
        self.is_syncing = true;
        self.set_status("Syncing GitHub... (press g again to ignore)");

        thread::spawn(move || {
            let cutoff_ts = crate::now_unix().saturating_sub((cfg.days as i64) * 86_400);
            let res = crate::repo::github::fetch_attention_prs_sync(
                &cfg.token,
                cfg.api_base.clone(),
                cutoff_ts,
                cfg.include_team_requests,
            )
            .map_err(|e| e.to_string());
            let _ = tx.send(SyncOutcome { result: res });
        });
    }

    pub fn poll_sync(&mut self) {
        let Some(rx) = &self.sync_rx else { return };
        match rx.try_recv() {
            Ok(outcome) => {
                self.sync_rx = None;
                self.is_syncing = false;
                match outcome.result {
                    Ok(prs) => {
                        let mut added = 0;
                        for pr in prs {
                            if attention::should_add_todo(&pr) {
                                let title = format!(
                                    "{}/{}#{} by {}: {}",
                                    pr.owner, pr.repo, pr.number, pr.author, pr.title
                                );
                                let (priority, due) = classify_pr_task(&pr);
                                let external_key =
                                    format!("github_pr:{}/{}#{}", pr.owner, pr.repo, pr.number);
                                self.repo.add(
                                    title,
                                    priority,
                                    due,
                                    Some(pr.url.clone()),
                                    Some(external_key),
                                );
                                added += 1;
                            }
                        }
                        self.reload();
                        self.set_status(&format!("Synced GitHub: {added} tasks added"));
                    }
                    Err(e) => {
                        self.set_status(&format!("GitHub sync failed: {e}"));
                    }
                }
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                self.sync_rx = None;
                self.is_syncing = false;
                self.set_status("GitHub sync channel closed");
            }
        }
    }

    fn sort_todos(&mut self) {
        self.todos.sort_by(|a, b| {
            // done items go last
            if a.done != b.done {
                return a.done.cmp(&b.done);
            }
            // earliest due first; None goes last
            match (&a.due, &b.due) {
                (Some(ad), Some(bd)) => {
                    if ad != bd {
                        return ad.cmp(bd);
                    }
                }
                (Some(_), None) => return std::cmp::Ordering::Less,
                (None, Some(_)) => return std::cmp::Ordering::Greater,
                (None, None) => {}
            }
            // priority high(1) < med(2) < low(3)
            if a.priority != b.priority {
                return a.priority.cmp(&b.priority);
            }
            a.created_at.cmp(&b.created_at)
        });
    }
}

fn parse_inline_meta(input: &str) -> Result<(String, Priority, Option<SystemTime>), String> {
    let mut title_parts: Vec<&str> = Vec::new();
    let mut priority = Priority::Medium;
    let mut due: Option<SystemTime> = None;

    for raw in input.split_whitespace() {
        let lower = raw.to_lowercase();
        if let Some(p) = parse_priority_token(&lower) {
            priority = p;
            continue;
        }
        if let Some(d) = parse_due_token(&lower)? {
            due = Some(d);
            continue;
        }
        title_parts.push(raw);
    }

    let title = title_parts.join(" ").trim().to_string();
    if title.is_empty() {
        return Err("Title is empty".into());
    }
    Ok((title, priority, due))
}

fn parse_priority_token(token: &str) -> Option<Priority> {
    match token {
        "p1" | "p:1" | "!" | "high" | "h" | "hi" => Some(Priority::High),
        "p3" | "p:3" | "!!!" | "low" | "l" => Some(Priority::Low),
        "p2" | "p:2" | "!!" | "m" | "med" | "mid" | "medium" => Some(Priority::Medium),
        _ => None,
    }
}

fn parse_due_token(token: &str) -> Result<Option<SystemTime>, String> {
    let token = token
        .strip_prefix("d:")
        .or_else(|| token.strip_prefix("due:"))
        .unwrap_or(token);

    if token == "today" || token == "tod" || token == "t" {
        return Ok(Some(end_of_day(OffsetDateTime::now_utc().date())));
    }
    if token == "tomorrow" || token == "tm" || token == "next" {
        let date = OffsetDateTime::now_utc()
            .date()
            .saturating_add(time::Duration::days(1));
        return Ok(Some(end_of_day(date)));
    }
    if let Some(rest) = token.strip_prefix('+') {
        let days: i64 = rest
            .parse()
            .map_err(|_| "Relative due must be a number (e.g. +3)".to_string())?;
        let date = OffsetDateTime::now_utc()
            .date()
            .saturating_add(time::Duration::days(days));
        return Ok(Some(end_of_day(date)));
    }

    if token.len() == 10 && token.chars().nth(4) == Some('-') {
        let fmt = format_description!("[year]-[month]-[day]");
        let date =
            Date::parse(token, &fmt).map_err(|_| "Use YYYY-MM-DD for due date".to_string())?;
        return Ok(Some(end_of_day(date)));
    }

    Ok(None)
}

fn end_of_day(date: Date) -> SystemTime {
    let dt = date
        .with_hms(23, 59, 59)
        .unwrap_or_else(|_| date.with_hms(0, 0, 0).unwrap());
    let odt = dt.assume_utc();
    let ts = odt.unix_timestamp();
    UNIX_EPOCH + StdDuration::from_secs(ts.max(0) as u64)
}

fn shift_days(time: SystemTime, days: i64) -> SystemTime {
    let odt: OffsetDateTime = time.into();
    let shifted = odt.date().saturating_add(time::Duration::days(days));
    end_of_day(shifted)
}

fn classify_pr_task(pr: &Pr) -> (Priority, Option<SystemTime>) {
    let is_renovate = pr.author.eq_ignore_ascii_case("renovate")
        || pr.author.eq_ignore_ascii_case("renovate-bot")
        || pr.author.eq_ignore_ascii_case("renovate[bot]");
    let today = OffsetDateTime::now_utc().date();
    if is_renovate {
        (
            Priority::Medium,
            Some(end_of_day(today.saturating_add(Duration::days(30)))),
        )
    } else {
        (Priority::High, Some(end_of_day(today)))
    }
}
