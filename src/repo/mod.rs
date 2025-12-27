use crate::domain::todo::{Priority, Todo, TodoId};

pub mod github;
pub mod memory;
pub mod sqlite;

pub trait TodoRepository {
    fn all(&self) -> Vec<Todo>;
    fn add(
        &mut self,
        title: String,
        priority: Priority,
        due: Option<std::time::SystemTime>,
        external_url: Option<String>,
        external_key: Option<String>,
    ) -> Todo;
    fn update_meta(
        &mut self,
        id: TodoId,
        priority: Priority,
        due: Option<std::time::SystemTime>,
    ) -> Option<Todo>;
    fn toggle(&mut self, id: TodoId) -> Option<Todo>;
    fn delete(&mut self, id: TodoId) -> Option<Todo>;
    fn clear_done(&mut self) -> usize;
}
