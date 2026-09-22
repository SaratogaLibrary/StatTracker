use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::error::AppResult;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Desk {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct QuestionType {
    pub id: i64,
    pub name: String,
    pub description: String,
    pub sort_order: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Stats {
    pub last_hour: i64,
    pub today: i64,
    pub week: i64,
    pub month: i64,
    pub year: i64,
}

#[derive(Debug, Clone)]
pub struct QueuedTally {
    pub id: i64,
    pub desk_id: i64,
    pub question_type_id: i64,
    pub created: String,
}

pub fn open(path: &std::path::Path) -> AppResult<Connection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(path)?;
    conn.execute_batch(
        r#"
        PRAGMA foreign_keys = ON;
        CREATE TABLE IF NOT EXISTS question_tallies (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            created TEXT NOT NULL DEFAULT (DATETIME('now', 'localtime')),
            desk_id INTEGER NOT NULL,
            question_type_id INTEGER NOT NULL,
            synced INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE IF NOT EXISTS desks (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS question_types (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            description TEXT,
            sort_order INTEGER
        );
        "#,
    )?;
    Ok(conn)
}

pub fn replace_desks(conn: &Connection, desks: &[Desk]) -> AppResult<()> {
    conn.execute("DELETE FROM desks", [])?;
    let mut stmt = conn.prepare("INSERT INTO desks (id, name) VALUES (?1, ?2)")?;
    for desk in desks {
        stmt.execute(params![desk.id, desk.name])?;
    }
    Ok(())
}

pub fn list_desks(conn: &Connection) -> AppResult<Vec<Desk>> {
    let mut stmt = conn.prepare("SELECT id, name FROM desks ORDER BY name ASC")?;
    let rows = stmt.query_map([], |row| {
        Ok(Desk {
            id: row.get(0)?,
            name: row.get(1)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

pub fn replace_question_types(conn: &Connection, types: &[QuestionType]) -> AppResult<()> {
    conn.execute("DELETE FROM question_types", [])?;
    let mut stmt = conn.prepare(
        "INSERT INTO question_types (id, name, description, sort_order) VALUES (?1, ?2, ?3, ?4)",
    )?;
    for (index, item) in types.iter().enumerate() {
        stmt.execute(params![
            item.id,
            item.name,
            item.description,
            if item.sort_order == 0 {
                index as i64
            } else {
                item.sort_order
            }
        ])?;
    }
    Ok(())
}

pub fn list_question_types(conn: &Connection) -> AppResult<Vec<QuestionType>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, COALESCE(description, ''), COALESCE(sort_order, 0)
         FROM question_types
         ORDER BY sort_order ASC, name ASC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(QuestionType {
            id: row.get(0)?,
            name: row.get(1)?,
            description: row.get(2)?,
            sort_order: row.get(3)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

pub fn insert_tally(conn: &Connection, desk_id: i64, question_type_id: i64) -> AppResult<QueuedTally> {
    conn.execute(
        "INSERT INTO question_tallies (desk_id, question_type_id, created, synced)
         VALUES (?1, ?2, DATETIME('now', 'localtime'), 0)",
        params![desk_id, question_type_id],
    )?;
    let id = conn.last_insert_rowid();
    let created: String = conn.query_row(
        "SELECT created FROM question_tallies WHERE id = ?1",
        params![id],
        |row| row.get(0),
    )?;
    Ok(QueuedTally {
        id,
        desk_id,
        question_type_id,
        created,
    })
}

pub fn mark_synced(conn: &Connection, id: i64) -> AppResult<()> {
    conn.execute(
        "UPDATE question_tallies SET synced = 1 WHERE id = ?1",
        params![id],
    )?;
    Ok(())
}

pub fn unsynced(conn: &Connection) -> AppResult<Vec<QueuedTally>> {
    let mut stmt = conn.prepare(
        "SELECT id, desk_id, question_type_id, created
         FROM question_tallies
         WHERE synced = 0
         ORDER BY id ASC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(QueuedTally {
            id: row.get(0)?,
            desk_id: row.get(1)?,
            question_type_id: row.get(2)?,
            created: row.get(3)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

pub fn stats(conn: &Connection) -> AppResult<Stats> {
    let count = |clause: &str| -> AppResult<i64> {
        let sql = format!("SELECT COUNT(*) FROM question_tallies WHERE {clause}");
        Ok(conn.query_row(&sql, [], |row| row.get(0))?)
    };
    Ok(Stats {
        last_hour: count("created >= datetime('now', 'localtime', '-1 hour')")?,
        today: count("date(created) = date('now', 'localtime')")?,
        week: count("created >= datetime('now', 'localtime', '-7 days')")?,
        month: count("created >= datetime('now', 'localtime', '-1 month')")?,
        year: count("created >= datetime('now', 'localtime', '-1 year')")?,
    })
}

pub fn empty_stats() -> Stats {
    Stats {
        last_hour: 0,
        today: 0,
        week: 0,
        month: 0,
        year: 0,
    }
}

