use crate::domain::todo::{Todo, TodoId};
use crate::repo::TodoRepository;

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
}

impl<R: TodoRepository> App<R> {
    pub fn new(repo: R) -> Self {
        let todos = repo.all();
        Self {
            repo,
            todos,
            selected: 0,
            mode: InputMode::Normal,
            input: String::new(),
            status: None,
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
}
