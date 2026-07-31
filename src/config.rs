use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const CONFIG_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Theme {
    /// OS 設定に従う (Windows レジストリの AppsUseLightTheme を参照)
    System,
    Dark,
    Light,
}

impl Default for Theme {
    fn default() -> Self {
        Theme::System
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SortKey {
    Name,
    Size,
    Modified,
    Kind,
}

impl Default for SortKey {
    fn default() -> Self {
        SortKey::Name
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub version: u32,
    #[serde(default = "def_show_files")]
    pub show_files: bool,
    #[serde(default)]
    pub theme: Theme,
    #[serde(default = "def_window_w")]
    pub window_w: f32,
    #[serde(default = "def_window_h")]
    pub window_h: f32,
    #[serde(default)]
    pub sort_key: SortKey,
    #[serde(default = "def_sort_asc")]
    pub sort_asc: bool,
}

fn def_show_files() -> bool {
    true
}
fn def_window_w() -> f32 {
    1000.0
}
fn def_window_h() -> f32 {
    640.0
}
fn def_sort_asc() -> bool {
    true
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            show_files: true,
            theme: Theme::System,
            window_w: 1000.0,
            window_h: 640.0,
            sort_key: SortKey::Name,
            sort_asc: true,
        }
    }
}

impl AppConfig {
    pub fn load() -> Self {
        let path = config_path();
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(_) => return Self::default(),
        };
        match serde_json::from_slice::<AppConfig>(&bytes) {
            Ok(c) if c.version == CONFIG_VERSION => c,
            _ => Self::default(),
        }
    }

    pub fn save(&self) {
        let path = config_path();
        if let Ok(json) = serde_json::to_vec_pretty(self) {
            let _ = crate::fsutil::atomic_write(&path, &json);
        }
    }
}

fn config_path() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    base.join("sizeview").join("config.json")
}
