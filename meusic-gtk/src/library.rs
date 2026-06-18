//! Music library scanning + tag/cover extraction (ported from the Tauri backend,
//! src-tauri/src/lib.rs). Pure Rust, no GTK — uses lofty for tags, walkdir to
//! recurse, rayon to read files in parallel.

use lofty::prelude::*;
use lofty::read_from_path;
use lofty::tag::ItemKey;
use rayon::prelude::*;
use std::path::Path;
use walkdir::WalkDir;

/// Metadata for a single audio track.
#[derive(Clone, Debug)]
pub struct Track {
    pub path: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_artist: String,
    pub track_no: u32,
    pub duration: u64, // seconds
    pub format: String, // e.g. "FLAC", "MP3"
    pub bitrate: u32,   // kbps, 0 if unknown
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
            if let Some(aa) = tag.get_string(ItemKey::AlbumArtist) {
                album_artist = aa.to_string();
            }
            if let Some(tn) = tag.track() {
                track_no = tn;
            }
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
        format,
        bitrate,
    }
}

/// Recursively scan a folder for audio files, read their metadata in parallel,
/// and return them sorted by album-artist → album → track number → title.
pub fn scan_folder(path: &str) -> Vec<Track> {
    let files: Vec<_> = WalkDir::new(path)
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

/// A sidebar grouping entry (a folder, album, or artist) + its track count.
#[derive(Clone, Debug)]
pub struct Group {
    pub label: String,
    pub key: String,
    pub count: usize,
}

/// The absolute parent-directory path of a track (its containing folder).
pub fn parent_dir(path: &str) -> String {
    Path::new(path)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default()
}

fn dir_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(path)
        .to_string()
}

fn group_by<F: Fn(&Track) -> (String, String)>(tracks: &[Track], f: F) -> Vec<Group> {
    use std::collections::HashMap;
    let mut map: HashMap<String, (String, usize)> = HashMap::new();
    for t in tracks {
        let (key, label) = f(t);
        let e = map.entry(key).or_insert((label, 0));
        e.1 += 1;
    }
    let mut groups: Vec<Group> = map
        .into_iter()
        .map(|(key, (label, count))| Group { label, key, count })
        .collect();
    groups.sort_by(|a, b| a.label.to_lowercase().cmp(&b.label.to_lowercase()));
    groups
}

/// Folders that directly contain audio (label = folder name, key = full path).
pub fn folder_groups(tracks: &[Track]) -> Vec<Group> {
    group_by(tracks, |t| {
        let dir = parent_dir(&t.path);
        let name = dir_name(&dir);
        (dir, name)
    })
}

pub fn album_groups(tracks: &[Track]) -> Vec<Group> {
    group_by(tracks, |t| (t.album.clone(), t.album.clone()))
}

pub fn artist_groups(tracks: &[Track]) -> Vec<Group> {
    group_by(tracks, |t| (t.album_artist.clone(), t.album_artist.clone()))
}

const IMAGE_EXTS: &[&str] = &["jpg", "jpeg", "png", "webp", "bmp", "gif"];
const COVER_NAMES: &[&str] = &[
    "cover", "folder", "front", "album", "albumart", "albumartsmall", "thumb", "artwork",
];

fn read_image_bytes(path: &Path) -> Option<Vec<u8>> {
    let bytes = std::fs::read(path).ok()?;
    if bytes.is_empty() {
        return None;
    }
    Some(bytes)
}

/// Look beside the audio file for a standalone cover image — common well-known
/// names first (cover.jpg, folder.jpg, …), then any image in the folder.
fn folder_cover_bytes(audio_path: &Path) -> Option<Vec<u8>> {
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
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let p = entry.path();
        let is_image = p
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| IMAGE_EXTS.contains(&e.to_lowercase().as_str()))
            .unwrap_or(false);
        if p.is_file() && is_image {
            if let Some(img) = read_image_bytes(&p) {
                return Some(img);
            }
        }
    }
    None
}

/// Cover-art bytes for a track: embedded picture first, then a standalone image
/// in the track's folder. Encoded bytes (JPEG/PNG/…), ready to decode.
pub fn cover_bytes(path: &str) -> Option<Vec<u8>> {
    let p = Path::new(path);
    if let Ok(tagged) = read_from_path(p) {
        if let Some(tag) = tagged.primary_tag().or_else(|| tagged.first_tag()) {
            if let Some(pic) = tag.pictures().first() {
                return Some(pic.data().to_vec());
            }
        }
    }
    folder_cover_bytes(p)
}

fn detect_ext(bytes: &[u8]) -> &'static str {
    match bytes {
        [0x89, b'P', b'N', b'G', ..] => "png",
        [b'G', b'I', b'F', b'8', ..] => "gif",
        [0x42, 0x4D, ..] => "bmp",
        b if b.len() > 11 && &b[0..4] == b"RIFF" && &b[8..12] == b"WEBP" => "webp",
        _ => "jpg",
    }
}

/// Percent-encode an absolute path into a `file://` URL (keeps `/`, encodes the
/// rest) so GNOME/Gio can parse it.
pub(crate) fn file_uri(p: &Path) -> String {
    let s = p.to_string_lossy();
    let mut out = String::from("file://");
    for &b in s.as_bytes() {
        match b {
            b'/' => out.push('/'),
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Directory holding the temp cover/station art written for MPRIS artUrls.
fn art_temp_dir() -> std::path::PathBuf {
    std::env::temp_dir().join("meusic-art")
}

/// Write already-read cover bytes to a temp file (hashed by source path) and
/// return a `file://` URL for the MPRIS `mpris:artUrl`. Takes the bytes the
/// caller already decoded so the cover isn't re-read per play/pause.
pub fn art_file_from_bytes(path: &str, bytes: &[u8]) -> Option<String> {
    use std::hash::{Hash, Hasher};
    let ext = detect_ext(bytes);
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut hasher);
    let dir = art_temp_dir();
    let _ = std::fs::create_dir_all(&dir);
    let file = dir.join(format!("{:016x}.{ext}", hasher.finish()));
    if !file.exists() {
        std::fs::write(&file, bytes).ok()?;
    }
    Some(file_uri(&file))
}

/// Delete temp art files older than a week so the `meusic-art` dir doesn't grow
/// without bound over a library's lifetime (files regenerate on demand).
pub fn prune_temp_art() {
    let Ok(rd) = std::fs::read_dir(art_temp_dir()) else {
        return;
    };
    let now = std::time::SystemTime::now();
    const WEEK: u64 = 7 * 24 * 3600;
    for entry in rd.flatten() {
        let stale = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| now.duration_since(t).ok())
            .map(|age| age.as_secs() > WEEK)
            .unwrap_or(false);
        if stale {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// Human total duration, e.g. "7 jam 23 menit" or "23 menit".
pub fn fmt_total(secs: u64) -> String {
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    if h > 0 {
        format!("{h} jam {m} menit")
    } else if m > 0 {
        format!("{m} menit")
    } else {
        format!("{secs} detik")
    }
}

/// Format a duration in seconds as `m:ss` (or `h:mm:ss`).
pub fn fmt_time(secs: u64) -> String {
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}
