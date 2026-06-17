//! User settings, persisted to settings.json in the config dir, plus a runtime
//! check for whether a system tray is available (KDE/AppIndicator vs stock GNOME).

use crate::util::config_file;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub remember_last_played: bool,
    pub resume_startup_page: bool,
    pub follow_song: bool,
    pub volume_scroll_step: u32,
    pub tray_icon: bool,
    pub minimize_to_tray: bool,
    pub close_to_tray: bool,
    pub power_save: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            remember_last_played: true,
            resume_startup_page: true,
            follow_song: true,
            volume_scroll_step: 2,
            tray_icon: true,
            minimize_to_tray: true,
            close_to_tray: true,
            power_save: false,
        }
    }
}

pub fn load() -> Settings {
    config_file("settings.json")
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(s: &Settings) {
    if let Some(p) = config_file("settings.json") {
        if let Ok(j) = serde_json::to_string_pretty(s) {
            let _ = std::fs::write(p, j);
        }
    }
}

/// Whether a StatusNotifier host (system tray) is registered on the session bus
/// — true on KDE or GNOME-with-AppIndicator, false on stock GNOME. Used to adapt
/// the tray settings + window close/minimize behavior so the window is never
/// stranded where there's no tray to restore it from.
pub fn has_system_tray() -> bool {
    zbus::block_on(async {
        let Ok(conn) = zbus::Connection::session().await else {
            return false;
        };
        let Ok(dbus) = zbus::fdo::DBusProxy::new(&conn).await else {
            return false;
        };
        match dbus.list_names().await {
            Ok(names) => names
                .iter()
                .any(|n| n.as_str().contains("StatusNotifierWatcher")),
            Err(_) => false,
        }
    })
}
