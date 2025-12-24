use crate::domain::todo::{Todo, TodoId};

pub mod github;
pub mod memory;
pub mod sqlite;

pub trait TodoRepository {
    fn all(&self) -> Vec<Todo>;
    fn add(&mut self, title: String) -> Todo;
    fn toggle(&mut self, id: TodoId) -> Option<Todo>;
    fn delete(&mut self, id: TodoId) -> Option<Todo>;
    fn clear_done(&mut self) -> usize;
}
