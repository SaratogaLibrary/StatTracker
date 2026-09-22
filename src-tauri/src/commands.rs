use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use auto_launch::AutoLaunchBuilder;
use serde::{Deserialize, Serialize};
use tauri::{
    AppHandle, LogicalSize, Manager, PhysicalPosition, Size, WindowBuilder, WindowUrl,
};

use crate::api;
use crate::config::{self, Config};
use crate::db::{self, Desk, QuestionType, Stats};
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::sync;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OuterRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Serialize)]
pub struct TemplateInfo {
    pub id: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Serialize)]
pub struct TemplatePayload {
    pub id: String,
    pub name: String,
    pub html: String,
    pub css: String,
    pub js: String,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Serialize)]
pub struct Bootstrap {
    pub configured: bool,
    pub config: Config,
    pub question_types: Vec<QuestionType>,
    pub desks: Vec<Desk>,
    pub stats: Stats,
    pub online: bool,
    pub last_error: Option<String>,
    pub template: Option<TemplatePayload>,
    pub templates: Vec<TemplateInfo>,
    pub default_storage: String,
    pub pending_storage: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RecordResult {
    pub stats: Stats,
    pub online: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveConfigInput {
    pub base_url: String,
    pub desk_id: i64,
    pub desk_name: String,
    #[serde(default)]
    pub org_name: String,
    #[serde(default)]
    pub window_title: String,
    #[serde(default)]
    pub storage_directory: String,
    #[serde(default)]
    pub update_mode: String,
    #[serde(default = "default_true")]
    pub autostart: bool,
    #[serde(default)]
    pub always_on_top: bool,
    #[serde(default)]
    pub unfocused_opacity: u8,
    #[serde(default)]
    pub inactivity_enabled: bool,
    #[serde(default)]
    pub inactivity_minutes: u32,
    #[serde(default)]
    pub inactivity_message: String,
    #[serde(default)]
    pub active_template: String,
}

fn default_true() -> bool {
    true
}

pub fn apply_autostart(app: &AppHandle, enabled: bool) -> AppResult<()> {
    if cfg!(debug_assertions) {
        return Ok(());
    }
    let exe = std::env::current_exe()?;
    let name = app.package_info().name.clone();
    let auto = AutoLaunchBuilder::new()
        .set_app_name(&name)
        .set_app_path(&exe.to_string_lossy())
        .set_use_launch_agent(true)
        .build()
        .map_err(|err| AppError::Message(err.to_string()))?;
    if enabled {
        auto.enable()
            .map_err(|err| AppError::Message(err.to_string()))?;
    } else {
        let _ = auto.disable();
    }
    Ok(())
}

fn db_path(storage: &str) -> PathBuf {
    PathBuf::from(storage).join("stattracker.db")
}

fn pending_or_configured_storage(app: &AppHandle) -> AppResult<PathBuf> {
    let pending = {
        let state = app.state::<AppState>();
        let inner = state
            .inner
            .lock()
            .map_err(|_| "Internal state lock was poisoned.")?;
        if let Some(pending) = inner.pending_storage.as_ref() {
            Some(pending.clone())
        } else {
            inner
                .config
                .as_ref()
                .map(|config| config.storage_directory.clone())
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
        }
    };
    if let Some(path) = pending {
        return config::normalize_storage_path(&path);
    }
    config::default_storage_dir(app)
}

pub fn open_db_for(config: &Config) -> AppResult<rusqlite::Connection> {
    db::open(&db_path(&config.storage_directory))
}

pub fn read_template(storage: &str, template_id: &str) -> AppResult<TemplatePayload> {
    let dir = config::template_dir(Path::new(storage), template_id)?;
    if !dir.exists() {
        return Err(AppError::Message(format!(
            "Template '{template_id}' was not found."
        )));
    }
    let manifest: serde_json::Value = if dir.join("manifest.json").exists() {
        serde_json::from_str(&fs::read_to_string(dir.join("manifest.json"))?)?
    } else {
        serde_json::json!({})
    };
    let name = manifest
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or(template_id)
        .to_string();
    let window = manifest.get("window").cloned().unwrap_or(serde_json::json!({}));
    let width = window.get("width").and_then(|v| v.as_u64()).unwrap_or(720) as u32;
    let height = window.get("height").and_then(|v| v.as_u64()).unwrap_or(88) as u32;
    let html = fs::read_to_string(dir.join("index.html")).unwrap_or_default();
    let css = fs::read_to_string(dir.join("style.css")).unwrap_or_default();
    let js = fs::read_to_string(dir.join("script.js")).unwrap_or_default();
    Ok(TemplatePayload {
        id: template_id.to_string(),
        name,
        html,
        css,
        js,
        width,
        height,
    })
}

pub fn list_templates_in(storage: &str) -> Vec<TemplateInfo> {
    let dir = PathBuf::from(storage).join("templates");
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        if !entry.path().is_dir() {
            continue;
        }
        let id = entry.file_name().to_string_lossy().into_owned();
        if !config::is_safe_template_id(&id) {
            continue;
        }
        if let Ok(payload) = read_template(storage, &id) {
            out.push(TemplateInfo {
                id: payload.id,
                name: payload.name,
                width: payload.width,
                height: payload.height,
            });
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

fn substitute(html: String, config: &Config) -> String {
    html.replace("{{deskName}}", &config.desk_name)
        .replace("{{orgName}}", &config.org_name)
        .replace("{{windowTitle}}", &config.window_title)
}

pub fn build_bootstrap(app: &AppHandle) -> AppResult<Bootstrap> {
    let default_storage = config::default_storage_dir(app)?
        .to_string_lossy()
        .into_owned();
    let state = app.state::<AppState>();
    let inner = state
        .inner
        .lock()
        .map_err(|_| "Internal state lock was poisoned.")?;
    let mut config = inner
        .config
        .clone()
        .unwrap_or_else(|| Config::defaults(default_storage.clone()));
    if !config.storage_directory.is_empty() {
        config.storage_directory = config::strip_verbatim_prefix(PathBuf::from(
            &config.storage_directory,
        ))
        .to_string_lossy()
        .into_owned();
    }
    let templates = if config.storage_directory.is_empty() {
        Vec::new()
    } else {
        list_templates_in(&config.storage_directory)
    };
    let template = if config.is_complete() {
        read_template(&config.storage_directory, &config.active_template)
            .ok()
            .map(|mut payload| {
                payload.html = substitute(payload.html, &config);
                payload
            })
    } else {
        None
    };
    Ok(Bootstrap {
        configured: config.is_complete(),
        question_types: inner.question_types.clone(),
        desks: inner
            .db
            .as_ref()
            .and_then(|conn| db::list_desks(conn).ok())
            .filter(|list| !list.is_empty())
            .unwrap_or_else(|| inner.desks.clone()),
        stats: inner.stats(),
        online: inner.online,
        last_error: inner.last_error.clone(),
        template,
        templates,
        pending_storage: inner.pending_storage.as_ref().map(|path| {
            config::strip_verbatim_prefix(path.clone())
                .to_string_lossy()
                .into_owned()
        }),
        config,
        default_storage,
    })
}

fn question_types_source_changed(previous: Option<&Config>, next: &Config) -> bool {
    match previous {
        Some(prev) if prev.is_complete() => {
            prev.base_url != next.base_url || prev.desk_id != next.desk_id
        }
        _ => true,
    }
}

fn publish_saved_config(app: &AppHandle, close_settings: bool) {
    if let Ok(bootstrap) = build_bootstrap(app) {
        let ready = !bootstrap.question_types.is_empty();
        let _ = app.emit_all("config-saved", &bootstrap);
        if close_settings && ready {
            if let Some(settings) = app.get_window("settings") {
                let _ = settings.close();
            }
        }
    }
}

#[tauri::command]
pub fn get_bootstrap(app: AppHandle) -> AppResult<Bootstrap> {
    build_bootstrap(&app)
}

#[tauri::command]
pub fn get_default_storage(app: AppHandle) -> AppResult<String> {
    Ok(config::default_storage_dir(&app)?
        .to_string_lossy()
        .into_owned())
}

fn folder_dialog_start(path: &Path) -> Option<PathBuf> {
    let mut candidate = path.to_path_buf();
    if !candidate.exists() {
        candidate = candidate.parent()?.to_path_buf();
    }
    if !candidate.exists() {
        return None;
    }
    Some(config::strip_verbatim_prefix(candidate))
}

#[tauri::command]
pub async fn pick_directory(app: AppHandle) -> AppResult<Option<String>> {
    let current = pending_or_configured_storage(&app).ok();
    if let Some(path) = current.as_ref() {
        let _ = fs::create_dir_all(path);
    }
    let start = current.as_deref().and_then(folder_dialog_start);
    let picked = tauri::async_runtime::spawn_blocking(move || {
        let mut dialog = rfd::FileDialog::new().set_title("Select storage location");
        if let Some(start) = start {
            dialog = dialog.set_directory(start);
        }
        dialog.pick_folder()
    })
    .await
    .ok()
    .flatten();
    let Some(picked) = picked else {
        return Ok(None);
    };
    let normalized = config::normalize_storage_path(&picked)?;
    {
        let state = app.state::<AppState>();
        let mut inner = state
            .inner
            .lock()
            .map_err(|_| "Internal state lock was poisoned.")?;
        inner.pending_storage = Some(normalized.clone());
    }
    Ok(Some(normalized.to_string_lossy().into_owned()))
}

#[tauri::command]
pub async fn fetch_desks(app: AppHandle, base_url: String) -> AppResult<Vec<Desk>> {
    let desks = api::fetch_desks(&config::normalize_base_url(&base_url)).await?;
    let storage = pending_or_configured_storage(&app)?;
    fs::create_dir_all(&storage)?;
    let conn = db::open(&storage.join("stattracker.db"))?;
    db::replace_desks(&conn, &desks)?;
    if let Ok(mut inner) = app.state::<AppState>().inner.lock() {
        inner.desks = desks.clone();
        if let Some(existing) = inner.db.as_ref() {
            let _ = db::replace_desks(existing, &desks);
        } else {
            inner.db = Some(conn);
        }
    }
    Ok(desks)
}

#[tauri::command]
pub async fn save_config(app: AppHandle, input: SaveConfigInput) -> AppResult<Bootstrap> {
    let storage = pending_or_configured_storage(&app)?;
    let mut config = Config {
        base_url: input.base_url,
        desk_id: input.desk_id,
        desk_name: input.desk_name,
        org_name: input.org_name,
        window_title: input.window_title,
        storage_directory: storage.to_string_lossy().into_owned(),
        update_mode: input.update_mode,
        autostart: input.autostart,
        always_on_top: input.always_on_top,
        unfocused_opacity: input.unfocused_opacity,
        inactivity_enabled: input.inactivity_enabled,
        inactivity_minutes: input.inactivity_minutes,
        inactivity_message: input.inactivity_message,
        active_template: input.active_template,
    };
    let _ = input.storage_directory;
    config.normalize();
    if !config.is_complete() {
        return Err(AppError::Message(
            "A base URL, desk, and storage location are required.".into(),
        ));
    }

    let previous = {
        let state = app.state::<AppState>();
        let inner = state
            .inner
            .lock()
            .map_err(|_| "Internal state lock was poisoned.")?;
        inner.config.clone()
    };

    fs::create_dir_all(&config.storage_directory)?;
    let _ = config::ensure_user_templates(&app, PathBuf::from(&config.storage_directory).as_path());
    config::save_config(&app, &config)?;
    let conn = open_db_for(&config)?;
    let cached = db::list_question_types(&conn).unwrap_or_default();
    let cached_desks = {
        let existing = db::list_desks(&conn).unwrap_or_default();
        if !existing.is_empty() {
            existing
        } else {
            app.state::<AppState>()
                .inner
                .lock()
                .ok()
                .map(|inner| inner.desks.clone())
                .unwrap_or_default()
        }
    };
    if db::list_desks(&conn).unwrap_or_default().is_empty() && !cached_desks.is_empty() {
        let _ = db::replace_desks(&conn, &cached_desks);
    }
    let needs_remote_types =
        question_types_source_changed(previous.as_ref(), &config) || cached.is_empty();
    {
        let state = app.state::<AppState>();
        let mut inner = state
            .inner
            .lock()
            .map_err(|_| "Internal state lock was poisoned.")?;
        inner.config = Some(config.clone());
        inner.db = Some(conn);
        inner.question_types = cached;
        inner.desks = cached_desks;
        inner.pending_storage = None;
    }

    let _ = apply_autostart(&app, config.autostart);

    if needs_remote_types {
        let handle = app.clone();
        let refresh_config = config.clone();
        tauri::async_runtime::spawn(async move {
            let _ = sync::refresh_types(&handle, &refresh_config).await;
            publish_saved_config(&handle, true);
        });
    } else {
        publish_saved_config(&app, true);
    }

    build_bootstrap(&app)
}

#[tauri::command]
pub async fn refresh_question_types(app: AppHandle) -> AppResult<Bootstrap> {
    let config = {
        let state = app.state::<AppState>();
        let inner = state
            .inner
            .lock()
            .map_err(|_| "Internal state lock was poisoned.")?;
        inner.config.clone().ok_or(AppError::NotConfigured)?
    };
    let _ = sync::refresh_types(&app, &config).await;
    build_bootstrap(&app)
}

#[tauri::command]
pub fn cycle_template(app: AppHandle) -> AppResult<Bootstrap> {
    let mut config = {
        let state = app.state::<AppState>();
        let inner = state
            .inner
            .lock()
            .map_err(|_| "Internal state lock was poisoned.")?;
        inner.config.clone().ok_or(AppError::NotConfigured)?
    };
    let templates = list_templates_in(&config.storage_directory);
    if templates.len() < 2 {
        return build_bootstrap(&app);
    }
    let index = templates
        .iter()
        .position(|template| template.id == config.active_template)
        .unwrap_or(0);
    config.active_template = templates[(index + 1) % templates.len()].id.clone();
    config::save_config(&app, &config)?;
    {
        let state = app.state::<AppState>();
        let mut inner = state
            .inner
            .lock()
            .map_err(|_| "Internal state lock was poisoned.")?;
        inner.config = Some(config);
    }
    publish_saved_config(&app, false);
    build_bootstrap(&app)
}

#[tauri::command]
pub fn record_tally(app: AppHandle, question_type_id: i64) -> AppResult<RecordResult> {
    let stats = {
        let state = app.state::<AppState>();
        let inner = state
            .inner
            .lock()
            .map_err(|_| "Internal state lock was poisoned.")?;
        let config = inner.config.clone().ok_or(AppError::NotConfigured)?;
        let conn = inner.db.as_ref().ok_or(AppError::NotConfigured)?;
        db::insert_tally(conn, config.desk_id, question_type_id)?;
        db::stats(conn)?
    };

    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let _ = sync::sync_once(&handle).await;
        if let Ok(inner) = handle.state::<AppState>().inner.lock() {
            let stats = inner.stats();
            let online = inner.online;
            let _ = handle.emit_all("tally-updated", RecordResult { stats, online });
        }
    });

    Ok(RecordResult {
        stats,
        online: app
            .state::<AppState>()
            .inner
            .lock()
            .map(|inner| inner.online)
            .unwrap_or(false),
    })
}

#[tauri::command]
pub fn get_stats(app: AppHandle) -> AppResult<Stats> {
    let state = app.state::<AppState>();
    let inner = state
        .inner
        .lock()
        .map_err(|_| "Internal state lock was poisoned.")?;
    Ok(inner.stats())
}

#[tauri::command]
pub async fn open_settings(app: AppHandle) -> AppResult<()> {
    if let Some(window) = app.get_window("settings") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return Ok(());
    }
    WindowBuilder::new(&app, "settings", WindowUrl::App("settings.html".into()))
        .title("StatTracker Settings")
        .inner_size(540.0, 780.0)
        .min_inner_size(420.0, 520.0)
        .resizable(true)
        .closable(true)
        .visible(true)
        .center()
        .build()?;
    Ok(())
}

#[tauri::command]
pub async fn open_help(app: AppHandle) -> AppResult<()> {
    if let Some(window) = app.get_window("help") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return Ok(());
    }
    WindowBuilder::new(&app, "help", WindowUrl::App("help.html".into()))
        .title("StatTracker Help")
        .inner_size(480.0, 640.0)
        .min_inner_size(360.0, 420.0)
        .resizable(true)
        .closable(true)
        .visible(true)
        .center()
        .build()?;
    Ok(())
}

#[tauri::command]
pub fn set_widget_size(app: AppHandle, width: f64, height: f64) -> AppResult<()> {
    if let Some(window) = app.get_window("main") {
        window.set_size(Size::Logical(LogicalSize {
            width: width.max(64.0),
            height: height.max(48.0),
        }))?;
    }
    Ok(())
}

#[cfg(windows)]
fn primary_work_area() -> Option<(i32, i32, i32, i32)> {
    #[repr(C)]
    struct Rect {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }
    const SPI_GETWORKAREA: u32 = 0x0030;
    extern "system" {
        fn SystemParametersInfoW(
            ui_action: u32,
            ui_param: u32,
            pv_param: *mut Rect,
            f_win_ini: u32,
        ) -> i32;
    }
    let mut rect = Rect {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    let ok = unsafe { SystemParametersInfoW(SPI_GETWORKAREA, 0, &mut rect, 0) };
    if ok != 0 {
        Some((rect.left, rect.top, rect.right, rect.bottom))
    } else {
        None
    }
}

fn clamp_to_work_area(
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
    margin: i32,
) -> (i32, i32) {
    let min_x = left + margin;
    let min_y = top + margin;
    let max_x = (right - width - margin).max(min_x);
    let max_y = (bottom - height - margin).max(min_y);
    (x.clamp(min_x, max_x), y.clamp(min_y, max_y))
}

#[cfg(windows)]
fn work_area_from_point(x: i32, y: i32) -> Option<(i32, i32, i32, i32)> {
    #[repr(C)]
    struct Rect {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }
    #[repr(C)]
    struct MonitorInfo {
        cb_size: u32,
        rc_monitor: Rect,
        rc_work: Rect,
        dw_flags: u32,
    }
    const MONITOR_DEFAULTTONEAREST: u32 = 2;
    extern "system" {
        fn MonitorFromRect(lprc: *const Rect, dw_flags: u32) -> isize;
        fn GetMonitorInfoW(h_monitor: isize, lpmi: *mut MonitorInfo) -> i32;
    }
    let probe = Rect {
        left: x,
        top: y,
        right: x.saturating_add(1),
        bottom: y.saturating_add(1),
    };
    unsafe {
        let monitor = MonitorFromRect(&probe, MONITOR_DEFAULTTONEAREST);
        if monitor == 0 {
            return None;
        }
        let mut info = MonitorInfo {
            cb_size: std::mem::size_of::<MonitorInfo>() as u32,
            rc_monitor: Rect {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            },
            rc_work: Rect {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            },
            dw_flags: 0,
        };
        if GetMonitorInfoW(monitor, &mut info) == 0 {
            return None;
        }
        Some((
            info.rc_work.left,
            info.rc_work.top,
            info.rc_work.right,
            info.rc_work.bottom,
        ))
    }
}

fn placement_bounds(
    window: &tauri::Window,
    dock: bool,
    prior: Option<&OuterRect>,
) -> AppResult<(i32, i32, i32, i32)> {
    #[cfg(windows)]
    {
        if dock {
            if let Some(area) = primary_work_area() {
                return Ok(area);
            }
        } else if let Some(prior) = prior {
            let cx = prior.x + (prior.width as i32 / 2);
            let cy = prior.y + (prior.height as i32 / 2);
            if let Some(area) = work_area_from_point(cx, cy) {
                return Ok(area);
            }
        } else if let Ok(pos) = window.outer_position() {
            if let Some(area) = work_area_from_point(pos.x, pos.y) {
                return Ok(area);
            }
        }
        if let Some(area) = primary_work_area() {
            return Ok(area);
        }
        monitor_bounds(window)
    }
    #[cfg(not(windows))]
    {
        let _ = (dock, prior);
        monitor_bounds(window)
    }
}

#[tauri::command]
pub fn widget_outer_rect(app: AppHandle) -> Option<OuterRect> {
    let window = app.get_window("main")?;
    let pos = window.outer_position().ok()?;
    let size = window.outer_size().ok()?;
    Some(OuterRect {
        x: pos.x,
        y: pos.y,
        width: size.width,
        height: size.height,
    })
}

/// Size and place the widget after a layout measure, before fade-in.
///
/// - `dock`: first show — pin the new size to the primary work-area bottom-right.
/// - otherwise: keep the previous window's bottom-right corner, then clamp so
///   no edge sits outside that monitor's work area (taskbar-excluded).
#[tauri::command]
pub fn place_widget(
    app: AppHandle,
    width: f64,
    height: f64,
    dock: bool,
    prior: Option<OuterRect>,
) -> AppResult<()> {
    let Some(window) = app.get_window("main") else {
        return Ok(());
    };
    const MARGIN: i32 = 12;
    let width = width.max(64.0);
    let height = height.max(48.0);
    window.set_size(Size::Logical(LogicalSize { width, height }))?;

    let scale = window.scale_factor().unwrap_or(1.0);
    let mut phys_w = (width * scale).round() as i32;
    let mut phys_h = (height * scale).round() as i32;
    if let (Ok(inner), Ok(outer)) = (window.inner_size(), window.outer_size()) {
        phys_w += (outer.width as i32 - inner.width as i32).max(0);
        phys_h += (outer.height as i32 - inner.height as i32).max(0);
    }

    let (left, top, right, bottom) = placement_bounds(&window, dock, prior.as_ref())?;

    let (x, y) = if dock || prior.is_none() {
        (right - phys_w - MARGIN, bottom - phys_h - MARGIN)
    } else {
        let prior = prior.unwrap();
        (
            prior.x + prior.width as i32 - phys_w,
            prior.y + prior.height as i32 - phys_h,
        )
    };

    let (x, y) = clamp_to_work_area(x, y, phys_w, phys_h, left, top, right, bottom, MARGIN);
    window.set_position(PhysicalPosition::new(x, y))?;
    Ok(())
}

fn monitor_bounds(window: &tauri::Window) -> AppResult<(i32, i32, i32, i32)> {
    let monitor = window
        .primary_monitor()?
        .ok_or_else(|| AppError::Message("No primary monitor was found.".into()))?;
    let pos = monitor.position();
    let size = monitor.size();
    Ok((
        pos.x,
        pos.y,
        pos.x + size.width as i32,
        pos.y + size.height as i32,
    ))
}

#[tauri::command]
pub fn set_always_on_top(app: AppHandle, enabled: bool) -> AppResult<()> {
    if let Some(window) = app.get_window("main") {
        window.set_always_on_top(enabled)?;
    }
    let state = app.state::<AppState>();
    if let Ok(mut inner) = state.inner.lock() {
        if let Some(config) = inner.config.as_mut() {
            config.always_on_top = enabled;
            let snapshot = config.clone();
            drop(inner);
            let _ = config::save_config(&app, &snapshot);
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateNotice {
    pub version: String,
    pub current_version: String,
    pub notes: String,
}

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
}

fn parse_semver(value: &str) -> Option<(u64, u64, u64)> {
    let trimmed = value.trim().trim_start_matches(|ch: char| ch == 'v' || ch == 'V');
    let mut parts = trimmed.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().unwrap_or("0").parse().ok()?;
    let patch = parts
        .next()
        .unwrap_or("0")
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>()
        .parse()
        .ok()?;
    Some((major, minor, patch))
}

fn github_releases_api(app: &AppHandle) -> Option<String> {
    let endpoint = app
        .config()
        .tauri
        .updater
        .endpoints
        .as_ref()?
        .first()?
        .to_string();
    let after = endpoint.split("github.com/").nth(1)?;
    let mut parts = after.split('/');
    let owner = parts.next()?;
    let repo = parts.next()?;
    if owner.is_empty() || repo.is_empty() {
        return None;
    }
    Some(format!(
        "https://api.github.com/repos/{owner}/{repo}/releases?per_page=30"
    ))
}

fn plain_release_notes(markdown: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    let mut blank = false;
    for raw in markdown.replace("\r\n", "\n").lines() {
        let mut line = raw.trim().to_string();
        if line.is_empty() {
            if !lines.is_empty() {
                blank = true;
            }
            continue;
        }
        line = line.trim_start_matches('#').trim().to_string();
        line = line.replace("**", "").replace("__", "");
        if let Some(rest) = line.strip_prefix("- ") {
            line = format!("• {rest}");
        } else if let Some(rest) = line.strip_prefix("* ") {
            line = format!("• {rest}");
        }
        if blank {
            lines.push(String::new());
            blank = false;
        }
        lines.push(line);
    }
    lines.join("\n").trim().to_string()
}

fn truncate_notes(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut end = 0;
    for (idx, ch) in text.char_indices().take(max_chars) {
        end = idx + ch.len_utf8();
    }
    let mut cut = &text[..end];
    if let Some(idx) = cut.rfind('\n') {
        if idx > max_chars / 2 {
            cut = &cut[..idx];
        }
    }
    format!("{}\n…", cut.trim_end())
}

fn format_release_notes(releases: &[GithubRelease], current: &str, latest: &str) -> String {
    let current_ver = parse_semver(current);
    let latest_ver = parse_semver(latest);
    let mut selected: Vec<( (u64, u64, u64), &GithubRelease )> = releases
        .iter()
        .filter(|release| !release.draft && !release.prerelease)
        .filter_map(|release| {
            let version = parse_semver(&release.tag_name)?;
            if let Some(current_ver) = current_ver {
                if version <= current_ver {
                    return None;
                }
            }
            if let Some(latest_ver) = latest_ver {
                if version > latest_ver {
                    return None;
                }
            }
            Some((version, release))
        })
        .collect();
    selected.sort_by(|a, b| b.0.cmp(&a.0));
    let mut blocks = Vec::new();
    for (version, release) in selected {
        let notes = plain_release_notes(&release.body);
        let heading = format!("{}.{}.{}", version.0, version.1, version.2);
        if notes.is_empty() {
            blocks.push(heading);
        } else {
            blocks.push(format!("{heading}\n{notes}"));
        }
    }
    truncate_notes(&blocks.join("\n\n"), 1600)
}

async fn github_notes_since(app: &AppHandle, current: &str, latest: &str) -> Option<String> {
    let url = github_releases_api(app)?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent("StatTracker/0.1")
        .build()
        .ok()?;
    let releases = client
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .json::<Vec<GithubRelease>>()
        .await
        .ok()?;
    let notes = format_release_notes(&releases, current, latest);
    if notes.is_empty() {
        None
    } else {
        Some(notes)
    }
}

#[tauri::command]
pub async fn check_updates(app: AppHandle) -> AppResult<Option<UpdateNotice>> {
    let mode = {
        let state = app.state::<AppState>();
        let inner = state
            .inner
            .lock()
            .map_err(|_| "Internal state lock was poisoned.")?;
        inner
            .config
            .as_ref()
            .map(|config| config.update_mode.clone())
            .unwrap_or_else(|| "notify".into())
    };
    match app.updater().check().await {
        Ok(update) => {
            if update.is_update_available() {
                if mode == "silent" {
                    update
                        .download_and_install()
                        .await
                        .map_err(|err| AppError::Message(err.to_string()))?;
                    return Ok(None);
                }
                let version = update.latest_version().to_string();
                let current_version = update.current_version().to_string();
                let fallback = update
                    .body()
                    .map(|body| plain_release_notes(body))
                    .unwrap_or_default();
                let notes = github_notes_since(&app, &current_version, &version)
                    .await
                    .unwrap_or(fallback);
                return Ok(Some(UpdateNotice {
                    version,
                    current_version,
                    notes,
                }));
            }
            Ok(None)
        }
        Err(_) => Ok(None),
    }
}

#[tauri::command]
pub async fn install_update(app: AppHandle) -> AppResult<()> {
    let update = app
        .updater()
        .check()
        .await
        .map_err(|err| AppError::Message(err.to_string()))?;
    if update.is_update_available() {
        update
            .download_and_install()
            .await
            .map_err(|err| AppError::Message(err.to_string()))?;
    }
    Ok(())
}

#[tauri::command]
pub fn close_widget(app: AppHandle) -> AppResult<()> {
    if let Some(window) = app.get_window("main") {
        window.close()?;
    }
    Ok(())
}

#[tauri::command]
pub fn show_widget(app: AppHandle) -> AppResult<()> {
    if let Some(window) = app.get_window("main") {
        window.show()?;
        let _ = window.unminimize();
    }
    Ok(())
}

#[tauri::command]
pub fn minimize_widget(app: AppHandle) -> AppResult<()> {
    if let Some(window) = app.get_window("main") {
        window.minimize()?;
    }
    Ok(())
}
