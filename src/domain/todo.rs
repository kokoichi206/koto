use serde::{Deserialize, Serialize};
use std::time::SystemTime;
use uuid::Uuid;

pub type TodoId = Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    High = 1,
    Medium = 2,
    Low = 3,
}

impl Priority {
    pub fn from_level(level: u8) -> Self {
        match level {
            1 => Priority::High,
            3 => Priority::Low,
            _ => Priority::Medium,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Todo {
    pub id: TodoId,
    pub title: String,
    pub done: bool,
    pub priority: Priority,
    pub due: Option<SystemTime>,
    pub created_at: SystemTime,
}

impl Todo {
    pub fn new(title: impl Into<String>) -> Self {
        Self::with_meta(title, Priority::Medium, None)
    }

    pub fn with_meta(
        title: impl Into<String>,
        priority: Priority,
        due: Option<SystemTime>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            title: title.into(),
            done: false,
            priority,
            due,
            created_at: SystemTime::now(),
        }
    }
}
