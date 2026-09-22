use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Mutex;

use rusqlite::Connection;

use crate::config::Config;
use crate::db::{self, Desk, QuestionType, Stats};

pub struct InnerState {
    pub config: Option<Config>,
    pub db: Option<Connection>,
    pub online: bool,
    pub last_error: Option<String>,
    pub question_types: Vec<QuestionType>,
    pub desks: Vec<Desk>,
    pub pending_storage: Option<PathBuf>,
}

impl InnerState {
    pub fn stats(&self) -> Stats {
        self.db
            .as_ref()
            .and_then(|conn| db::stats(conn).ok())
            .unwrap_or_else(db::empty_stats)
    }
}

pub struct AppState {
    pub inner: Mutex<InnerState>,
    pub syncing: AtomicBool,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(InnerState {
                config: None,
                db: None,
                online: false,
                last_error: None,
                question_types: Vec::new(),
                desks: Vec::new(),
                pending_storage: None,
            }),
            syncing: AtomicBool::new(false),
        }
    }
}
