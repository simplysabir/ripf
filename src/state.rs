//! Session state written on exit and restored by `-r`.
//!
//! Separate from config on purpose: config is hand-edited and lives in
//! ~/.config; this is machine-written and lives in ~/.local/state. Losing it
//! is never an error — every failure path here degrades to "no resume".

use crate::app::Mode;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize)]
pub struct State {
    pub query: String,
    pub mode: Mode,
    pub selected: usize,
}

/// XDG state dir, same reasoning as `config_path`: on macOS `dirs` would send
/// this to ~/Library/Application Support, but ripf is a terminal tool.
fn state_path() -> Option<PathBuf> {
    let base = match std::env::var_os("XDG_STATE_HOME") {
        Some(x) if !x.is_empty() => PathBuf::from(x),
        _ => dirs::home_dir()?.join(".local").join("state"),
    };
    Some(base.join("ripf").join("state.json"))
}

pub fn load() -> Option<State> {
    let text = std::fs::read_to_string(state_path()?).ok()?;
    serde_json::from_str(&text).ok()
}

/// Best-effort. A failure to persist state must never surface to the user.
pub fn save(state: &State) {
    let Some(path) = state_path() else { return };
    let Some(dir) = path.parent() else { return };
    if std::fs::create_dir_all(dir).is_err() {
        return;
    }
    if let Ok(json) = serde_json::to_string_pretty(state) {
        let _ = std::fs::write(&path, json);
    }
}
