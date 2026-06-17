//! Audio playback via GStreamer `playbin3`. Thin wrapper exposing the controls
//! the UI needs: load/play/pause/seek/volume + position/duration queries. The
//! bus (for end-of-stream → advance) is exposed so the UI can watch it.

use gstreamer as gst;
use gstreamer::prelude::*;

pub struct Player {
    playbin: gst::Element,
    eq: Option<gst::Element>,
    spectrum: Option<gst::Element>,
}

impl Player {
    pub fn new() -> Self {
        let playbin = gst::ElementFactory::make("playbin3")
            .build()
            .expect("playbin3 (gstreamer-plugins-base) is required");
        // Audio filter chain: 10-band equalizer (EQ popover) → spectrum analyser
        // (posts magnitude messages on the bus for the visualizer). Built as a
        // bin with auto ghost pads so it slots into playbin's `audio-filter`.
        let filter = gst::parse::bin_from_description(
            "equalizer-10bands name=meq ! spectrum name=msp bands=128 threshold=-65 \
             interval=30000000 post-messages=true",
            true,
        )
        .ok();
        let eq = filter.as_ref().and_then(|b| b.by_name("meq"));
        let spectrum = filter.as_ref().and_then(|b| b.by_name("msp"));
        if let Some(f) = &filter {
            playbin.set_property("audio-filter", f);
        }
        // Enable ICY/shoutcast metadata on the http source so radio streams post
        // their current song title as tag messages.
        playbin.connect("source-setup", false, |args| {
            if let Some(src) = args.get(1).and_then(|v| v.get::<gst::Element>().ok()) {
                if src.list_properties().iter().any(|p| p.name() == "iradio-mode") {
                    src.set_property("iradio-mode", true);
                }
            }
            None
        });
        Player { playbin, eq, spectrum }
    }

    /// Enable/disable the spectrum analyser's bus messages — turned off in
    /// power-save mode to skip the periodic FFT (real CPU saving, not just redraw).
    pub fn set_spectrum_enabled(&self, on: bool) {
        if let Some(sp) = &self.spectrum {
            sp.set_property("post-messages", on);
        }
    }

    /// Point the player at a stream URL (internet radio) and play from live.
    pub fn load_url(&self, uri: &str) {
        let _ = self.playbin.set_state(gst::State::Null);
        self.playbin.set_property("uri", uri);
    }

    /// Set one equalizer band's gain in dB (band index 0..=9, gain -24..=12).
    pub fn set_eq_band(&self, band: u32, gain_db: f64) {
        if let Some(eq) = &self.eq {
            eq.set_property(&format!("band{band}"), gain_db.clamp(-24.0, 12.0));
        }
    }

    /// The playbin's bus — watch it for EOS / errors.
    pub fn bus(&self) -> gst::Bus {
        self.playbin.bus().expect("playbin has a bus")
    }

    /// Point the player at a local file and start from the top.
    pub fn load(&self, path: &str) {
        let _ = self.playbin.set_state(gst::State::Null);
        if let Ok(uri) = gst::glib::filename_to_uri(path, None) {
            self.playbin.set_property("uri", uri.as_str());
        }
    }

    pub fn play(&self) {
        let _ = self.playbin.set_state(gst::State::Playing);
    }

    /// Whether the pipeline has actually reached the Playing state.
    pub fn is_active(&self) -> bool {
        self.playbin.current_state() == gst::State::Playing
    }

    pub fn pause(&self) {
        let _ = self.playbin.set_state(gst::State::Paused);
    }

    pub fn stop(&self) {
        let _ = self.playbin.set_state(gst::State::Null);
    }

    pub fn set_volume(&self, v: f64) {
        self.playbin.set_property("volume", v.clamp(0.0, 1.0));
    }

    pub fn seek(&self, secs: f64) {
        let _ = self.playbin.seek_simple(
            gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT,
            gst::ClockTime::from_mseconds((secs.max(0.0) * 1000.0) as u64),
        );
    }

    /// Current playback position in seconds (0 if unknown).
    pub fn position(&self) -> f64 {
        self.playbin
            .query_position::<gst::ClockTime>()
            .map(|c| c.mseconds() as f64 / 1000.0)
            .unwrap_or(0.0)
    }

    /// Total stream duration in seconds (0 if not yet known).
    pub fn duration(&self) -> f64 {
        self.playbin
            .query_duration::<gst::ClockTime>()
            .map(|c| c.mseconds() as f64 / 1000.0)
            .unwrap_or(0.0)
    }
}
