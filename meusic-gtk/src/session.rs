//! Last-session persistence (session.json in the config dir): the scanned root
//! folder, the last page (mode + selected group), and the last track + its
//! position. Lets the app resume where the user left off — gated at restore
//! time by the resume_startup_page / remember_last_played settings.

use crate::util::config_file;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Session {
    pub root_path: String,
    pub mode: String, // "folders" | "albums" | "artists" | "songs"
    pub sel_group: Option<String>,
    pub track_path: Option<String>,
    pub position: f64,
}

pub fn load() -> Session {
    config_file("session.json")
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(s: &Session) {
    if let Some(p) = config_file("session.json") {
        if let Ok(j) = serde_json::to_string_pretty(s) {
            let _ = std::fs::write(p, j);
        }
    }
}
