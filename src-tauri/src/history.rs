use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection};
use serde::Serialize;

use crate::error::AppResult;

/// How many entries to keep; older ones are pruned on insert.
const MAX_ENTRIES: i64 = 1000;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryItem {
    pub id: i64,
    /// Unix time in milliseconds.
    pub created_at: i64,
    pub source_text: String,
    pub result_text: String,
    pub source_lang: Option<String>,
    pub target_lang: String,
    pub model: String,
}

pub struct NewEntry<'a> {
    pub source_text: &'a str,
    pub result_text: &'a str,
    pub source_lang: Option<&'a str>,
    pub target_lang: &'a str,
    pub model: &'a str,
}

pub struct History {
    conn: Mutex<Connection>,
}

impl History {
    pub fn open(path: &Path) -> AppResult<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        Self::init(Connection::open(path)?)
    }

    #[cfg(test)]
    pub fn open_in_memory() -> AppResult<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> AppResult<Self> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS history (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                created_at  INTEGER NOT NULL,
                source_text TEXT NOT NULL,
                result_text TEXT NOT NULL,
                source_lang TEXT,
                target_lang TEXT NOT NULL,
                model       TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS history_created_at ON history (created_at);",
        )?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn add(&self, entry: NewEntry) -> AppResult<i64> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO history (created_at, source_text, result_text, source_lang, target_lang, model)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                now_millis(),
                entry.source_text,
                entry.result_text,
                entry.source_lang,
                entry.target_lang,
                entry.model
            ],
        )?;
        let id = conn.last_insert_rowid();
        conn.execute(
            "DELETE FROM history WHERE id NOT IN
                (SELECT id FROM history ORDER BY id DESC LIMIT ?1)",
            [MAX_ENTRIES],
        )?;
        Ok(id)
    }

    /// Newest first. `query` matches the source or result text.
    pub fn list(
        &self,
        query: Option<&str>,
        limit: u32,
        offset: u32,
    ) -> AppResult<Vec<HistoryItem>> {
        let pattern = match query.map(str::trim) {
            Some(q) if !q.is_empty() => format!("%{}%", escape_like(q)),
            _ => "%".to_string(),
        };
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, created_at, source_text, result_text, source_lang, target_lang, model
             FROM history
             WHERE source_text LIKE ?1 ESCAPE '\\' OR result_text LIKE ?1 ESCAPE '\\'
             ORDER BY id DESC LIMIT ?2 OFFSET ?3",
        )?;
        let rows = stmt.query_map(params![pattern, limit, offset], |row| {
            Ok(HistoryItem {
                id: row.get(0)?,
                created_at: row.get(1)?,
                source_text: row.get(2)?,
                result_text: row.get(3)?,
                source_lang: row.get(4)?,
                target_lang: row.get(5)?,
                model: row.get(6)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn delete(&self, id: i64) -> AppResult<()> {
        self.conn
            .lock()
            .unwrap()
            .execute("DELETE FROM history WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn clear(&self) -> AppResult<()> {
        self.conn
            .lock()
            .unwrap()
            .execute("DELETE FROM history", [])?;
        Ok(())
    }
}

fn escape_like(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(source: &str) -> NewEntry<'_> {
        NewEntry {
            source_text: source,
            result_text: "result",
            source_lang: Some("en"),
            target_lang: "zh-CN",
            model: "m",
        }
    }

    #[test]
    fn add_list_search_delete_clear() {
        let history = History::open_in_memory().unwrap();
        let first = history.add(entry("hello world")).unwrap();
        history.add(entry("100% sure")).unwrap();
        history.add(entry("good_bye")).unwrap();

        let all = history.list(None, 10, 0).unwrap();
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].source_text, "good_bye", "newest first");

        assert_eq!(history.list(Some("WORLD"), 10, 0).unwrap().len(), 1);
        // LIKE wildcards in the query are matched literally.
        assert_eq!(history.list(Some("%"), 10, 0).unwrap().len(), 1);
        assert_eq!(history.list(Some("_"), 10, 0).unwrap().len(), 1);
        assert_eq!(
            history.list(None, 1, 1).unwrap()[0].source_text,
            "100% sure"
        );

        history.delete(first).unwrap();
        assert_eq!(history.list(None, 10, 0).unwrap().len(), 2);
        history.clear().unwrap();
        assert!(history.list(None, 10, 0).unwrap().is_empty());
    }

    #[test]
    fn prunes_old_entries() {
        let history = History::open_in_memory().unwrap();
        for i in 0..MAX_ENTRIES + 5 {
            history.add(entry(&i.to_string())).unwrap();
        }
        let items = history.list(None, 2000, 0).unwrap();
        assert_eq!(items.len() as i64, MAX_ENTRIES);
        assert_eq!(items.last().unwrap().source_text, "5");
    }
}
