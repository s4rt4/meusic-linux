//! Native MPRIS2 MediaPlayer2 service for the GNOME/Linux media popup.
//!
//! webkit2gtk's built-in MPRIS bridge forwards title/artist/album but DROPS the
//! cover art: it refuses to expose a `file://` artUrl from the page's `http://`
//! origin (security), and GNOME won't render a loopback-`http://` art URL. So on
//! Linux meusic exposes its OWN MPRIS service here, with full control of the
//! Metadata (incl. a real `file://` artUrl) and the playback controls wired back
//! to the React player via a Tauri event.
//!
//! NOTE: webkit still publishes its own (art-less) MPRIS player for the played
//! audio and there's no app-side way to disable it (see the note in lib.rs), so
//! the GNOME popup shows our player alongside webkit's redundant clone.
//!
//! The heavy lifting is Linux-only; the two Tauri commands are compiled on every
//! platform (so `generate_handler!` can list them unconditionally) but no-op off
//! Linux.

/// Push the current now-playing metadata + playback state to the MPRIS service.
/// `path` is the track's file path (None for radio) — the backend extracts its
/// cover to a temp file for the `file://` artUrl. No-op when MPRIS is off.
#[tauri::command]
#[allow(unused_variables)]
pub fn mpris_update(
    title: String,
    artist: String,
    album: String,
    length_secs: f64,
    status: String,
    path: Option<String>,
) {
    #[cfg(target_os = "linux")]
    imp::update(title, artist, album, length_secs, status, path);
}

/// Update the MPRIS playback position (seconds). Cheap — just stores an atomic,
/// no D-Bus signal (Position isn't emitted via PropertiesChanged per the spec;
/// clients poll it / extrapolate). No-op when MPRIS is off.
#[tauri::command]
#[allow(unused_variables)]
pub fn mpris_position(secs: f64) {
    #[cfg(target_os = "linux")]
    imp::set_position(secs);
}

/// Register the MPRIS service on the session bus (Linux only). Best-effort: if
/// there's no session bus (e.g. headless) it logs and the commands stay no-ops.
#[cfg(target_os = "linux")]
pub fn start(app: tauri::AppHandle) {
    imp::start(app);
}

#[cfg(target_os = "linux")]
mod imp {
    use std::collections::hash_map::DefaultHasher;
    use std::collections::HashMap;
    use std::future::pending;
    use std::hash::{Hash, Hasher};
    use std::sync::atomic::{AtomicI64, Ordering};
    use std::sync::{Arc, OnceLock};

    use tauri::{AppHandle, Emitter, Manager};
    use zbus::connection;
    use zbus::object_server::SignalEmitter;
    use zbus::zvariant::{ObjectPath, OwnedValue, Value};
    use zbus::Connection;

    const OBJ_PATH: &str = "/org/mpris/MediaPlayer2";

    static CONNECTION: OnceLock<Connection> = OnceLock::new();
    static POSITION: OnceLock<Arc<AtomicI64>> = OnceLock::new();

    // ---- org.mpris.MediaPlayer2 (root) --------------------------------------

    struct Root {
        app: AppHandle,
    }

    #[zbus::interface(name = "org.mpris.MediaPlayer2")]
    impl Root {
        fn raise(&self) {
            if let Some(w) = self.app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
        }

        fn quit(&self) {
            self.app.exit(0);
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

        // Matches the installed meusic.desktop basename, so GNOME shows our app
        // icon in the popup. See scripts/install-linux-desktop.sh.
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

    // ---- org.mpris.MediaPlayer2.Player --------------------------------------

    struct Player {
        app: AppHandle,
        pos: Arc<AtomicI64>,
        status: String, // "Playing" | "Paused" | "Stopped"
        title: String,
        artist: String,
        album: String,
        art_url: Option<String>,
        trackid: String,
        length_micros: i64,
        volume: f64,
    }

    impl Player {
        /// Forward a control action to the React player (App.tsx listens on "mpris").
        fn send(&self, action: &str) {
            let _ = self.app.emit("mpris", action);
        }
    }

    #[zbus::interface(name = "org.mpris.MediaPlayer2.Player")]
    impl Player {
        fn next(&self) {
            self.send("next");
        }
        fn previous(&self) {
            self.send("previous");
        }
        fn pause(&self) {
            self.send("pause");
        }
        fn play_pause(&self) {
            self.send("playpause");
        }
        fn stop(&self) {
            self.send("stop");
        }
        fn play(&self) {
            self.send("play");
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
            let tid = ObjectPath::try_from(self.trackid.clone()).unwrap_or_else(|_| {
                ObjectPath::try_from("/org/mpris/MediaPlayer2/track/0").unwrap()
            });
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
            self.volume
        }
        #[zbus(property)]
        fn set_volume(&mut self, _volume: f64) {
            // meusic's own UI owns the volume; ignore external sets.
        }

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

    // ---- service plumbing ---------------------------------------------------

    /// A stable D-Bus object path per track so GNOME's URI-keyed art cache
    /// refreshes when the track changes.
    fn make_trackid(path: Option<&str>) -> String {
        match path {
            Some(p) => {
                let mut h = DefaultHasher::new();
                p.hash(&mut h);
                format!("/org/mpris/MediaPlayer2/track/{:016x}", h.finish())
            }
            None => "/org/mpris/MediaPlayer2/track/stream".to_string(),
        }
    }

    pub fn set_position(secs: f64) {
        if let Some(p) = POSITION.get() {
            p.store((secs.max(0.0) * 1_000_000.0) as i64, Ordering::Relaxed);
        }
    }

    pub fn update(
        title: String,
        artist: String,
        album: String,
        length_secs: f64,
        status: String,
        path: Option<String>,
    ) {
        let Some(conn) = CONNECTION.get() else {
            return;
        };
        let art_url = path.as_deref().and_then(crate::art_file_url);
        let trackid = make_trackid(path.as_deref());
        let length_micros = (length_secs.max(0.0) * 1_000_000.0) as i64;
        let conn = conn.clone();
        let res: zbus::Result<()> = zbus::block_on(async move {
            let iref = conn
                .object_server()
                .interface::<_, Player>(OBJ_PATH)
                .await?;
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
            crate::log_line(&format!("[WARN] mpris update: {e}"));
        }
    }

    async fn build(app: AppHandle, pos: Arc<AtomicI64>) -> zbus::Result<Connection> {
        let player = Player {
            app: app.clone(),
            pos,
            status: "Stopped".into(),
            title: String::new(),
            artist: String::new(),
            album: String::new(),
            art_url: None,
            trackid: "/org/mpris/MediaPlayer2/track/0".into(),
            length_micros: 0,
            volume: 1.0,
        };
        connection::Builder::session()?
            .name("org.mpris.MediaPlayer2.meusic")?
            .serve_at(OBJ_PATH, Root { app })?
            .serve_at(OBJ_PATH, player)?
            .build()
            .await
    }

    pub fn start(app: AppHandle) {
        let pos = Arc::new(AtomicI64::new(0));
        let _ = POSITION.set(pos.clone());
        std::thread::spawn(move || {
            zbus::block_on(async move {
                match build(app, pos).await {
                    Ok(conn) => {
                        let _ = CONNECTION.set(conn);
                        crate::log_line("[INFO] mpris service registered");
                        pending::<()>().await;
                    }
                    Err(e) => crate::log_line(&format!("[WARN] mpris unavailable: {e}")),
                }
            });
        });
    }
}
