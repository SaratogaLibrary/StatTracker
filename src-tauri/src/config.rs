use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::error::{AppError, AppResult};

const POINTER_FILE: &str = "storage-path.txt";
const CONFIG_FILE: &str = "config.toml";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub base_url: String,
    pub desk_id: i64,
    pub desk_name: String,
    #[serde(default)]
    pub org_name: String,
    #[serde(default = "default_title")]
    pub window_title: String,
    pub storage_directory: String,
    #[serde(default = "default_update_mode")]
    pub update_mode: String,
    #[serde(default = "default_true")]
    pub autostart: bool,
    #[serde(default)]
    pub always_on_top: bool,
    #[serde(default = "default_opacity")]
    pub unfocused_opacity: u8,
    #[serde(default)]
    pub inactivity_enabled: bool,
    #[serde(default = "default_inactivity_minutes")]
    pub inactivity_minutes: u32,
    #[serde(default = "default_inactivity_message")]
    pub inactivity_message: String,
    #[serde(default = "default_template")]
    pub active_template: String,
}

fn default_title() -> String {
    "StatTracker".into()
}
fn default_update_mode() -> String {
    "notify".into()
}
fn default_true() -> bool {
    true
}
fn default_opacity() -> u8 {
    55
}
fn default_inactivity_minutes() -> u32 {
    15
}
fn default_inactivity_message() -> String {
    "No questions have been recorded recently. Please remember to log desk activity.".into()
}
fn default_template() -> String {
    "horizontal".into()
}

impl Config {
    pub fn defaults(storage_directory: String) -> Self {
        Self {
            base_url: String::new(),
            desk_id: 0,
            desk_name: String::new(),
            org_name: String::new(),
            window_title: default_title(),
            storage_directory,
            update_mode: default_update_mode(),
            autostart: true,
            always_on_top: false,
            unfocused_opacity: default_opacity(),
            inactivity_enabled: false,
            inactivity_minutes: default_inactivity_minutes(),
            inactivity_message: default_inactivity_message(),
            active_template: default_template(),
        }
    }

    pub fn is_complete(&self) -> bool {
        !self.base_url.is_empty() && self.desk_id > 0 && !self.storage_directory.is_empty()
    }

    pub fn normalize(&mut self) {
        self.base_url = normalize_base_url(&self.base_url);
        if self.unfocused_opacity > 100 {
            self.unfocused_opacity = 100;
        }
        if self.update_mode != "silent" {
            self.update_mode = "notify".into();
        }
        if !is_safe_template_id(&self.active_template) {
            self.active_template = default_template();
        }
        if self.window_title.is_empty() {
            self.window_title = default_title();
        }
        if self.inactivity_minutes == 0 {
            self.inactivity_minutes = default_inactivity_minutes();
        }
    }
}

pub fn normalize_base_url(url: &str) -> String {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.ends_with('/') {
        trimmed.to_string()
    } else {
        format!("{trimmed}/")
    }
}

pub fn app_data_dir(app: &AppHandle) -> AppResult<PathBuf> {
    app.path_resolver()
        .app_data_dir()
        .ok_or_else(|| AppError::Message("Could not resolve the application data directory.".into()))
}

pub fn pointer_path(app: &AppHandle) -> AppResult<PathBuf> {
    Ok(app_data_dir(app)?.join(POINTER_FILE))
}

pub fn default_storage_dir(app: &AppHandle) -> AppResult<PathBuf> {
    app_data_dir(app)
}

pub fn is_safe_template_id(id: &str) -> bool {
    if id.is_empty() || id.contains('/') || id.contains('\\') || id.contains('\0') {
        return false;
    }
    let path = Path::new(id);
    if path.is_absolute() || path.has_root() {
        return false;
    }
    let mut components = path.components();
    matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none()
}

pub fn normalize_storage_path(path: &Path) -> AppResult<PathBuf> {
    let raw = path.to_string_lossy();
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(AppError::Message("Storage location is empty.".into()));
    }
    let path = PathBuf::from(trimmed);
    if let Ok(canonical) = fs::canonicalize(&path) {
        return Ok(strip_verbatim_prefix(canonical));
    }
    if !path.is_absolute() {
        return Err(AppError::Message(
            "Storage location must be an absolute path.".into(),
        ));
    }
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir | Component::CurDir))
    {
        return Err(AppError::Message(
            "Storage location must not contain '.' or '..'.".into(),
        ));
    }
    Ok(strip_verbatim_prefix(path))
}

pub(crate) fn strip_verbatim_prefix(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy();
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        PathBuf::from(format!(r"\\{rest}"))
    } else if let Some(rest) = text.strip_prefix(r"\\?\") {
        PathBuf::from(rest)
    } else {
        path
    }
}

pub fn template_dir(storage: &Path, template_id: &str) -> AppResult<PathBuf> {
    if !is_safe_template_id(template_id) {
        return Err(AppError::Message(format!(
            "Template '{template_id}' is not allowed."
        )));
    }
    let root = storage.join("templates");
    let dir = root.join(template_id);
    ensure_inside(&root, &dir)
}

fn ensure_inside(root: &Path, candidate: &Path) -> AppResult<PathBuf> {
    if root.exists() && candidate.exists() {
        let root_canon = fs::canonicalize(root)?;
        let candidate_canon = fs::canonicalize(candidate)?;
        if !candidate_canon.starts_with(&root_canon) {
            return Err(AppError::Message(
                "Template path is outside the templates directory.".into(),
            ));
        }
        return Ok(candidate_canon);
    }
    Ok(candidate.to_path_buf())
}

pub fn read_storage_dir(app: &AppHandle) -> AppResult<PathBuf> {
    let pointer = pointer_path(app)?;
    if pointer.exists() {
        let text = fs::read_to_string(&pointer)?;
        let path = PathBuf::from(text.trim());
        if let Ok(normalized) = normalize_storage_path(&path) {
            if normalized.as_os_str() != path.as_os_str() {
                let _ = write_storage_pointer(app, &normalized);
            }
            return Ok(normalized);
        }
    }
    default_storage_dir(app)
}

pub fn write_storage_pointer(app: &AppHandle, storage: &Path) -> AppResult<()> {
    let dir = app_data_dir(app)?;
    fs::create_dir_all(&dir)?;
    fs::write(pointer_path(app)?, storage.to_string_lossy().as_bytes())?;
    Ok(())
}

pub fn config_path_in(storage: &Path) -> PathBuf {
    storage.join(CONFIG_FILE)
}

pub fn load_config(app: &AppHandle) -> AppResult<Option<Config>> {
    let storage = read_storage_dir(app)?;
    let path = config_path_in(&storage);
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(path)?;
    let mut config: Config = toml::from_str(&text)?;
    config.storage_directory = storage.to_string_lossy().into_owned();
    config.normalize();
    Ok(Some(config))
}

pub fn save_config(app: &AppHandle, config: &Config) -> AppResult<()> {
    let storage = normalize_storage_path(Path::new(&config.storage_directory))?;
    fs::create_dir_all(&storage)?;
    write_storage_pointer(app, &storage)?;
    let mut to_write = config.clone();
    to_write.storage_directory = storage.to_string_lossy().into_owned();
    let text = toml::to_string_pretty(&to_write)?;
    fs::write(config_path_in(&storage), text)?;
    Ok(())
}

pub fn bundled_templates_dir(app: &AppHandle) -> PathBuf {
    if let Some(path) = app.path_resolver().resolve_resource("templates") {
        if path.exists() {
            return path;
        }
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../templates")
}

pub fn copy_dir_all(src: &Path, dst: &Path) -> AppResult<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let target = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_all(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

pub fn ensure_user_templates(app: &AppHandle, storage: &Path) -> AppResult<PathBuf> {
    let dest = storage.join("templates");
    fs::create_dir_all(&dest)?;
    let src = bundled_templates_dir(app);
    if src.exists() {
        for entry in fs::read_dir(&src)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                let name = entry.file_name();
                let Some(name) = name.to_str() else {
                    continue;
                };
                if !is_safe_template_id(name) {
                    continue;
                }
                let target = dest.join(name);
                if !target.exists() || cfg!(debug_assertions) {
                    copy_dir_all(&entry.path(), &target)?;
                }
            }
        }
    }
    Ok(dest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_ids_must_be_a_single_normal_component() {
        assert!(is_safe_template_id("horizontal"));
        assert!(is_safe_template_id("my-template"));
        assert!(!is_safe_template_id(""));
        assert!(!is_safe_template_id("."));
        assert!(!is_safe_template_id(".."));
        assert!(!is_safe_template_id("foo/bar"));
        assert!(!is_safe_template_id("foo\\bar"));
        assert!(!is_safe_template_id("../evil"));
    }

    #[test]
    fn storage_paths_must_be_absolute_without_parent_dir() {
        assert!(normalize_storage_path(Path::new("")).is_err());
        assert!(normalize_storage_path(Path::new("relative-folder")).is_err());
        let traversal = if cfg!(windows) {
            Path::new(r"C:\StatTrackerMissingDir\..\AlsoMissingStatTracker")
        } else {
            Path::new("/tmp/stattracker-missing-dir/../also-missing-stattracker")
        };
        assert!(normalize_storage_path(traversal).is_err());
    }

    #[test]
    fn strips_windows_verbatim_prefix() {
        let local = strip_verbatim_prefix(PathBuf::from(
            r"\\?\C:\Users\demo\AppData\Roaming\org.sspl.stattracker",
        ));
        assert_eq!(
            local,
            PathBuf::from(r"C:\Users\demo\AppData\Roaming\org.sspl.stattracker")
        );
        let unc = strip_verbatim_prefix(PathBuf::from(r"\\?\UNC\server\share\folder"));
        assert_eq!(unc, PathBuf::from(r"\\server\share\folder"));
    }
}

