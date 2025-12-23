use crate::domain::todo::{Todo, TodoId};

pub mod memory;

pub trait TodoRepository {
    fn all(&self) -> Vec<Todo>;
    fn add(&mut self, title: impl Into<String>) -> Todo;
    fn toggle(&mut self, id: TodoId) -> Option<Todo>;
    fn delete(&mut self, id: TodoId) -> Option<Todo>;
    fn clear_done(&mut self) -> usize;
}
