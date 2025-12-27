use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, Row, params};
use uuid::Uuid;

use super::TodoRepository;
use crate::domain::todo::{Priority, Todo, TodoId};

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
        let conn = Connection::open(path)
            .with_context(|| format!("failed to open db {}", path.display()))?;
        init_schema(&conn)?;
        Ok(Self { conn })
    }
}

impl TodoRepository for SqliteTodoRepo {
    fn all(&self) -> Vec<Todo> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, title, done, priority, due, created_at, external_url, external_key FROM todos ORDER BY created_at ASC",
            )
            .expect("failed to prepare select");
        let iter = stmt
            .query_map([], row_to_todo)
            .expect("failed to iterate todos");
        iter.map(|r| r.expect("failed to decode todo")).collect()
    }

    fn add(
        &mut self,
        title: String,
        priority: Priority,
        due: Option<std::time::SystemTime>,
        external_url: Option<String>,
        external_key: Option<String>,
    ) -> Todo {
        if let Some(ref key) = external_key
            && let Some(mut existing) = fetch_todo_by_external_key(&self.conn, key)
        {
            self.conn
                .execute(
                    "UPDATE todos SET title = ?1, external_url = ?2 WHERE id = ?3",
                    params![title, external_url, existing.id.to_string()],
                )
                .expect("failed to update external todo");
            existing.title = title;
            existing.external_url = external_url;
            return existing;
        }

        let mut todo = Todo::with_meta(title, priority, due);
        todo.external_url = external_url;
        todo.external_key = external_key;
        self.conn
            .execute(
                "INSERT INTO todos (id, title, done, priority, due, created_at, external_url, external_key) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    todo.id.to_string(),
                    &todo.title,
                    todo.done as i32,
                    todo.priority as i32,
                    todo.due.map(to_unix),
                    to_unix(todo.created_at),
                    todo.external_url,
                    todo.external_key
                ],
            )
            .expect("failed to insert todo");
        todo
    }

    fn update_meta(
        &mut self,
        id: TodoId,
        priority: Priority,
        due: Option<std::time::SystemTime>,
    ) -> Option<Todo> {
        let mut todo = fetch_todo(&self.conn, id)?;
        todo.priority = priority;
        todo.due = due;
        self.conn
            .execute(
                "UPDATE todos SET priority = ?1, due = ?2 WHERE id = ?3",
                params![priority as i32, todo.due.map(to_unix), todo.id.to_string()],
            )
            .expect("failed to update meta");
        Some(todo)
    }

    fn toggle(&mut self, id: TodoId) -> Option<Todo> {
        let mut todo = fetch_todo(&self.conn, id)?;
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
            .expect("failed to clear done")
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
  priority INTEGER NOT NULL DEFAULT 2,
  due INTEGER NULL,
  created_at INTEGER NOT NULL,
  external_url TEXT NULL,
  external_key TEXT NULL
);
"#,
    )
    .context("failed to initialize schema")?;

    ensure_column(
        conn,
        "priority",
        "ALTER TABLE todos ADD COLUMN priority INTEGER NOT NULL DEFAULT 2",
    )?;
    ensure_column(conn, "due", "ALTER TABLE todos ADD COLUMN due INTEGER NULL")?;
    ensure_column(
        conn,
        "external_url",
        "ALTER TABLE todos ADD COLUMN external_url TEXT NULL",
    )?;
    ensure_column(
        conn,
        "external_key",
        "ALTER TABLE todos ADD COLUMN external_key TEXT NULL",
    )?;

    conn.execute(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_todos_external_key ON todos(external_key)",
        [],
    )
    .context("failed to create external key index")?;
    Ok(())
}

fn row_to_todo(row: &Row) -> rusqlite::Result<Todo> {
    let id: String = row.get("id")?;
    let created_at: i64 = row.get("created_at")?;
    let priority_val: i32 = row.get("priority").unwrap_or(2);
    Ok(Todo {
        id: Uuid::parse_str(&id).unwrap_or_else(|_| Uuid::nil()),
        title: row.get("title")?,
        done: row.get::<_, i32>("done")? != 0,
        priority: Priority::from_level(priority_val as u8),
        due: row
            .get::<_, Option<i64>>("due")
            .unwrap_or(None)
            .map(from_unix),
        created_at: from_unix(created_at),
        external_url: row.get::<_, Option<String>>("external_url").unwrap_or(None),
        external_key: row.get::<_, Option<String>>("external_key").unwrap_or(None),
    })
}

fn fetch_todo(conn: &Connection, id: TodoId) -> Option<Todo> {
    conn.query_row(
        "SELECT id, title, done, priority, due, created_at, external_url, external_key FROM todos WHERE id = ?1",
        params![id.to_string()],
        row_to_todo,
    )
    .optional()
    .expect("failed to load todo")
}

fn fetch_todo_by_external_key(conn: &Connection, external_key: &str) -> Option<Todo> {
    conn.query_row(
        "SELECT id, title, done, priority, due, created_at, external_url, external_key FROM todos WHERE external_key = ?1",
        params![external_key],
        row_to_todo,
    )
    .optional()
    .expect("failed to load todo by external_key")
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

fn ensure_column(conn: &Connection, name: &str, alter_sql: &str) -> Result<()> {
    let mut stmt = conn.prepare("PRAGMA table_info(todos)")?;
    let cols = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if !cols.iter().any(|c| c == name) {
        conn.execute(alter_sql, [])
            .context("failed to add column")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sqlite_repo_round_trip() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let mut repo = SqliteTodoRepo::open(tmp.path()).unwrap();

        let todo = repo.add("hello".to_string(), Priority::Medium, None, None, None);
        assert_eq!(repo.all().len(), 1);

        let toggled = repo.toggle(todo.id).unwrap();
        assert!(toggled.done);

        assert_eq!(repo.clear_done(), 1);
        assert!(repo.all().is_empty());
    }
}
