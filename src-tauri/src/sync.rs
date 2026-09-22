use std::sync::atomic::Ordering;
use std::time::Duration;

use tauri::{AppHandle, Manager};

use crate::api;
use crate::config::Config;
use crate::db;
use crate::error::AppResult;
use crate::state::AppState;

async fn acquire_sync(app: &AppHandle) {
    let flag = &app.state::<AppState>().syncing;
    while flag
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        let _ = tauri::async_runtime::spawn_blocking(|| {
            std::thread::sleep(Duration::from_millis(8));
        })
        .await;
    }
}

fn release_sync(app: &AppHandle) {
    app.state::<AppState>()
        .syncing
        .store(false, Ordering::Release);
}

/// Post each unsynced local tally exactly once. Concurrent callers wait and
/// then drain whatever is still outstanding, so overlapping clicks cannot
/// POST the same row twice.
pub async fn sync_once(app: &AppHandle) -> AppResult<bool> {
    acquire_sync(app).await;
    let result = drain_unsynced(app).await;
    release_sync(app);
    result
}

async fn drain_unsynced(app: &AppHandle) -> AppResult<bool> {
    loop {
        let (base_url, queue) = {
            let state = app.state::<AppState>();
            let inner = state
                .inner
                .lock()
                .map_err(|_| "Internal state lock was poisoned.")?;
            let config = match inner.config.as_ref() {
                Some(config) if config.is_complete() => config.clone(),
                _ => return Ok(inner.online),
            };
            let Some(conn) = inner.db.as_ref() else {
                return Ok(inner.online);
            };
            (config.base_url, db::unsynced(conn)?)
        };

        if queue.is_empty() {
            set_online(app, true, None);
            return Ok(true);
        }

        for item in queue {
            match api::post_tally(
                &base_url,
                item.desk_id,
                item.question_type_id,
                Some(&item.created),
            )
            .await
            {
                Ok(()) => {
                    if let Ok(state) = app.state::<AppState>().inner.lock() {
                        if let Some(conn) = state.db.as_ref() {
                            let _ = db::mark_synced(conn, item.id);
                        }
                    }
                }
                Err(err) => {
                    set_online(app, false, Some(err.to_string()));
                    return Ok(false);
                }
            }
        }
    }
}

pub fn set_online(app: &AppHandle, online: bool, last_error: Option<String>) {
    if let Ok(mut inner) = app.state::<AppState>().inner.lock() {
        inner.online = online;
        inner.last_error = last_error.clone();
    }
    let _ = app.emit_all(
        "connectivity",
        ConnectivityEvent {
            online,
            last_error,
        },
    );
}

#[derive(Clone, serde::Serialize)]
pub struct ConnectivityEvent {
    pub online: bool,
    pub last_error: Option<String>,
}

pub async fn refresh_types(app: &AppHandle, config: &Config) -> AppResult<Vec<db::QuestionType>> {
    match api::fetch_question_types(&config.base_url, config.desk_id).await {
        Ok(types) => {
            if let Ok(mut inner) = app.state::<AppState>().inner.lock() {
                if let Some(conn) = inner.db.as_ref() {
                    let _ = db::replace_question_types(conn, &types);
                }
                inner.question_types = types.clone();
            }
            set_online(app, true, None);
            let _ = sync_once(app).await;
            Ok(types)
        }
        Err(err) => {
            let cached = {
                let state = app.state::<AppState>();
                let inner = state
                    .inner
                    .lock()
                    .map_err(|_| "Internal state lock was poisoned.")?;
                if let Some(conn) = inner.db.as_ref() {
                    db::list_question_types(conn).unwrap_or_default()
                } else {
                    Vec::new()
                }
            };
            set_online(app, false, Some(err.to_string()));
            if cached.is_empty() {
                Err(err)
            } else {
                if let Ok(mut inner) = app.state::<AppState>().inner.lock() {
                    inner.question_types = cached.clone();
                }
                Ok(cached)
            }
        }
    }
}
