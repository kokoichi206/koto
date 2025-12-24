use std::collections::VecDeque;

use super::TodoRepository;
use crate::domain::todo::{Priority, Todo, TodoId};

#[derive(Default)]
pub struct InMemoryTodoRepo {
    items: VecDeque<Todo>,
}

impl InMemoryTodoRepo {
    pub fn with_seed(seed: impl IntoIterator<Item = Todo>) -> Self {
        let mut repo = Self::default();
        repo.items.extend(seed);
        repo
    }
}

impl TodoRepository for InMemoryTodoRepo {
    fn all(&self) -> Vec<Todo> {
        self.items.iter().cloned().collect()
    }

    fn add(
        &mut self,
        title: String,
        priority: Priority,
        due: Option<std::time::SystemTime>,
    ) -> Todo {
        let todo = Todo::with_meta(title, priority, due);
        self.items.push_back(todo.clone());
        todo
    }

    fn update_meta(
        &mut self,
        id: TodoId,
        priority: Priority,
        due: Option<std::time::SystemTime>,
    ) -> Option<Todo> {
        for todo in &mut self.items {
            if todo.id == id {
                todo.priority = priority;
                todo.due = due;
                return Some(todo.clone());
            }
        }
        None
    }

    fn toggle(&mut self, id: TodoId) -> Option<Todo> {
        for todo in &mut self.items {
            if todo.id == id {
                todo.done = !todo.done;
                return Some(todo.clone());
            }
        }
        None
    }

    fn delete(&mut self, id: TodoId) -> Option<Todo> {
        if let Some(pos) = self.items.iter().position(|t| t.id == id) {
            return self.items.remove(pos);
        }
        None
    }

    fn clear_done(&mut self) -> usize {
        let before = self.items.len();
        self.items.retain(|t| !t.done);
        before - self.items.len()
    }
}
