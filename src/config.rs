//! Persistent state, stored as JSON in a per-OS config directory.
//!
//! We deliberately use `~/.config/presenter` on both macOS and Linux (rather than
//! macOS's `~/Library/Application Support`), and `%APPDATA%\presenter` on Windows.
//! We manage this file ourselves (rather than via eframe storage) so it always lives
//! at this location and is written promptly.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The `presenter` configuration directory for the current OS.
pub fn config_dir() -> PathBuf {
  let base = if cfg!(windows) {
    std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."))
  } else {
    std::env::var_os("XDG_CONFIG_HOME")
      .map(PathBuf::from)
      .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
      .unwrap_or_else(|| PathBuf::from(".config"))
  };
  base.join("presenter")
}

/// Full path to the persisted state file.
pub fn state_path() -> PathBuf {
  config_dir().join("state.json")
}

/// A window's position and size (logical pixels).
#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub struct Geometry {
  pub x: f32,
  pub y: f32,
  pub w: f32,
  pub h: f32,
}

/// A recently opened document and the page last viewed in it (for resume).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RecentEntry {
  pub path: PathBuf,
  pub page: usize,
}

/// Everything persisted between sessions.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct State {
  pub footer_font: f32,
  pub layout_index: usize,
  pub audience_fullscreen: bool,
  pub audience_geometry: Option<Geometry>,
  pub window_geometry: Option<Geometry>,
  pub recent: Vec<RecentEntry>,
}

impl Default for State {
  fn default() -> Self {
    Self {
      footer_font: 24.0,
      layout_index: 0,
      audience_fullscreen: true,
      audience_geometry: None,
      window_geometry: None,
      recent: Vec::new(),
    }
  }
}

impl State {
  /// Load the state file, or defaults if it is missing or unreadable.
  pub fn load() -> Self {
    match std::fs::read_to_string(state_path()) {
      Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
        tracing::warn!("state file unreadable, using defaults: {e}");
        State::default()
      }),
      Err(_) => State::default(),
    }
  }

  /// Write the state file, creating the config directory if needed.
  pub fn save(&self) {
    let path = state_path();
    if let Some(dir) = path.parent() {
      if let Err(e) = std::fs::create_dir_all(dir) {
        tracing::warn!("could not create config dir {}: {e}", dir.display());
        return;
      }
    }
    match serde_json::to_string_pretty(self) {
      Ok(json) => {
        if let Err(e) = std::fs::write(&path, json) {
          tracing::warn!("could not write state {}: {e}", path.display());
        }
      }
      Err(e) => tracing::warn!("could not serialize state: {e}"),
    }
  }

  /// Move `path` to the front of the recent list, keeping its last page.
  pub fn record_recent(&mut self, path: &Path, page: usize, max: usize) {
    let path = path.to_path_buf();
    self.recent.retain(|e| e.path != path);
    self.recent.insert(0, RecentEntry { path, page });
    self.recent.truncate(max);
  }

  /// The last page viewed in `path`, if known.
  pub fn last_page(&self, path: &Path) -> Option<usize> {
    self.recent.iter().find(|e| e.path == path).map(|e| e.page)
  }

  /// Update the stored page for `path` if it is in the recent list.
  pub fn set_page(&mut self, path: &Path, page: usize) {
    if let Some(e) = self.recent.iter_mut().find(|e| e.path == path) {
      e.page = page;
    }
  }
}
