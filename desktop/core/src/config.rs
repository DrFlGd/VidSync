//! What the app keeps on this computer (the system's per-user config folder), as
//! opposed to the portable library folder: which library is open, libraries
//! opened before, where yt-dlp and ffmpeg are, cookies for YouTube, and the
//! page's preferences.

use crate::util::{read_json_object, write_atomic};
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};

pub fn home_dir() -> PathBuf {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."))
}

/// `~/Vid Sync`, until the user chooses another folder.
pub fn default_library_path() -> PathBuf {
    home_dir().join("Vid Sync")
}

/// How yt-dlp gets YouTube cookies, for members-only, age-restricted and
/// private videos. Kept on this computer only.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Cookies {
    /// "" (none), "browser" or "file".
    pub mode: String,
    /// The browser for `--cookies-from-browser` (chrome, edge, firefox, brave, ...).
    pub browser: String,
    /// A Netscape cookies file for `--cookies`.
    pub file: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub library: Option<PathBuf>,
    pub recent_libraries: Vec<PathBuf>,
    /// yt-dlp, when the user pointed at one ("" finds it).
    pub ytdlp_path: String,
    /// ffmpeg, when the user pointed at one ("" finds it).
    pub ffmpeg_path: String,
    pub cookies: Cookies,
}

impl AppConfig {
    pub fn load(path: &Path) -> Self {
        std::fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        write_atomic(path, &serde_json::to_vec_pretty(self)?)
    }

    pub fn library_path(&self) -> PathBuf {
        self.library.clone().unwrap_or_else(default_library_path)
    }

    pub fn set_library(&mut self, dir: &Path) {
        self.library = Some(dir.to_path_buf());
        self.recent_libraries.retain(|p| p != dir);
        self.recent_libraries.insert(0, dir.to_path_buf());
        self.recent_libraries.truncate(8);
    }
}

/// Preferences the page stores (a JSON object: theme, view, sort).
pub struct Prefs {
    path: PathBuf,
}

impl Prefs {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
    pub fn get(&self) -> Value {
        read_json_object(&self.path)
    }
    pub fn set(&self, prefs: &Value) -> Result<()> {
        if !prefs.is_object() {
            bail!("preferences must be an object");
        }
        write_atomic(&self.path, &serde_json::to_vec_pretty(prefs)?)
    }
}
