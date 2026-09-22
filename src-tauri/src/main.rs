#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod api;
mod commands;
mod config;
mod db;
mod error;
mod state;
mod sync;

use std::path::PathBuf;
use std::time::Duration;

use tauri::Manager;

use commands::open_db_for;
use config::Config;
use state::AppState;

fn main() {
    tauri::Builder::default()
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::get_bootstrap,
            commands::get_default_storage,
            commands::pick_directory,
            commands::fetch_desks,
            commands::save_config,
            commands::refresh_question_types,
            commands::cycle_template,
            commands::record_tally,
            commands::get_stats,
            commands::open_settings,
            commands::open_help,
            commands::set_widget_size,
            commands::widget_outer_rect,
            commands::place_widget,
            commands::set_always_on_top,
            commands::check_updates,
            commands::install_update,
            commands::close_widget,
            commands::show_widget,
            commands::minimize_widget,
        ])
        .setup(|app| {
            let handle = app.handle();
            match config::load_config(&handle) {
                Ok(Some(config)) if config.is_complete() => {
                    let _ = config::ensure_user_templates(
                        &handle,
                        PathBuf::from(&config.storage_directory).as_path(),
                    );
                    if let Ok(conn) = open_db_for(&config) {
                        let cached = db::list_question_types(&conn).unwrap_or_default();
                        let desks = db::list_desks(&conn).unwrap_or_default();
                        if let Ok(mut inner) = handle.state::<AppState>().inner.lock() {
                            inner.config = Some(config.clone());
                            inner.db = Some(conn);
                            inner.question_types = cached;
                            inner.desks = desks;
                        }
                    }
                    let _ = commands::apply_autostart(&handle, config.autostart);
                    let startup = handle.clone();
                    let startup_config = config.clone();
                    tauri::async_runtime::spawn(async move {
                        let _ = sync::refresh_types(&startup, &startup_config).await;
                    });
                    let update_handle = handle.clone();
                    tauri::async_runtime::spawn(async move {
                        sleep_ms(2000).await;
                        if let Ok(Some(notice)) = commands::check_updates(update_handle.clone()).await
                        {
                            let _ = update_handle.emit_all("update-available", notice);
                        }
                    });
                }
                _ => {
                    let storage = config::default_storage_dir(&handle).ok();
                    if let Some(storage) = storage.as_ref() {
                        let _ = config::ensure_user_templates(&handle, storage);
                        let defaults = Config::defaults(storage.to_string_lossy().into());
                        if let Ok(conn) = open_db_for(&defaults) {
                            let desks = db::list_desks(&conn).unwrap_or_default();
                            if let Ok(mut inner) = handle.state::<AppState>().inner.lock() {
                                inner.config = Some(defaults);
                                inner.db = Some(conn);
                                inner.desks = desks;
                            }
                        } else if let Ok(mut inner) = handle.state::<AppState>().inner.lock() {
                            inner.config = Some(defaults);
                        }
                    }
                    let _ = commands::open_settings(handle.clone());
                }
            }

            let probe = handle.clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    sleep_ms(60_000).await;
                    let should = {
                        probe
                            .state::<AppState>()
                            .inner
                            .lock()
                            .ok()
                            .map(|inner| inner.config.as_ref().is_some_and(|c| c.is_complete()))
                            .unwrap_or(false)
                    };
                    if should {
                        let _ = sync::sync_once(&probe).await;
                    }
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running StatTracker");
}

async fn sleep_ms(ms: u64) {
    tauri::async_runtime::spawn_blocking(move || {
        std::thread::sleep(Duration::from_millis(ms));
    })
    .await
    .ok();
}
