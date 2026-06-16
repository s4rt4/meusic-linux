// meusic — Rust backend
// Recursive audio library scanning + tag/cover-art extraction via lofty.

mod radio;

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use lofty::prelude::*;
use lofty::read_from_path;
use lofty::tag::ItemKey;
use rayon::prelude::*;
use serde::Serialize;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::Manager;
use walkdir::WalkDir;

/// Metadata for a single audio track. `path` is the absolute file path the
/// frontend turns into an asset URL (via convertFileSrc) for playback.
#[derive(Serialize, Clone)]
struct Track {
    path: String,
    title: String,
    artist: String,
    album: String,
    album_artist: String,
    track_no: u32,
    duration: u64, // seconds
    has_cover: bool,
    format: String, // e.g. "FLAC", "MP3"
    bitrate: u32,   // kbps, 0 if unknown
}

const AUDIO_EXTS: &[&str] = &[
    "mp3", "flac", "wav", "ogg", "m4a", "aac", "opus", "wma", "aiff", "aif", "alac",
];

fn is_audio(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| AUDIO_EXTS.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

/// Read tags + properties for one file. Always returns a Track — on any read
/// failure it falls back to sensible defaults (filename as title) so a single
/// broken file never aborts a whole library scan.
fn read_track(path: &Path) -> Track {
    let path_str = path.to_string_lossy().to_string();
    let file_stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Unknown")
        .to_string();

    let mut title = file_stem;
    let mut artist = String::from("Unknown Artist");
    let mut album = String::from("Unknown Album");
    let mut album_artist = String::new();
    let mut track_no = 0u32;
    let mut duration = 0u64;
    let mut has_cover = false;
    let mut bitrate = 0u32;
    let format = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_uppercase())
        .unwrap_or_default();

    if let Ok(tagged) = read_from_path(path) {
        let props = tagged.properties();
        duration = props.duration().as_secs();
        bitrate = props
            .audio_bitrate()
            .or_else(|| props.overall_bitrate())
            .unwrap_or(0);
        if let Some(tag) = tagged.primary_tag().or_else(|| tagged.first_tag()) {
            if let Some(t) = tag.title() {
                if !t.trim().is_empty() {
                    title = t.to_string();
                }
            }
            if let Some(a) = tag.artist() {
                if !a.trim().is_empty() {
                    artist = a.to_string();
                }
            }
            if let Some(al) = tag.album() {
                if !al.trim().is_empty() {
                    album = al.to_string();
                }
            }
            if let Some(aa) = tag.get_string(&ItemKey::AlbumArtist) {
                album_artist = aa.to_string();
            }
            if let Some(tn) = tag.track() {
                track_no = tn;
            }
            has_cover = !tag.pictures().is_empty();
        }
    }

    if album_artist.trim().is_empty() {
        album_artist = artist.clone();
    }

    Track {
        path: path_str,
        title,
        artist,
        album,
        album_artist,
        track_no,
        duration,
        has_cover,
        format,
        bitrate,
    }
}

/// Recursively scan a folder (and all subfolders) for audio files, read their
/// metadata in parallel, and return them sorted by album-artist → album →
/// track number → title so albums group together naturally.
#[tauri::command]
fn scan_folder(path: String) -> Vec<Track> {
    let files: Vec<_> = WalkDir::new(&path)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file() && is_audio(e.path()))
        .map(|e| e.into_path())
        .collect();

    let mut tracks: Vec<Track> = files.par_iter().map(|p| read_track(p)).collect();

    tracks.sort_by(|a, b| {
        (
            a.album_artist.to_lowercase(),
            a.album.to_lowercase(),
            a.track_no,
            a.title.to_lowercase(),
        )
            .cmp(&(
                b.album_artist.to_lowercase(),
                b.album.to_lowercase(),
                b.track_no,
                b.title.to_lowercase(),
            ))
    });

    tracks
}

const IMAGE_EXTS: &[&str] = &["jpg", "jpeg", "png", "webp", "bmp", "gif"];
const COVER_NAMES: &[&str] = &[
    "cover",
    "folder",
    "front",
    "album",
    "albumart",
    "albumartsmall",
    "thumb",
    "artwork",
];

fn image_mime(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .as_deref()
    {
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        Some("bmp") => "image/bmp",
        Some("gif") => "image/gif",
        _ => "image/jpeg",
    }
}

fn read_image_bytes(path: &Path) -> Option<(Vec<u8>, String)> {
    let bytes = std::fs::read(path).ok()?;
    if bytes.is_empty() {
        return None;
    }
    Some((bytes, image_mime(path).to_string()))
}

/// Look beside the audio file for a standalone cover image — first the common
/// well-known names (cover.jpg, folder.jpg, …), then any image in the folder.
/// This is how players like Dopamine find art for files without embedded tags.
fn folder_cover_bytes(audio_path: &Path) -> Option<(Vec<u8>, String)> {
    let dir = audio_path.parent()?;

    for name in COVER_NAMES {
        for ext in IMAGE_EXTS {
            let candidate = dir.join(format!("{name}.{ext}"));
            if candidate.is_file() {
                if let Some(img) = read_image_bytes(&candidate) {
                    return Some(img);
                }
            }
        }
    }

    // Fallback: the first image file we find in the folder.
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_file() {
            let is_image = p
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| IMAGE_EXTS.contains(&e.to_lowercase().as_str()))
                .unwrap_or(false);
            if is_image {
                if let Some(img) = read_image_bytes(&p) {
                    return Some(img);
                }
            }
        }
    }
    None
}

/// Cover art bytes + MIME for a track: embedded art first, then a standalone
/// image in the track's folder. Shared by `get_cover` (UI, as a data URI) and
/// the `/cover` loopback route (MPRIS art, which needs a fetchable URL).
pub(crate) fn cover_bytes(p: &Path) -> Option<(Vec<u8>, String)> {
    if let Ok(tagged) = read_from_path(p) {
        if let Some(tag) = tagged.primary_tag().or_else(|| tagged.first_tag()) {
            if let Some(pic) = tag.pictures().first() {
                let mime = pic
                    .mime_type()
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_else(|| "image/jpeg".to_string());
                return Some((pic.data().to_vec(), mime));
            }
        }
    }
    folder_cover_bytes(p)
}

/// Cover art for a single track as a base64 data URI: embedded art first, then
/// a standalone image in the track's folder. Loaded lazily (per displayed/
/// playing track) so large libraries stay light on memory.
#[tauri::command]
fn get_cover(path: String) -> Option<String> {
    cover_bytes(Path::new(&path))
        .map(|(bytes, mime)| format!("data:{mime};base64,{}", STANDARD.encode(bytes)))
}

// ---- Crash / error logging --------------------------------------------------

/// Per-user log file: %LOCALAPPDATA%\meusic\meusic.log on Windows, and
/// $XDG_DATA_HOME (or ~/.local/share) /meusic/meusic.log on Linux/macOS.
/// Falls back to the cwd if no suitable base dir is found.
fn log_path() -> PathBuf {
    let base = std::env::var("LOCALAPPDATA")
        .ok()
        .or_else(|| std::env::var("XDG_DATA_HOME").ok())
        .or_else(|| std::env::var("HOME").ok().map(|h| format!("{h}/.local/share")))
        .unwrap_or_else(|| ".".to_string());
    let dir = Path::new(&base).join("meusic");
    let _ = std::fs::create_dir_all(&dir);
    dir.join("meusic.log")
}

/// Append a timestamped line to the log file (best-effort, never panics).
pub(crate) fn log_line(line: &str) {
    let ts = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(log_path()) {
        let _ = writeln!(f, "{ts} {line}");
    }
}

/// Capture Rust panics into the log (instead of a silent crash) for monitoring.
fn install_panic_hook() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        log_line(&format!("[PANIC] {info}"));
        default(info);
    }));
}

/// Frontend-reported error/event, written to the same log.
#[tauri::command]
fn log_event(level: String, message: String) {
    log_line(&format!("[{level}] {message}"));
}

/// Show / focus / unminimize the main window (from tray menu or mini-player).
fn reveal_main(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// Toggle the tray mini-player popup, positioning it at the bottom-right of the
/// primary monitor (just above the taskbar / tray area).
fn toggle_mini(app: &tauri::AppHandle) {
    let Some(mini) = app.get_webview_window("miniplayer") else {
        return;
    };
    if mini.is_visible().unwrap_or(false) {
        let _ = mini.hide();
        return;
    }
    if let Ok(Some(mon)) = mini.primary_monitor() {
        let msize = mon.size();
        let mpos = mon.position();
        let wsize = mini
            .outer_size()
            .unwrap_or(tauri::PhysicalSize::new(320, 300));
        let margin = 12i32;
        let taskbar = 56i32;
        let x = mpos.x + msize.width as i32 - wsize.width as i32 - margin;
        let y = mpos.y + msize.height as i32 - wsize.height as i32 - taskbar;
        let _ = mini.set_position(tauri::PhysicalPosition::new(x, y));
    }
    let _ = mini.show();
    let _ = mini.set_focus();
}

/// Toggle the system-tray icon's visibility (honors the user setting).
#[tauri::command]
fn set_tray_visible(app: tauri::AppHandle, visible: bool) {
    if let Some(tray) = app.tray_by_id("main") {
        let _ = tray.set_visible(visible);
    }
}

// ---- Persistent key/value store (file-backed) -------------------------------
// Settings/session are written to disk so they survive an OS shutdown, which can
// kill the process before WebView2 flushes localStorage to disk.

/// Path to `<name>.json` in the per-app config dir (%APPDATA%\com.sarta.meusic).
fn store_path(app: &tauri::AppHandle, name: &str) -> Option<PathBuf> {
    let dir = app.path().app_config_dir().ok()?;
    let _ = std::fs::create_dir_all(&dir);
    Some(dir.join(format!("{name}.json")))
}

/// Read a named store's contents (None if it doesn't exist yet).
#[tauri::command]
fn load_store(app: tauri::AppHandle, name: String) -> Option<String> {
    let p = store_path(&app, &name)?;
    std::fs::read_to_string(p).ok()
}

/// Write a named store's contents to disk (immediately, no buffering).
#[tauri::command]
fn save_store(app: tauri::AppHandle, name: String, contents: String) -> Result<(), String> {
    let p = store_path(&app, &name).ok_or("no config dir")?;
    std::fs::write(p, contents).map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    install_panic_hook();
    log_line("[INFO] app start");
    tauri::Builder::default()
        // Single-instance must be registered first: a second launch forwards to
        // this callback (reveal the existing window) instead of opening anew.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            reveal_main(app);
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let show = MenuItem::with_id(app, "show", "Tampilkan meusic", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Keluar", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &quit])?;

            let mut builder = TrayIconBuilder::with_id("main")
                .tooltip("meusic")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => reveal_main(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        toggle_mini(tray.app_handle());
                    }
                });
            // Guard against a missing icon instead of unwrap()-panicking on startup.
            if let Some(icon) = app.default_window_icon() {
                builder = builder.icon(icon.clone());
            }
            builder.build(app)?;

            // Start the internet-radio streaming proxy (loopback HTTP server).
            radio::start(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            scan_folder,
            get_cover,
            set_tray_visible,
            log_event,
            load_store,
            save_store,
            radio::radio_proxy_port
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
