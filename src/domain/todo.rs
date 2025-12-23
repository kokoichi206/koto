use serde::{Deserialize, Serialize};
use std::time::SystemTime;
use uuid::Uuid;

pub type TodoId = Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Todo {
    pub id: TodoId,
    pub title: String,
    pub done: bool,
    pub created_at: SystemTime,
}

impl Todo {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            title: title.into(),
            done: false,
            created_at: SystemTime::now(),
        }
    }
}
