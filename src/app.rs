use crate::domain::todo::{Todo, TodoId};
use crate::repo::TodoRepository;
use crate::repo::github::model::Pr;
use crate::usecase::attention;
use std::sync::mpsc::{self, Receiver};
use std::thread;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Editing,
}

pub struct App<R: TodoRepository> {
    repo: R,
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

impl<R: TodoRepository> App<R> {
    pub fn new(repo: R, github: Option<GithubConfig>) -> Self {
        let todos = repo.all();
        Self {
            repo,
            todos,
            selected: 0,
            mode: InputMode::Normal,
            input: String::new(),
            status: None,
            github,
            is_syncing: false,
            sync_rx: None,
        }
    }

    pub fn reload(&mut self) {
        self.todos = self.repo.all();
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
        if self.input.trim().is_empty() {
            self.set_status("Cannot add an empty task");
            return;
        }
        let title = self.input.trim().to_owned();
        self.repo.add(title);
        self.input.clear();
        self.mode = InputMode::Normal;
        self.reload();
        if !self.todos.is_empty() {
            self.selected = self.todos.len() - 1;
        }
        self.set_status("Added");
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
                                self.repo.add(title);
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
}
