//! A tiny `glib::Object` wrapper around a `Track`, so tracks can live in a
//! `gio::ListStore` and be virtualized by a `gtk::ListView` (only the visible
//! rows get widgets — the whole library no longer builds N row widgets).

use crate::library::Track;
use relm4::gtk::glib;
use relm4::gtk::subclass::prelude::*;
use std::cell::RefCell;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct TrackObject {
        pub track: RefCell<Option<Track>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for TrackObject {
        const NAME: &'static str = "MeusicTrackObject";
        type Type = super::TrackObject;
    }

    impl ObjectImpl for TrackObject {}
}

glib::wrapper! {
    pub struct TrackObject(ObjectSubclass<imp::TrackObject>);
}

impl TrackObject {
    pub fn new(track: Track) -> Self {
        let obj: Self = glib::Object::new();
        obj.imp().track.replace(Some(track));
        obj
    }

    /// The wrapped track (clone — cheap relative to the row work it drives).
    pub fn track(&self) -> Track {
        self.imp()
            .track
            .borrow()
            .clone()
            .expect("TrackObject always holds a track")
    }
}
