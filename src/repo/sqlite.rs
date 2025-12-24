use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension, Row};
use uuid::Uuid;

use super::TodoRepository;
use crate::domain::todo::{Todo, TodoId};

pub struct SqliteTodoRepo {
    conn: Connection,
}

impl SqliteTodoRepo {
    pub fn open_default() -> Result<Self> {
        let path = default_db_path()?;
        Self::open(path)
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create db dir {}", parent.display()))?;
        }
        let conn =
            Connection::open(path).with_context(|| format!("failed to open db {}", path.display()))?;
        init_schema(&conn)?;
        Ok(Self { conn })
    }
}

impl TodoRepository for SqliteTodoRepo {
    fn all(&self) -> Vec<Todo> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, title, done, created_at FROM todos ORDER BY created_at ASC")
            .expect("failed to prepare select");
        let iter = stmt
            .query_map([], row_to_todo)
            .expect("failed to iterate todos");
        iter.map(|r| r.expect("failed to decode todo")).collect()
    }

    fn add(&mut self, title: String) -> Todo {
        let todo = Todo::new(title);
        self.conn
            .execute(
                "INSERT INTO todos (id, title, done, created_at) VALUES (?1, ?2, ?3, ?4)",
                params![
                    todo.id.to_string(),
                    todo.title,
                    todo.done as i32,
                    to_unix(todo.created_at)
                ],
            )
            .expect("failed to insert todo");
        todo
    }

    fn toggle(&mut self, id: TodoId) -> Option<Todo> {
        let Some(mut todo) = fetch_todo(&self.conn, id) else {
            return None;
        };
        todo.done = !todo.done;
        self.conn
            .execute(
                "UPDATE todos SET done = ?1 WHERE id = ?2",
                params![todo.done as i32, todo.id.to_string()],
            )
            .expect("failed to update todo");
        Some(todo)
    }

    fn delete(&mut self, id: TodoId) -> Option<Todo> {
        let todo = fetch_todo(&self.conn, id)?;
        self.conn
            .execute("DELETE FROM todos WHERE id = ?1", params![id.to_string()])
            .expect("failed to delete todo");
        Some(todo)
    }

    fn clear_done(&mut self) -> usize {
        self.conn
            .execute("DELETE FROM todos WHERE done = 1", [])
            .expect("failed to clear done") as usize
    }
}

fn init_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
PRAGMA journal_mode=WAL;
CREATE TABLE IF NOT EXISTS todos (
  id TEXT PRIMARY KEY,
  title TEXT NOT NULL,
  done INTEGER NOT NULL DEFAULT 0,
  created_at INTEGER NOT NULL
);
"#,
    )
    .context("failed to initialize schema")?;
    Ok(())
}

fn row_to_todo(row: &Row) -> rusqlite::Result<Todo> {
    let id: String = row.get("id")?;
    let created_at: i64 = row.get("created_at")?;
    Ok(Todo {
        id: Uuid::parse_str(&id).unwrap_or_else(|_| Uuid::nil()),
        title: row.get("title")?,
        done: row.get::<_, i32>("done")? != 0,
        created_at: from_unix(created_at),
    })
}

fn fetch_todo(conn: &Connection, id: TodoId) -> Option<Todo> {
    conn.query_row(
        "SELECT id, title, done, created_at FROM todos WHERE id = ?1",
        params![id.to_string()],
        row_to_todo,
    )
    .optional()
    .expect("failed to load todo")
}

fn to_unix(time: SystemTime) -> i64 {
    time.duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn from_unix(secs: i64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(secs as u64)
}

fn default_db_path() -> Result<PathBuf> {
    let base = dirs::data_dir().context("failed to resolve data dir")?;
    Ok(base.join("koto").join("todos.sqlite"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sqlite_repo_round_trip() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let mut repo = SqliteTodoRepo::open(tmp.path()).unwrap();

        let todo = repo.add("hello".to_string());
        assert_eq!(repo.all().len(), 1);

        let toggled = repo.toggle(todo.id).unwrap();
        assert!(toggled.done);

        assert_eq!(repo.clear_done(), 1);
        assert!(repo.all().is_empty());
    }
}
