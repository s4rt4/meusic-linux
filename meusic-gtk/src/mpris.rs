//! Native MPRIS2 service for the GNOME media popup — full metadata incl. a real
//! `file://` cover-art URL, and transport controls wired back to the app via a
//! callback. Runs a zbus connection on a dedicated thread; the app updates the
//! exposed properties from the main thread via `update()` / `set_position()`.

use std::collections::HashMap;
use std::future::pending;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, OnceLock};

use zbus::connection;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::{ObjectPath, OwnedValue, Value};
use zbus::Connection;

/// Transport actions the popup / media keys can send us.
#[derive(Debug, Clone, Copy)]
pub enum Control {
    PlayPause,
    Play,
    Pause,
    Stop,
    Next,
    Previous,
    Raise,
    Quit,
}

type Cb = Arc<dyn Fn(Control) + Send + Sync>;

const OBJ_PATH: &str = "/org/mpris/MediaPlayer2";
static CONNECTION: OnceLock<Connection> = OnceLock::new();
static POSITION: OnceLock<Arc<AtomicI64>> = OnceLock::new();

// ---- org.mpris.MediaPlayer2 (root) -----------------------------------------

struct Root {
    cb: Cb,
}

#[zbus::interface(name = "org.mpris.MediaPlayer2")]
impl Root {
    fn raise(&self) {
        (self.cb)(Control::Raise);
    }
    fn quit(&self) {
        (self.cb)(Control::Quit);
    }
    #[zbus(property)]
    fn can_quit(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn can_raise(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn has_track_list(&self) -> bool {
        false
    }
    #[zbus(property)]
    fn identity(&self) -> String {
        "meusic".into()
    }
    #[zbus(property)]
    fn desktop_entry(&self) -> String {
        "meusic".into()
    }
    #[zbus(property)]
    fn supported_uri_schemes(&self) -> Vec<String> {
        vec![]
    }
    #[zbus(property)]
    fn supported_mime_types(&self) -> Vec<String> {
        vec![]
    }
}

// ---- org.mpris.MediaPlayer2.Player -----------------------------------------

struct Player {
    cb: Cb,
    pos: Arc<AtomicI64>,
    status: String, // "Playing" | "Paused" | "Stopped"
    title: String,
    artist: String,
    album: String,
    art_url: Option<String>,
    trackid: String,
    length_micros: i64,
}

impl Player {
    fn send(&self, c: Control) {
        (self.cb)(c);
    }
}

#[zbus::interface(name = "org.mpris.MediaPlayer2.Player")]
impl Player {
    fn next(&self) {
        self.send(Control::Next);
    }
    fn previous(&self) {
        self.send(Control::Previous);
    }
    fn pause(&self) {
        self.send(Control::Pause);
    }
    fn play_pause(&self) {
        self.send(Control::PlayPause);
    }
    fn stop(&self) {
        self.send(Control::Stop);
    }
    fn play(&self) {
        self.send(Control::Play);
    }
    fn seek(&self, _offset: i64) {}
    fn set_position(&self, _track: ObjectPath<'_>, _position: i64) {}
    fn open_uri(&self, _uri: String) {}

    #[zbus(property)]
    fn playback_status(&self) -> String {
        self.status.clone()
    }

    #[zbus(property)]
    fn metadata(&self) -> HashMap<String, OwnedValue> {
        let mut m: HashMap<String, OwnedValue> = HashMap::new();
        let tid = ObjectPath::try_from(self.trackid.clone())
            .unwrap_or_else(|_| ObjectPath::try_from("/org/mpris/MediaPlayer2/track/0").unwrap());
        let mut put = |k: &str, v: Value<'_>| {
            if let Ok(owned) = v.try_to_owned() {
                m.insert(k.to_string(), owned);
            }
        };
        put("mpris:trackid", Value::from(tid));
        put("mpris:length", Value::from(self.length_micros));
        if let Some(art) = &self.art_url {
            put("mpris:artUrl", Value::from(art.clone()));
        }
        put("xesam:title", Value::from(self.title.clone()));
        put("xesam:artist", Value::from(vec![self.artist.clone()]));
        put("xesam:album", Value::from(self.album.clone()));
        m
    }

    #[zbus(property)]
    fn position(&self) -> i64 {
        self.pos.load(Ordering::Relaxed)
    }
    #[zbus(property)]
    fn rate(&self) -> f64 {
        1.0
    }
    #[zbus(property)]
    fn minimum_rate(&self) -> f64 {
        1.0
    }
    #[zbus(property)]
    fn maximum_rate(&self) -> f64 {
        1.0
    }
    #[zbus(property)]
    fn volume(&self) -> f64 {
        1.0
    }
    #[zbus(property)]
    fn set_volume(&mut self, _volume: f64) {}
    #[zbus(property)]
    fn can_go_next(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn can_go_previous(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn can_play(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn can_pause(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn can_seek(&self) -> bool {
        false
    }
    #[zbus(property)]
    fn can_control(&self) -> bool {
        true
    }
}

// ---- service plumbing ------------------------------------------------------

fn make_trackid(path: Option<&str>) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    match path {
        Some(p) => {
            let mut h = DefaultHasher::new();
            p.hash(&mut h);
            format!("/org/mpris/MediaPlayer2/track/{:016x}", h.finish())
        }
        None => "/org/mpris/MediaPlayer2/track/none".to_string(),
    }
}

async fn build(cb: Cb, pos: Arc<AtomicI64>) -> zbus::Result<Connection> {
    let player = Player {
        cb: cb.clone(),
        pos,
        status: "Stopped".into(),
        title: String::new(),
        artist: String::new(),
        album: String::new(),
        art_url: None,
        trackid: "/org/mpris/MediaPlayer2/track/0".into(),
        length_micros: 0,
    };
    connection::Builder::session()?
        .name("org.mpris.MediaPlayer2.meusic")?
        .serve_at(OBJ_PATH, Root { cb })?
        .serve_at(OBJ_PATH, player)?
        .build()
        .await
}

/// Register the MPRIS service. `cb` receives transport actions (called from the
/// D-Bus thread). Best-effort: logs and no-ops if there's no session bus.
pub fn start(cb: impl Fn(Control) + Send + Sync + 'static) {
    let cb: Cb = Arc::new(cb);
    let pos = Arc::new(AtomicI64::new(0));
    let _ = POSITION.set(pos.clone());
    std::thread::spawn(move || {
        zbus::block_on(async move {
            match build(cb, pos).await {
                Ok(conn) => {
                    let _ = CONNECTION.set(conn);
                    pending::<()>().await;
                }
                Err(e) => eprintln!("meusic: mpris unavailable: {e}"),
            }
        });
    });
}

/// Push now-playing metadata + playback status to the popup. `path` is the track
/// file (None for nothing playing); its cover becomes the `file://` artUrl.
pub fn update(
    title: &str,
    artist: &str,
    album: &str,
    length_secs: f64,
    status: &str,
    art_url: Option<String>,
    trackid_seed: Option<&str>,
) {
    let Some(conn) = CONNECTION.get() else {
        return;
    };
    let trackid = make_trackid(trackid_seed);
    let length_micros = (length_secs.max(0.0) * 1_000_000.0) as i64;
    let (title, artist, album, status) = (
        title.to_string(),
        artist.to_string(),
        album.to_string(),
        status.to_string(),
    );
    let conn = conn.clone();
    let res: zbus::Result<()> = zbus::block_on(async move {
        let iref = conn.object_server().interface::<_, Player>(OBJ_PATH).await?;
        let mut p = iref.get_mut().await;
        p.title = title;
        p.artist = artist;
        p.album = album;
        p.length_micros = length_micros;
        p.status = status;
        p.art_url = art_url;
        p.trackid = trackid;
        let emitter: &SignalEmitter<'_> = iref.signal_emitter();
        p.metadata_changed(emitter).await?;
        p.playback_status_changed(emitter).await?;
        Ok(())
    });
    if let Err(e) = res {
        eprintln!("meusic: mpris update: {e}");
    }
}

/// Update the reported playback position (seconds). Cheap atomic store.
pub fn set_position(secs: f64) {
    if let Some(p) = POSITION.get() {
        p.store((secs.max(0.0) * 1_000_000.0) as i64, Ordering::Relaxed);
    }
}
