//! meusic — native GTK4 / libadwaita rewrite (relm4).
//!
//! Phase 5: UI revision toward the original meusic design — logo + mode tabs +
//! search + menu in the header, a two-pane body (sidebar groups + song list),
//! and a Dopamine-style bottom bar (cover, title/artist, format/bitrate badge,
//! repeat/prev/play/next/shuffle, EQ, volume). Icons are the app's own Lucide
//! set, registered as a custom icon theme. Adaptive gradient + MPRIS from before.

mod art;
mod library;
mod mpris;
mod player;
mod session;
mod settings;
mod stations;
mod track_object;
mod util;

use library::{album_groups, artist_groups, cover_bytes, fmt_time, folder_groups, parent_dir,
              scan_folder, Track};
use player::Player;
use relm4::adw::prelude::*;
use relm4::gtk::{gdk, gio, glib, pango};
use track_object::TrackObject;
use relm4::{adw, gtk, ComponentParts, ComponentSender, RelmApp, SimpleComponent};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

const STYLE: &str = r#"
headerbar, toolbarview, scrolledwindow, scrolledwindow > viewport, list, list > row {
    background-color: transparent;
}
headerbar { box-shadow: none; min-height: 54px; }
.glasscard {
    background-color: rgba(20, 20, 28, 0.5);
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 16px;
}
list > row { border-radius: 10px; }
list > row:hover { background-color: rgba(255, 255, 255, 0.06); }
listview, listview > row { background-color: transparent; }
listview > row { border-radius: 10px; padding: 0; }
listview > row:hover { background-color: rgba(255, 255, 255, 0.06); }
.songrow { border-radius: 10px; }
.songrow.playing { background-color: rgba(255, 255, 255, 0.14); }
.tab { padding: 4px 12px; border-radius: 8px; color: alpha(white, 0.55); }
.tab.active { color: @accent_color; box-shadow: inset 0 -3px 0 @accent_bg_color; }
.ctl { color: alpha(white, 0.6); }
.ctl.active { color: @accent_color; }
.leaf-on { color: #44aa00; }
.fmt-badge {
    background-color: alpha(@accent_bg_color, 0.30);
    color: @accent_color;
    border-radius: 5px;
    padding: 1px 6px;
    font-size: 0.7em;
    font-weight: 800;
}
.nowbar { background-color: rgba(15, 15, 22, 0.62); border-top: 1px solid rgba(255,255,255,0.08); }
.cover { border-radius: 8px; background-color: rgba(255,255,255,0.06); }
.play {
    background-color: @accent_bg_color;
    color: #ffffff;
    min-width: 22px;
    min-height: 22px;
    padding: 10px;
    box-shadow: 0 4px 14px -4px @accent_bg_color;
}
.play:hover { filter: brightness(1.1); }
.play:disabled { background-color: alpha(white, 0.12); }
.count { color: alpha(white, 0.4); font-size: 0.85em; }
.searchbar { border-radius: 999px; }
.np { background-color: transparent; }
.np-title { font-size: 2.4rem; font-weight: 800; }
.npcover { border-radius: 22px; }
.coverbtn { padding: 0; border-radius: 8px; }
.volpct {
    background-color: rgba(0, 0, 0, 0.55);
    color: #ffffff;
    border-radius: 6px;
    padding: 1px 5px;
    font-weight: 700;
    font-size: 0.8em;
}
"#;

type PaletteF = Vec<(f64, f64, f64)>;

// EQ: 6 UI sliders mapped onto bands of GStreamer's equalizer-10bands.
const EQ_FREQS: [&str; 6] = ["60", "170", "350", "1k", "3.5k", "10k"];
const EQ_BAND_MAP: [u32; 6] = [1, 2, 4, 5, 7, 8];

// Radio reconnect backoff (seconds); the last value repeats so a long outage
// keeps being retried (ported from the Windows build's RADIO_BACKOFF).
const RADIO_BACKOFF: [u64; 5] = [2, 5, 10, 30, 60];
// ~12s of no playback progress (250ms ticks) while "playing" = a silent stall.
const RADIO_STALL_TICKS: u32 = 48;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Mode {
    Folders,
    Albums,
    Artists,
    Songs,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Repeat {
    Off,
    All,
    One,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum AppMode {
    Music,
    Radio,
}

#[derive(Clone, Copy, Debug)]
enum SettingField {
    Remember,
    ResumePage,
    FollowSong,
    TrayIcon,
    MinTray,
    CloseTray,
}

struct App {
    all_tracks: Vec<Track>,
    view_tracks: Vec<Track>,
    mode: Mode,
    sel_group: Option<String>,
    query: String,
    queue: Vec<Track>,
    qidx: Option<usize>,
    current_path: Option<String>,
    cur_art_url: Option<String>,
    root_path: Option<String>,
    pending_restore: Option<session::Session>,
    playing: bool,
    position: f64,
    duration: f64,
    scanning: bool,
    music_err_streak: u32,
    repeat: Repeat,
    shuffle: bool,
    np_open: bool,
    app_mode: AppMode,
    music_position: f64,
    pending_seek: Option<f64>,
    settings: settings::Settings,
    has_tray: bool,
    stations_data: Vec<stations::Station>,
    station_id: Option<String>,
    radio_title: Option<String>,
    radio_connecting: bool,
    radio_error: bool,
    radio_retries: u32,
    radio_reconnect_pending: bool,
    radio_last_pos: f64,
    radio_stall_ticks: u32,
    player: Player,
    song_view: gtk::ListView,
    song_model: gio::ListStore,
    sidebar: gtk::ListBox,
    stations_list: gtk::ListBox,
    radio_chip_state: Rc<RefCell<(String, (u8, u8, u8))>>,
    radio_vis: gtk::DrawingArea,
    seek: gtk::Scale,
    bg: gtk::DrawingArea,
    cover: gtk::Image,
    np_cover: gtk::Picture,
    np_bg: gtk::DrawingArea,
    accent_provider: gtk::CssProvider,
    palette: Rc<RefCell<PaletteF>>,
    sidebar_keys: Rc<RefCell<Vec<String>>>,
    sidebar_icons: Rc<RefCell<Vec<(String, gtk::Image)>>>,
    station_keys: Rc<RefCell<Vec<String>>>,
    station_rows: Rc<RefCell<Vec<(String, gtk::ListBoxRow)>>>,
    // Currently-bound (visible/recycled) song rows — the playing highlight + the
    // animated EQ indicator are applied over just these, not the whole library.
    bound_rows: Rc<RefCell<Vec<(String, gtk::Box, gtk::DrawingArea)>>>,
    current_path_shared: Rc<RefCell<Option<String>>>,
    playing_eq: Rc<RefCell<Option<gtk::DrawingArea>>>,
    playing_flag: Rc<Cell<bool>>,
    volume_pct: gtk::Label,
    volume_pct_gen: Rc<Cell<u64>>,
    logo_white: Option<gdk::Texture>,
    logo_green: Option<gdk::Texture>,
    power_save: bool,
    power_save_flag: Rc<Cell<bool>>,
    window: adw::ApplicationWindow,
    sender: ComponentSender<App>,
    _bus_watch: gstreamer::bus::BusWatchGuard,
}

#[derive(Debug)]
enum Msg {
    PickFolder,
    ScanStarted(String),
    Scanned(Vec<Track>),
    RestoreSession,
    SaveSession,
    SetMode(Mode),
    SelectGroup(String),
    Search(String),
    PlayIndex(usize),
    Toggle,
    SetPlaying(bool),
    Stop,
    Next,
    Prev,
    CycleRepeat,
    ToggleShuffle,
    SetEq(usize, f64),
    Seek(f64),
    SetVolume(f64),
    Raise,
    OpenNowPlaying,
    CloseNowPlaying,
    SetAppMode(AppMode),
    PlayStation(String),
    AddStation,
    EditStation(String),
    DeleteStation(String),
    SaveStation { id: Option<String>, name: String, url: String },
    RadioTitle(Option<String>),
    RadioError(String),
    ReconnectRadio,
    SetBool(SettingField, bool),
    TogglePowerSave,
    OpenAbout,
    CloseRequest,
    Quit,
    Tick,
}

fn tab_classes(active: bool) -> &'static [&'static str] {
    if active {
        &["flat", "tab", "active"]
    } else {
        &["flat", "tab"]
    }
}

fn ctl_classes(active: bool) -> &'static [&'static str] {
    if active {
        &["flat", "ctl", "active"]
    } else {
        &["flat", "ctl"]
    }
}

fn seg_classes(active: bool) -> &'static [&'static str] {
    if active {
        &["suggested-action"]
    } else {
        &["flat"]
    }
}

#[relm4::component]
impl SimpleComponent for App {
    type Init = ();
    type Input = Msg;
    type Output = ();

    view! {
        adw::ApplicationWindow {
            set_title: Some("meusic"),
            set_default_width: 1180,
            set_default_height: 760,

            // Intercept the window-manager close (X). Whether it minimizes (keep
            // playing) or actually quits is decided in update() from the current
            // "close to tray" setting — always Stop here so the default destroy
            // never races our decision.
            connect_close_request[sender] => move |_| {
                sender.input(Msg::CloseRequest);
                glib::Propagation::Stop
            },

            gtk::Overlay {
                #[local_ref]
                bg -> gtk::DrawingArea { set_hexpand: true, set_vexpand: true },

                add_overlay = &adw::ToolbarView {
                    add_top_bar = &adw::HeaderBar {
                        #[wrap(Some)]
                        set_title_widget = &gtk::Box {
                            add_css_class: "tabs",
                            set_spacing: 2,
                            #[watch]
                            set_visible: model.app_mode == AppMode::Music,

                            gtk::Button {
                                #[watch]
                                set_css_classes: tab_classes(model.mode == Mode::Folders),
                                #[name = "bc_folders"]
                                #[wrap(Some)]
                                set_child = &adw::ButtonContent {
                                    set_icon_name: "meusic-folder",
                                    set_label: "Folders",
                                },
                                connect_clicked => Msg::SetMode(Mode::Folders),
                            },
                            gtk::Button {
                                #[watch]
                                set_css_classes: tab_classes(model.mode == Mode::Albums),
                                #[name = "bc_albums"]
                                #[wrap(Some)]
                                set_child = &adw::ButtonContent {
                                    set_icon_name: "meusic-album",
                                    set_label: "Albums",
                                },
                                connect_clicked => Msg::SetMode(Mode::Albums),
                            },
                            gtk::Button {
                                #[watch]
                                set_css_classes: tab_classes(model.mode == Mode::Artists),
                                #[name = "bc_artists"]
                                #[wrap(Some)]
                                set_child = &adw::ButtonContent {
                                    set_icon_name: "meusic-artist",
                                    set_label: "Artists",
                                },
                                connect_clicked => Msg::SetMode(Mode::Artists),
                            },
                            gtk::Button {
                                #[watch]
                                set_css_classes: tab_classes(model.mode == Mode::Songs),
                                #[name = "bc_songs"]
                                #[wrap(Some)]
                                set_child = &adw::ButtonContent {
                                    set_icon_name: "meusic-music-note",
                                    set_label: "Songs",
                                },
                                connect_clicked => Msg::SetMode(Mode::Songs),
                            },
                        },

                        pack_start = &gtk::Button {
                            add_css_class: "flat",
                            set_tooltip_text: Some("Tentang meusic"),
                            set_margin_start: 4,
                            connect_clicked => Msg::OpenAbout,
                            #[wrap(Some)]
                            set_child = &gtk::Picture {
                                #[watch]
                                set_paintable: model.logo_paintable(),
                                set_content_fit: gtk::ContentFit::ScaleDown,
                                set_can_shrink: false,
                                set_halign: gtk::Align::Start,
                                set_size_request: (112, 26),
                            },
                        },

                        pack_end = &gtk::MenuButton {
                            set_icon_name: "meusic-menu",
                            add_css_class: "flat",
                            #[wrap(Some)]
                            set_popover = &gtk::Popover {
                                gtk::Box {
                                    set_orientation: gtk::Orientation::Vertical,
                                    set_spacing: 4,
                                    set_width_request: 280,
                                    set_margin_top: 8,
                                    set_margin_bottom: 8,
                                    set_margin_start: 8,
                                    set_margin_end: 8,

                                    gtk::Box {
                                        add_css_class: "linked",
                                        set_homogeneous: true,
                                        set_margin_bottom: 4,
                                        gtk::Button {
                                            #[watch]
                                            set_css_classes: seg_classes(model.app_mode == AppMode::Music),
                                            #[wrap(Some)]
                                            set_child = &adw::ButtonContent {
                                                set_icon_name: "meusic-music-note",
                                                set_label: "Music",
                                            },
                                            connect_clicked => Msg::SetAppMode(AppMode::Music),
                                        },
                                        gtk::Button {
                                            #[watch]
                                            set_css_classes: seg_classes(model.app_mode == AppMode::Radio),
                                            #[wrap(Some)]
                                            set_child = &adw::ButtonContent {
                                                set_icon_name: "meusic-radio",
                                                set_label: "Radio",
                                            },
                                            connect_clicked => Msg::SetAppMode(AppMode::Radio),
                                        },
                                    },

                                    gtk::Label {
                                        set_label: "Pemutaran",
                                        set_xalign: 0.0,
                                        add_css_class: "caption-heading",
                                        add_css_class: "dim-label",
                                        set_margin_top: 6,
                                    },
                                    gtk::Box {
                                        set_spacing: 10,
                                        #[watch]
                                        set_sensitive: model.app_mode == AppMode::Music,
                                        gtk::Label { set_label: "Ikuti lagu", set_hexpand: true, set_xalign: 0.0 },
                                        gtk::Switch {
                                            set_valign: gtk::Align::Center,
                                            #[watch] set_active: model.settings.follow_song,
                                            connect_state_set[sender] => move |_, s| {
                                                sender.input(Msg::SetBool(SettingField::FollowSong, s));
                                                glib::Propagation::Proceed
                                            },
                                        },
                                    },
                                    gtk::Box {
                                        set_spacing: 10,
                                        #[watch]
                                        set_sensitive: model.app_mode == AppMode::Music,
                                        gtk::Label { set_label: "Lanjutkan lagu terakhir", set_hexpand: true, set_xalign: 0.0 },
                                        gtk::Switch {
                                            set_valign: gtk::Align::Center,
                                            #[watch] set_active: model.settings.remember_last_played,
                                            connect_state_set[sender] => move |_, s| {
                                                sender.input(Msg::SetBool(SettingField::Remember, s));
                                                glib::Propagation::Proceed
                                            },
                                        },
                                    },
                                    gtk::Box {
                                        set_spacing: 10,
                                        #[watch]
                                        set_sensitive: model.app_mode == AppMode::Music,
                                        gtk::Label { set_label: "Buka halaman terakhir", set_hexpand: true, set_xalign: 0.0 },
                                        gtk::Switch {
                                            set_valign: gtk::Align::Center,
                                            #[watch] set_active: model.settings.resume_startup_page,
                                            connect_state_set[sender] => move |_, s| {
                                                sender.input(Msg::SetBool(SettingField::ResumePage, s));
                                                glib::Propagation::Proceed
                                            },
                                        },
                                    },

                                    gtk::Box {
                                        set_spacing: 6,
                                        set_margin_top: 6,
                                        gtk::Label {
                                            set_label: "Area notifikasi",
                                            set_xalign: 0.0,
                                            set_hexpand: true,
                                            add_css_class: "caption-heading",
                                            add_css_class: "dim-label",
                                        },
                                        gtk::Image {
                                            set_visible: !model.has_tray,
                                            set_icon_name: Some("help-about-symbolic"),
                                            set_tooltip_text: Some(
                                                "Tak ada system tray (GNOME) — ikon tray & 'minimize ke tray' \
                                                 tak berlaku. 'Close = minimize' di bawah tetap berfungsi: X \
                                                 me-minimize app (tetap jalan) alih-alih keluar."
                                            ),
                                            add_css_class: "dim-label",
                                        },
                                    },
                                    gtk::Box {
                                        set_spacing: 10,
                                        #[watch]
                                        set_sensitive: model.has_tray,
                                        gtk::Label { set_label: "Ikon di system tray", set_hexpand: true, set_xalign: 0.0 },
                                        gtk::Switch {
                                            set_valign: gtk::Align::Center,
                                            #[watch] set_active: model.settings.tray_icon,
                                            connect_state_set[sender] => move |_, s| {
                                                sender.input(Msg::SetBool(SettingField::TrayIcon, s));
                                                glib::Propagation::Proceed
                                            },
                                        },
                                    },
                                    gtk::Box {
                                        set_spacing: 10,
                                        #[watch]
                                        set_sensitive: model.has_tray,
                                        gtk::Label { set_label: "Minimize ke tray", set_hexpand: true, set_xalign: 0.0 },
                                        gtk::Switch {
                                            set_valign: gtk::Align::Center,
                                            #[watch] set_active: model.settings.minimize_to_tray,
                                            connect_state_set[sender] => move |_, s| {
                                                sender.input(Msg::SetBool(SettingField::MinTray, s));
                                                glib::Propagation::Proceed
                                            },
                                        },
                                    },
                                    gtk::Box {
                                        set_spacing: 10,
                                        #[watch]
                                        set_tooltip_text: if model.has_tray { None } else {
                                            Some("Tanpa tray: aktif = X me-minimize (app tetap jalan); nonaktif = X keluar.")
                                        },
                                        gtk::Label {
                                            #[watch]
                                            set_label: if model.has_tray { "Close ke tray" } else { "Close = minimize (jangan keluar)" },
                                            set_hexpand: true, set_xalign: 0.0,
                                        },
                                        gtk::Switch {
                                            set_valign: gtk::Align::Center,
                                            #[watch] set_active: model.settings.close_to_tray,
                                            connect_state_set[sender] => move |_, s| {
                                                sender.input(Msg::SetBool(SettingField::CloseTray, s));
                                                glib::Propagation::Proceed
                                            },
                                        },
                                    },

                                    gtk::Separator { set_margin_top: 6 },
                                    gtk::Button {
                                        add_css_class: "flat",
                                        set_margin_top: 2,
                                        #[wrap(Some)]
                                        set_child = &adw::ButtonContent {
                                            set_icon_name: "application-exit-symbolic",
                                            set_label: "Keluar",
                                        },
                                        connect_clicked => Msg::Quit,
                                    },
                                },
                            },
                        },
                        pack_end = &gtk::Button {
                            #[wrap(Some)]
                                #[name = "bc_buka"]
                            set_child = &adw::ButtonContent {
                                set_icon_name: "meusic-folder-open",
                                #[watch]
                                set_label: if model.scanning { "Memindai…" } else { "Buka Folder" },
                            },
                            #[watch]
                            set_visible: model.app_mode == AppMode::Music,
                            #[watch]
                            set_sensitive: !model.scanning,
                            connect_clicked => Msg::PickFolder,
                        },
                        pack_end = &gtk::SearchEntry {
                            set_placeholder_text: Some("Cari…"),
                            add_css_class: "searchbar",
                            set_width_request: 220,
                            connect_search_changed[sender] => move |e| {
                                sender.input(Msg::Search(e.text().to_string()));
                            },
                        },
                    },

                    // two-pane body
                    #[wrap(Some)]
                    set_content = &gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,

                        gtk::Box {
                            #[watch]
                            set_visible: model.app_mode == AppMode::Music,
                            set_orientation: gtk::Orientation::Horizontal,
                            set_vexpand: true,
                            set_spacing: 14,
                            set_margin_start: 14,
                            set_margin_end: 14,
                            set_margin_top: 6,
                            set_margin_bottom: 10,

                        gtk::Box {
                            add_css_class: "glasscard",
                            set_orientation: gtk::Orientation::Vertical,
                            set_width_request: 280,
                            set_hexpand: false,
                            #[watch]
                            set_visible: model.sidebar_visible(),

                            gtk::ScrolledWindow {
                                set_hscrollbar_policy: gtk::PolicyType::Never,
                                set_vexpand: true,

                                #[local_ref]
                                sidebar -> gtk::ListBox {
                                    set_selection_mode: gtk::SelectionMode::Single,
                                    set_valign: gtk::Align::Start,
                                    set_margin_top: 8,
                                    set_margin_bottom: 8,
                                    set_margin_start: 8,
                                    set_margin_end: 8,
                                },
                            },
                        },

                        gtk::Box {
                            add_css_class: "glasscard",
                            set_orientation: gtk::Orientation::Vertical,
                            set_hexpand: true,

                            gtk::Box {
                                set_orientation: gtk::Orientation::Vertical,
                                set_margin_start: 16,
                                set_margin_end: 16,
                                set_margin_top: 12,
                                set_margin_bottom: 8,
                                gtk::Label {
                                    #[watch]
                                    set_label: &model.view_title(),
                                    set_xalign: 0.0,
                                    add_css_class: "heading",
                                },
                                gtk::Label {
                                    #[watch]
                                    set_label: &model.view_summary(),
                                    set_xalign: 0.0,
                                    add_css_class: "dim-label",
                                    add_css_class: "caption",
                                },
                            },

                            gtk::ScrolledWindow {
                                set_hscrollbar_policy: gtk::PolicyType::Never,
                                set_vexpand: true,

                                #[local_ref]
                                song_view -> gtk::ListView {
                                    add_css_class: "songview",
                                    set_margin_start: 8,
                                    set_margin_end: 8,
                                    set_margin_bottom: 8,
                                },
                            },
                        },
                        },

                        gtk::Box {
                            #[watch]
                            set_visible: model.app_mode == AppMode::Radio,
                            set_orientation: gtk::Orientation::Horizontal,
                            set_vexpand: true,
                            set_spacing: 14,
                            set_margin_start: 14,
                            set_margin_end: 14,
                            set_margin_top: 6,
                            set_margin_bottom: 10,

                            gtk::Box {
                                add_css_class: "glasscard",
                                set_orientation: gtk::Orientation::Vertical,
                                set_width_request: 300,
                                set_hexpand: false,

                                gtk::Button {
                                    add_css_class: "suggested-action",
                                    set_margin_top: 10,
                                    set_margin_start: 10,
                                    set_margin_end: 10,
                                    #[wrap(Some)]
                                    set_child = &adw::ButtonContent {
                                        set_icon_name: "meusic-plus",
                                        set_label: "Tambah stasiun",
                                    },
                                    connect_clicked => Msg::AddStation,
                                },
                                gtk::ScrolledWindow {
                                    set_hscrollbar_policy: gtk::PolicyType::Never,
                                    set_vexpand: true,
                                    #[local_ref]
                                    stations_list -> gtk::ListBox {
                                        set_selection_mode: gtk::SelectionMode::Single,
                                        set_valign: gtk::Align::Start,
                                        set_margin_top: 8,
                                        set_margin_bottom: 8,
                                        set_margin_start: 8,
                                        set_margin_end: 8,
                                    },
                                },
                            },

                            gtk::Box {
                                add_css_class: "glasscard",
                                set_orientation: gtk::Orientation::Vertical,
                                set_hexpand: true,
                                set_vexpand: true,

                                #[local_ref]
                                radio_vis -> gtk::DrawingArea {
                                    set_hexpand: true,
                                    set_vexpand: true,
                                    set_margin_top: 18,
                                    set_margin_bottom: 18,
                                    set_margin_start: 18,
                                    set_margin_end: 18,
                                },
                            },
                        },
                    },

                    add_bottom_bar = &gtk::Box {
                        add_css_class: "nowbar",
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 4,
                        set_margin_start: 14,
                        set_margin_end: 14,
                        set_margin_top: 6,
                        set_margin_bottom: 10,

                        #[local_ref]
                        seek -> gtk::Scale {
                            set_hexpand: true,
                            set_draw_value: false,
                            set_range: (0.0, 1.0),
                            #[watch]
                            set_visible: model.app_mode == AppMode::Music,
                        },

                        gtk::CenterBox {
                            #[wrap(Some)]
                            set_start_widget = &gtk::Box {
                                set_orientation: gtk::Orientation::Horizontal,
                                set_spacing: 10,
                                set_halign: gtk::Align::Start,

                                gtk::Button {
                                    add_css_class: "flat",
                                    add_css_class: "coverbtn",
                                    set_valign: gtk::Align::Center,
                                    set_tooltip_text: Some("Buka Now Playing"),
                                    connect_clicked => Msg::OpenNowPlaying,
                                    #[local_ref]
                                    cover -> gtk::Image {
                                        set_pixel_size: 56,
                                        add_css_class: "cover",
                                    },
                                },
                                gtk::Box {
                                    set_orientation: gtk::Orientation::Vertical,
                                    set_valign: gtk::Align::Center,
                                    set_width_request: 170,
                                    set_spacing: 1,
                                    gtk::Label {
                                        #[watch] set_label: &model.now_title(),
                                        set_xalign: 0.0,
                                        set_ellipsize: pango::EllipsizeMode::End,
                                        add_css_class: "heading",
                                    },
                                    gtk::Label {
                                        #[watch] set_label: &model.now_artist(),
                                        set_xalign: 0.0,
                                        set_ellipsize: pango::EllipsizeMode::End,
                                        add_css_class: "dim-label",
                                    },
                                    gtk::Box {
                                        set_spacing: 6,
                                        set_halign: gtk::Align::Start,
                                        gtk::Label {
                                            #[watch] set_label: &model.now_format(),
                                            #[watch] set_visible: !model.now_format().is_empty(),
                                            add_css_class: "fmt-badge",
                                        },
                                        gtk::Label {
                                            #[watch] set_label: &model.now_kbps(),
                                            #[watch] set_visible: !model.now_kbps().is_empty(),
                                            add_css_class: "dim-label",
                                            add_css_class: "caption",
                                        },
                                    },
                                },
                            },

                            #[wrap(Some)]
                            set_center_widget = &gtk::Box {
                                set_orientation: gtk::Orientation::Horizontal,
                                set_spacing: 8,
                                set_halign: gtk::Align::Center,

                                gtk::Button {
                                    #[watch] set_visible: model.app_mode == AppMode::Music,
                                    #[watch] set_css_classes: ctl_classes(model.repeat != Repeat::Off),
                                    #[watch] set_icon_name: if model.repeat == Repeat::One {
                                        "meusic-repeat-one"
                                    } else {
                                        "meusic-repeat"
                                    },
                                    connect_clicked => Msg::CycleRepeat,
                                },
                                gtk::Button {
                                    add_css_class: "flat",
                                    set_icon_name: "meusic-prev",
                                    #[watch] set_visible: model.app_mode == AppMode::Music,
                                    #[watch] set_sensitive: model.qidx.is_some(),
                                    connect_clicked => Msg::Prev,
                                },
                                gtk::Button {
                                    add_css_class: "circular",
                                    add_css_class: "play",
                                    set_valign: gtk::Align::Center,
                                    #[watch] set_icon_name: if model.playing { "meusic-pause" } else { "meusic-play" },
                                    #[watch] set_sensitive: model.has_current(),
                                    connect_clicked => Msg::Toggle,
                                },
                                gtk::Button {
                                    add_css_class: "flat",
                                    set_icon_name: "meusic-next",
                                    #[watch] set_visible: model.app_mode == AppMode::Music,
                                    #[watch] set_sensitive: model.qidx.is_some(),
                                    connect_clicked => Msg::Next,
                                },
                                gtk::Button {
                                    #[watch] set_visible: model.app_mode == AppMode::Music,
                                    #[watch] set_css_classes: ctl_classes(model.shuffle),
                                    set_icon_name: "meusic-shuffle",
                                    connect_clicked => Msg::ToggleShuffle,
                                },
                            },

                            #[wrap(Some)]
                            set_end_widget = &gtk::Box {
                                set_orientation: gtk::Orientation::Horizontal,
                                set_spacing: 8,
                                set_halign: gtk::Align::End,

                                gtk::Label {
                                    #[watch] set_label: &model.time_text(),
                                    #[watch] set_visible: model.app_mode == AppMode::Music,
                                    add_css_class: "dim-label",
                                    add_css_class: "numeric",
                                },
                                gtk::Button {
                                    add_css_class: "flat",
                                    add_css_class: "ctl",
                                    #[watch]
                                    set_icon_name: if model.power_save {
                                        "meusic-leaf-green"
                                    } else {
                                        "meusic-leaf"
                                    },
                                    set_tooltip_text: Some("Hemat energi"),
                                    connect_clicked => Msg::TogglePowerSave,
                                },
                                gtk::MenuButton {
                                    add_css_class: "flat",
                                    add_css_class: "ctl",
                                    set_icon_name: "meusic-sliders-vertical",
                                    #[wrap(Some)]
                                    set_popover = &gtk::Popover {
                                        gtk::Box {
                                            set_orientation: gtk::Orientation::Horizontal,
                                            set_spacing: 8,
                                            set_margin_top: 12,
                                            set_margin_bottom: 12,
                                            set_margin_start: 12,
                                            set_margin_end: 12,

                                            gtk::Box {
                                                set_orientation: gtk::Orientation::Vertical,
                                                set_spacing: 4,
                                                gtk::Scale {
                                                    set_orientation: gtk::Orientation::Vertical,
                                                    set_inverted: true,
                                                    set_range: (-12.0, 12.0),
                                                    set_value: 0.0,
                                                    set_draw_value: false,
                                                    set_height_request: 120,
                                                    connect_value_changed[sender] => move |s| {
                                                        sender.input(Msg::SetEq(0, s.value()));
                                                    },
                                                },
                                                gtk::Label { set_label: EQ_FREQS[0], add_css_class: "dim-label", add_css_class: "caption" },
                                            },
                                            gtk::Box {
                                                set_orientation: gtk::Orientation::Vertical,
                                                set_spacing: 4,
                                                gtk::Scale {
                                                    set_orientation: gtk::Orientation::Vertical,
                                                    set_inverted: true,
                                                    set_range: (-12.0, 12.0),
                                                    set_value: 0.0,
                                                    set_draw_value: false,
                                                    set_height_request: 120,
                                                    connect_value_changed[sender] => move |s| {
                                                        sender.input(Msg::SetEq(1, s.value()));
                                                    },
                                                },
                                                gtk::Label { set_label: EQ_FREQS[1], add_css_class: "dim-label", add_css_class: "caption" },
                                            },
                                            gtk::Box {
                                                set_orientation: gtk::Orientation::Vertical,
                                                set_spacing: 4,
                                                gtk::Scale {
                                                    set_orientation: gtk::Orientation::Vertical,
                                                    set_inverted: true,
                                                    set_range: (-12.0, 12.0),
                                                    set_value: 0.0,
                                                    set_draw_value: false,
                                                    set_height_request: 120,
                                                    connect_value_changed[sender] => move |s| {
                                                        sender.input(Msg::SetEq(2, s.value()));
                                                    },
                                                },
                                                gtk::Label { set_label: EQ_FREQS[2], add_css_class: "dim-label", add_css_class: "caption" },
                                            },
                                            gtk::Box {
                                                set_orientation: gtk::Orientation::Vertical,
                                                set_spacing: 4,
                                                gtk::Scale {
                                                    set_orientation: gtk::Orientation::Vertical,
                                                    set_inverted: true,
                                                    set_range: (-12.0, 12.0),
                                                    set_value: 0.0,
                                                    set_draw_value: false,
                                                    set_height_request: 120,
                                                    connect_value_changed[sender] => move |s| {
                                                        sender.input(Msg::SetEq(3, s.value()));
                                                    },
                                                },
                                                gtk::Label { set_label: EQ_FREQS[3], add_css_class: "dim-label", add_css_class: "caption" },
                                            },
                                            gtk::Box {
                                                set_orientation: gtk::Orientation::Vertical,
                                                set_spacing: 4,
                                                gtk::Scale {
                                                    set_orientation: gtk::Orientation::Vertical,
                                                    set_inverted: true,
                                                    set_range: (-12.0, 12.0),
                                                    set_value: 0.0,
                                                    set_draw_value: false,
                                                    set_height_request: 120,
                                                    connect_value_changed[sender] => move |s| {
                                                        sender.input(Msg::SetEq(4, s.value()));
                                                    },
                                                },
                                                gtk::Label { set_label: EQ_FREQS[4], add_css_class: "dim-label", add_css_class: "caption" },
                                            },
                                            gtk::Box {
                                                set_orientation: gtk::Orientation::Vertical,
                                                set_spacing: 4,
                                                gtk::Scale {
                                                    set_orientation: gtk::Orientation::Vertical,
                                                    set_inverted: true,
                                                    set_range: (-12.0, 12.0),
                                                    set_value: 0.0,
                                                    set_draw_value: false,
                                                    set_height_request: 120,
                                                    connect_value_changed[sender] => move |s| {
                                                        sender.input(Msg::SetEq(5, s.value()));
                                                    },
                                                },
                                                gtk::Label { set_label: EQ_FREQS[5], add_css_class: "dim-label", add_css_class: "caption" },
                                            },
                                        },
                                    },
                                },
                                #[local_ref]
                                volume_pct -> gtk::Label {
                                    add_css_class: "volpct",
                                    add_css_class: "numeric",
                                    set_width_chars: 4,
                                    set_xalign: 1.0,
                                    set_valign: gtk::Align::Center,
                                },
                                gtk::Image {
                                    set_icon_name: Some("meusic-volume-high"),
                                    add_css_class: "ctl",
                                },
                                #[name = "volume_scale"]
                                gtk::Scale {
                                    set_range: (0.0, 1.0),
                                    set_value: model.settings.volume,
                                    set_draw_value: false,
                                    set_width_request: 100,
                                    connect_value_changed[sender] => move |s| {
                                        sender.input(Msg::SetVolume(s.value()));
                                    },
                                },
                            },
                        },
                    },
                },

                add_overlay = &gtk::Overlay {
                    add_css_class: "np",
                    #[watch]
                    set_visible: model.np_open,

                    // Base: soft adaptive gradient from the cover palette.
                    #[local_ref]
                    np_bg -> gtk::DrawingArea {},

                    add_overlay = &gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,

                        gtk::Box {
                            set_halign: gtk::Align::End,
                            set_margin_top: 10,
                            set_margin_end: 12,
                            gtk::Button {
                                add_css_class: "flat",
                                add_css_class: "circular",
                                set_icon_name: "meusic-close",
                                connect_clicked => Msg::CloseNowPlaying,
                            },
                        },

                        // Two equal columns: cover (left) + info & visualizer (right).
                        gtk::Box {
                            set_orientation: gtk::Orientation::Horizontal,
                            set_homogeneous: true,
                            set_hexpand: true,
                            set_vexpand: true,
                            set_spacing: 36,
                            set_margin_start: 48,
                            set_margin_end: 48,
                            set_margin_top: 8,
                            set_margin_bottom: 48,

                            #[local_ref]
                            np_cover -> gtk::Picture {
                                add_css_class: "npcover",
                                set_hexpand: true,
                                set_vexpand: true,
                                set_halign: gtk::Align::Center,
                                set_valign: gtk::Align::Center,
                                set_content_fit: gtk::ContentFit::Contain,
                            },

                            gtk::Box {
                                set_orientation: gtk::Orientation::Vertical,
                                set_valign: gtk::Align::Center,
                                set_hexpand: true,
                                set_spacing: 4,

                                gtk::Label {
                                    #[watch] set_label: &model.now_title(),
                                    add_css_class: "np-title",
                                    set_xalign: 0.0,
                                    set_wrap: true,
                                    set_lines: 2,
                                    set_ellipsize: pango::EllipsizeMode::End,
                                },
                                gtk::Label {
                                    #[watch] set_label: &model.now_artist(),
                                    add_css_class: "title-2",
                                    add_css_class: "dim-label",
                                    set_xalign: 0.0,
                                    set_ellipsize: pango::EllipsizeMode::End,
                                },
                                gtk::Label {
                                    #[watch] set_label: &model.now_album(),
                                    add_css_class: "dim-label",
                                    set_xalign: 0.0,
                                    set_ellipsize: pango::EllipsizeMode::End,
                                },
                                gtk::Box {
                                    set_spacing: 6,
                                    set_halign: gtk::Align::Start,
                                    set_margin_top: 8,
                                    gtk::Label {
                                        #[watch] set_label: &model.now_format(),
                                        #[watch] set_visible: !model.now_format().is_empty(),
                                        add_css_class: "fmt-badge",
                                    },
                                    gtk::Label {
                                        #[watch] set_label: &model.now_kbps(),
                                        #[watch] set_visible: !model.now_kbps().is_empty(),
                                        add_css_class: "dim-label",
                                        add_css_class: "caption",
                                    },
                                },

                                #[local_ref]
                                np_vis -> gtk::DrawingArea {
                                    set_height_request: 240,
                                    set_hexpand: true,
                                    set_margin_top: 24,
                                },
                            },
                        },
                    },
                },
            }
        }
    }

    fn init(
        _init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceDark);

        let app_settings = settings::load();
        let has_tray = settings::has_system_tray();
        let stations_data = stations::load();
        // Last session — only meaningful for resume if a folder was saved.
        let loaded_session = session::load();
        let pending_restore = if loaded_session.root_path.is_empty() {
            None
        } else {
            Some(loaded_session)
        };

        if let Some(display) = gdk::Display::default() {
            let provider = gtk::CssProvider::new();
            provider.load_from_string(STYLE);
            gtk::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
            // Register the app's own (Lucide) icon set — from the baked-in
            // GResource so it works in dev and when installed alike.
            gtk::IconTheme::for_display(&display).add_resource_path("/com/sarta/meusic/icons");
        }
        let accent_provider = gtk::CssProvider::new();
        if let Some(display) = gdk::Display::default() {
            gtk::style_context_add_provider_for_display(
                &display,
                &accent_provider,
                gtk::STYLE_PROVIDER_PRIORITY_USER,
            );
        }

        // Logo wordmark (white normally, green in power-save) — from the GResource.
        let load_logo = |resource: &str| -> Option<gdk::Texture> {
            gtk::gdk_pixbuf::Pixbuf::from_resource_at_scale(resource, -1, 26, true)
                .ok()
                .map(|pb| gdk::Texture::for_pixbuf(&pb))
        };
        let logo_white = load_logo("/com/sarta/meusic/assets/logo.svg");
        let logo_green = load_logo("/com/sarta/meusic/assets/logo-green.svg");

        let player = Player::new();
        player.set_spectrum_enabled(!app_settings.power_save);
        player.set_volume(app_settings.volume);

        let sidebar = gtk::ListBox::new();
        let seek = gtk::Scale::new(gtk::Orientation::Horizontal, None::<&gtk::Adjustment>);
        let cover = gtk::Image::new();
        cover.set_icon_name(Some("meusic-music-note"));
        // Transient volume-percent readout (fades in on change, out after ~1s).
        // Uses opacity (not visibility) so it reserves space and the bar layout
        // never jumps when it appears/disappears.
        let volume_pct = gtk::Label::new(None);
        volume_pct.set_opacity(0.0);
        let palette: Rc<RefCell<PaletteF>> = Rc::new(RefCell::new(
            art::default_palette()
                .iter()
                .map(|&(r, g, b)| (r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0))
                .collect(),
        ));
        let power_save_flag: Rc<Cell<bool>> = Rc::new(Cell::new(app_settings.power_save));
        let bg = build_gradient_bg(palette.clone(), power_save_flag.clone());
        let np_bg = build_gradient_bg(palette.clone(), power_save_flag.clone());

        // Now-Playing cover + spectrum visualizers (one for the Now-Playing view,
        // one for the radio panel — both fed by the shared spectrum buffer).
        let np_cover = gtk::Picture::new();
        // Station chip color/initials state — drawn by the radial radio visualizer.
        let radio_chip_state: Rc<RefCell<(String, (u8, u8, u8))>> =
            Rc::new(RefCell::new(("♪".to_string(), (90, 90, 140))));
        let spectrum: Rc<RefCell<Vec<f64>>> = Rc::new(RefCell::new(Vec::new()));
        let np_vis = build_np_visualizer(spectrum.clone(), palette.clone());
        let radio_vis = build_radial_visualizer(
            spectrum.clone(),
            palette.clone(),
            radio_chip_state.clone(),
        );

        // ---- Virtualized song list (gtk::ListView) ----------------------------
        // The model holds lightweight TrackObjects; only the handful of visible
        // rows ever get widgets (the whole library no longer builds N rows).
        let eq_phase: Rc<Cell<f64>> = Rc::new(Cell::new(0.0));
        let current_path_shared: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
        let playing_eq: Rc<RefCell<Option<gtk::DrawingArea>>> = Rc::new(RefCell::new(None));
        let bound_rows: Rc<RefCell<Vec<(String, gtk::Box, gtk::DrawingArea)>>> =
            Rc::new(RefCell::new(Vec::new()));
        let playing_flag: Rc<Cell<bool>> = Rc::new(Cell::new(false));
        let song_model = gio::ListStore::new::<TrackObject>();

        let factory = gtk::SignalListItemFactory::new();
        // setup: build the (reusable) row widget tree once per recycled row.
        {
            let eq_phase = eq_phase.clone();
            let palette = palette.clone();
            factory.connect_setup(move |_, item| {
                let Some(item) = item.downcast_ref::<gtk::ListItem>() else { return };
                let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
                row.add_css_class("songrow");
                row.set_margin_top(5);
                row.set_margin_bottom(5);
                row.set_margin_start(10);
                row.set_margin_end(10);

                // Lead slot: the animated EQ indicator (shown only on the playing row).
                let lead = gtk::Box::new(gtk::Orientation::Horizontal, 0);
                lead.set_size_request(20, -1);
                lead.set_valign(gtk::Align::Center);
                let eq = gtk::DrawingArea::new();
                eq.set_size_request(20, 16);
                eq.set_valign(gtk::Align::Center);
                eq.set_visible(false);
                {
                    let phase = eq_phase.clone();
                    let pal = palette.clone();
                    eq.set_draw_func(move |_, cr, w, h| {
                        let (w, h) = (w as f64, h as f64);
                        let pal = pal.borrow();
                        let (r, g, b) = pal.first().copied().unwrap_or((0.6, 0.6, 0.8));
                        cr.set_source_rgba(r, g, b, 0.95);
                        let ph = phase.get();
                        let n = 4usize;
                        let slot = w / n as f64;
                        let bw = slot * 0.55;
                        for i in 0..n {
                            let v = 0.5 + 0.5 * (ph + i as f64 * 0.9).sin();
                            let bh = (0.28 + 0.72 * v) * h;
                            cr.rectangle(i as f64 * slot + (slot - bw) / 2.0, h - bh, bw, bh);
                        }
                        let _ = cr.fill();
                    });
                }
                lead.append(&eq);

                let info = gtk::Box::new(gtk::Orientation::Vertical, 1);
                info.set_hexpand(true);
                let title = gtk::Label::new(None);
                title.set_xalign(0.0);
                title.set_ellipsize(pango::EllipsizeMode::End);
                let sub = gtk::Label::new(None);
                sub.set_xalign(0.0);
                sub.set_ellipsize(pango::EllipsizeMode::End);
                sub.add_css_class("dim-label");
                sub.add_css_class("caption");
                info.append(&title);
                info.append(&sub);

                let dur = gtk::Label::new(None);
                dur.add_css_class("dim-label");
                dur.add_css_class("numeric");

                row.append(&lead);
                row.append(&info);
                row.append(&dur);
                item.set_child(Some(&row));
            });
        }
        // bind: fill the row from its TrackObject + apply the playing highlight.
        {
            let current = current_path_shared.clone();
            let playing_eq = playing_eq.clone();
            let bound = bound_rows.clone();
            factory.connect_bind(move |_, item| {
                let Some(item) = item.downcast_ref::<gtk::ListItem>() else { return };
                let Some(obj) = item.item().and_downcast::<TrackObject>() else { return };
                let Some(row) = item.child().and_downcast::<gtk::Box>() else { return };
                let t = obj.track();
                let (eq, title, sub, dur) = row_widgets(&row);
                title.set_label(&t.title);
                sub.set_label(&format!("{} · {}", t.artist, t.album));
                dur.set_label(&fmt_time(t.duration));
                let is_cur = current.borrow().as_deref() == Some(t.path.as_str());
                apply_row_state(&row, &eq, is_cur, &playing_eq);
                bound.borrow_mut().push((t.path.clone(), row, eq));
            });
        }
        // unbind: drop the row from the bound set (release the EQ indicator).
        {
            let bound = bound_rows.clone();
            let playing_eq = playing_eq.clone();
            factory.connect_unbind(move |_, item| {
                let Some(item) = item.downcast_ref::<gtk::ListItem>() else { return };
                let Some(row) = item.child().and_downcast::<gtk::Box>() else { return };
                let mut clear = false;
                bound.borrow_mut().retain(|(_, b, eq)| {
                    if *b == row {
                        if playing_eq.borrow().as_ref() == Some(eq) {
                            clear = true;
                        }
                        false
                    } else {
                        true
                    }
                });
                if clear {
                    *playing_eq.borrow_mut() = None;
                }
            });
        }

        let selection = gtk::NoSelection::new(Some(song_model.clone()));
        let song_view = gtk::ListView::new(Some(selection), Some(factory));
        song_view.set_single_click_activate(true);
        {
            let play_sender = sender.clone();
            song_view.connect_activate(move |_, pos| {
                play_sender.input(Msg::PlayIndex(pos as usize));
            });
        }

        // Animate the EQ indicator on the playing row (single shared phase, one
        // queue_draw on the registered visible row — no work when idle/paused).
        {
            let phase = eq_phase.clone();
            let ps = power_save_flag.clone();
            let playing = playing_flag.clone();
            let active = playing_eq.clone();
            glib::timeout_add_local(Duration::from_millis(110), move || {
                if !ps.get() && playing.get() {
                    if let Some(eq) = active.borrow().as_ref() {
                        phase.set(phase.get() + 0.38);
                        eq.queue_draw();
                    }
                }
                glib::ControlFlow::Continue
            });
        }
        let sidebar_icons: Rc<RefCell<Vec<(String, gtk::Image)>>> = Rc::new(RefCell::new(Vec::new()));

        // Bus watch: auto-advance on EOS + feed the spectrum visualizers.
        let bus = player.bus();
        let eos_sender = sender.clone();
        let spec_buf = spectrum.clone();
        let np_vis_widget = np_vis.clone();
        let radio_vis_widget = radio_vis.clone();
        let bus_watch = bus
            .add_watch_local(move |_, message| {
                use gstreamer::MessageView;
                match message.view() {
                    MessageView::Eos(_) => eos_sender.input(Msg::Next),
                    MessageView::Error(err) => {
                        let detail = format!("{} {}", err.error(), err.debug().unwrap_or_default());
                        eprintln!("meusic: gst error: {detail}");
                        eos_sender.input(Msg::RadioError(detail));
                    }
                    MessageView::Tag(t) => {
                        // ICY "now playing" for radio streams.
                        if let Some(title) = t.tags().get::<gstreamer::tags::Title>() {
                            let s = title.get().to_string();
                            if !s.trim().is_empty() {
                                eos_sender.input(Msg::RadioTitle(Some(s)));
                            }
                        }
                    }
                    MessageView::Element(e) => {
                        if let Some(s) = e.structure() {
                            if s.name() == "spectrum" {
                                if let Ok(list) = s.get::<gstreamer::List>("magnitude") {
                                    let norm: Vec<f64> = list
                                        .iter()
                                        .filter_map(|v| {
                                            v.get::<f32>()
                                                .ok()
                                                .map(|x| x as f64)
                                                .or_else(|| v.get::<f64>().ok())
                                        })
                                        // Linear 0..1 from dB; the binning + gamma
                                        // happen at draw time (mirrors the Windows build).
                                        .map(|db| ((db + 65.0) / 65.0).clamp(0.0, 1.0))
                                        .collect();
                                    *spec_buf.borrow_mut() = norm;
                                    np_vis_widget.queue_draw();
                                    radio_vis_widget.queue_draw();
                                }
                            }
                        }
                    }
                    _ => {}
                }
                glib::ControlFlow::Continue
            })
            .expect("failed to add bus watch");

        let sidebar_keys: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
        let keys_for_row = sidebar_keys.clone();
        let group_sender = sender.clone();
        sidebar.connect_row_activated(move |_, row| {
            let idx = row.index();
            if idx >= 0 {
                if let Some(key) = keys_for_row.borrow().get(idx as usize) {
                    group_sender.input(Msg::SelectGroup(key.clone()));
                }
            }
        });

        let stations_list = gtk::ListBox::new();
        let station_keys: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
        let st_keys = station_keys.clone();
        let st_sender = sender.clone();
        stations_list.connect_row_activated(move |_, row| {
            let idx = row.index();
            if idx >= 0 {
                if let Some(id) = st_keys.borrow().get(idx as usize) {
                    st_sender.input(Msg::PlayStation(id.clone()));
                }
            }
        });


        let seek_sender = sender.clone();
        seek.connect_change_value(move |_, _, value| {
            seek_sender.input(Msg::Seek(value));
            glib::Propagation::Proceed
        });

        let tick_sender = sender.clone();
        let tick_ps = power_save_flag.clone();
        let tick_count = Cell::new(0u32);
        glib::timeout_add_local(Duration::from_millis(250), move || {
            let c = tick_count.get().wrapping_add(1);
            tick_count.set(c);
            // Power-save: tick 4× less often (~1s) — slower seek/position updates.
            if !(tick_ps.get() && c % 4 != 0) {
                tick_sender.input(Msg::Tick);
            }
            glib::ControlFlow::Continue
        });

        // Periodically snapshot the session (mainly to capture the play position)
        // so resume is up to date even without an explicit state change.
        let save_sender = sender.clone();
        glib::timeout_add_local(Duration::from_secs(5), move || {
            save_sender.input(Msg::SaveSession);
            glib::ControlFlow::Continue
        });

        let ctl_sender = sender.clone();
        mpris::start(move |control| {
            use mpris::Control::*;
            let msg = match control {
                PlayPause => Msg::Toggle,
                Play => Msg::SetPlaying(true),
                Pause => Msg::SetPlaying(false),
                Stop => Msg::Stop,
                Next => Msg::Next,
                Previous => Msg::Prev,
                Raise | Quit => Msg::Raise,
            };
            ctl_sender.input(msg);
        });

        let model = App {
            all_tracks: Vec::new(),
            view_tracks: Vec::new(),
            mode: Mode::Folders,
            sel_group: None,
            query: String::new(),
            queue: Vec::new(),
            qidx: None,
            current_path: None,
            cur_art_url: None,
            root_path: None,
            pending_restore,
            playing: false,
            position: 0.0,
            duration: 0.0,
            scanning: false,
            music_err_streak: 0,
            repeat: Repeat::Off,
            shuffle: false,
            np_open: false,
            app_mode: AppMode::Music,
            music_position: 0.0,
            pending_seek: None,
            settings: app_settings,
            has_tray,
            stations_data,
            station_id: None,
            radio_title: None,
            radio_connecting: false,
            radio_error: false,
            radio_retries: 0,
            radio_reconnect_pending: false,
            radio_last_pos: 0.0,
            radio_stall_ticks: 0,
            player,
            song_view: song_view.clone(),
            song_model: song_model.clone(),
            sidebar: sidebar.clone(),
            stations_list: stations_list.clone(),
            radio_chip_state,
            radio_vis: radio_vis.clone(),
            seek: seek.clone(),
            bg: bg.clone(),
            cover: cover.clone(),
            np_cover: np_cover.clone(),
            np_bg: np_bg.clone(),
            accent_provider,
            palette,
            sidebar_keys,
            sidebar_icons,
            station_keys,
            station_rows: Rc::new(RefCell::new(Vec::new())),
            bound_rows,
            current_path_shared,
            playing_eq,
            playing_flag,
            volume_pct: volume_pct.clone(),
            volume_pct_gen: Rc::new(Cell::new(0)),
            logo_white,
            logo_green,
            power_save: power_save_flag.get(),
            power_save_flag,
            window: root.clone(),
            sender: sender.clone(),
            _bus_watch: bus_watch,
        };
        model.rebuild_stations();
        let widgets = view_output!();

        // Volume slider: scroll over it to adjust by the configured step (percent).
        {
            let step = (model.settings.volume_scroll_step as f64 / 100.0).clamp(0.01, 1.0);
            let scale = widgets.volume_scale.clone();
            let scroll =
                gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
            scroll.connect_scroll(move |_, _dx, dy| {
                // Scroll up (dy < 0) raises the volume; the value-changed handler
                // pushes it to the player + persists it.
                let next = (scale.value() - dy.signum() * step).clamp(0.0, 1.0);
                scale.set_value(next);
                glib::Propagation::Stop
            });
            widgets.volume_scale.add_controller(scroll);
        }

        // Responsive: when the window is narrow, collapse the tab + "Buka Folder"
        // labels to icon-only (the logo stays fixed instead of shrinking away).
        let breakpoint = adw::Breakpoint::new(adw::BreakpointCondition::new_length(
            adw::BreakpointConditionLengthType::MaxWidth,
            900.0,
            adw::LengthUnit::Px,
        ));
        let empty = "".to_value();
        for bc in [
            &widgets.bc_folders,
            &widgets.bc_albums,
            &widgets.bc_artists,
            &widgets.bc_songs,
            &widgets.bc_buka,
        ] {
            breakpoint.add_setter(bc, "label", Some(&empty));
        }
        root.add_breakpoint(breakpoint);

        // Kick off last-session restore (re-scan the saved folder, then restore
        // page/track per the settings) once the component is up.
        if model.pending_restore.is_some() {
            sender.input(Msg::RestoreSession);
        }

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            Msg::PickFolder => {
                let dialog = gtk::FileDialog::builder().title("Buka Folder").build();
                let scan_sender = sender.clone();
                // Mark "scanning" only once a folder is actually chosen — if the
                // dialog is cancelled the callback returns early and we must not
                // get stuck showing "Memindai…" forever.
                dialog.select_folder(None::<&gtk::Window>, gio::Cancellable::NONE, move |result| {
                    let Ok(folder) = result else { return };
                    let Some(path) = folder.path() else { return };
                    let path = path.to_string_lossy().to_string();
                    scan_sender.input(Msg::ScanStarted(path.clone()));
                    let scan_sender = scan_sender.clone();
                    std::thread::spawn(move || {
                        let tracks = scan_folder(&path);
                        scan_sender.input(Msg::Scanned(tracks));
                    });
                });
            }
            Msg::ScanStarted(path) => {
                self.scanning = true;
                self.root_path = Some(path);
            }
            Msg::Scanned(tracks) => {
                self.scanning = false;
                self.all_tracks = tracks;
                // A pending restore (startup) applies the saved page/track instead
                // of the default reset.
                if let Some(s) = self.pending_restore.take() {
                    self.restore_from_session(s);
                } else {
                    self.sel_group = None;
                    self.rebuild_sidebar();
                    self.recompute_view();
                }
                self.save_session();
            }
            Msg::RestoreSession => {
                if let Some(s) = self.pending_restore.clone() {
                    self.root_path = Some(s.root_path.clone());
                    self.scanning = true;
                    let scan_sender = sender.clone();
                    let root = s.root_path.clone();
                    std::thread::spawn(move || {
                        let tracks = scan_folder(&root);
                        scan_sender.input(Msg::Scanned(tracks));
                    });
                }
            }
            Msg::SaveSession => self.save_session(),
            Msg::SetMode(m) => {
                if self.mode != m {
                    self.mode = m;
                    self.sel_group = None;
                    self.rebuild_sidebar();
                    self.recompute_view();
                    self.save_session();
                }
            }
            Msg::SelectGroup(key) => {
                self.sel_group = Some(key);
                self.recompute_view();
                self.refresh_folder_icons();
                self.save_session();
            }
            Msg::Search(q) => {
                self.query = q;
                self.recompute_view();
                self.rebuild_stations();
            }
            Msg::PlayIndex(i) => {
                if i < self.view_tracks.len() {
                    self.music_err_streak = 0;
                    self.queue = self.view_tracks.clone();
                    self.play_at(i);
                }
            }
            Msg::Toggle => {
                if self.has_current() {
                    if self.playing {
                        self.player.pause();
                    } else {
                        self.player.play();
                    }
                    self.playing = !self.playing;
                    self.mpris_sync();
                }
            }
            Msg::SetPlaying(p) => {
                if self.has_current() && self.playing != p {
                    if p {
                        self.player.play();
                    } else {
                        self.player.pause();
                    }
                    self.playing = p;
                    self.mpris_sync();
                }
            }
            Msg::Stop => {
                self.player.stop();
                self.playing = false;
                self.qidx = None;
                self.current_path = None;
                self.refresh_highlight();
                self.apply_visuals(None);
                self.mpris_sync();
                self.save_session();
            }
            Msg::Next => {
                // EOS in radio = the live stream dropped → reconnect, don't advance.
                if self.app_mode == AppMode::Radio {
                    if self.station_id.is_some() && !self.radio_error {
                        self.schedule_reconnect();
                    }
                } else {
                    self.advance(false);
                }
            }
            Msg::Prev => {
                if self.position > 3.0 {
                    self.player.seek(0.0);
                } else if let Some(i) = self.qidx {
                    let len = self.queue.len();
                    let prev = if i > 0 {
                        i - 1
                    } else if self.repeat == Repeat::All && len > 0 {
                        len - 1
                    } else {
                        0
                    };
                    self.play_at(prev);
                }
            }
            Msg::CycleRepeat => {
                self.repeat = match self.repeat {
                    Repeat::Off => Repeat::All,
                    Repeat::All => Repeat::One,
                    Repeat::One => Repeat::Off,
                };
            }
            Msg::ToggleShuffle => {
                self.shuffle = !self.shuffle;
            }
            Msg::SetEq(i, gain) => {
                if let Some(&band) = EQ_BAND_MAP.get(i) {
                    self.player.set_eq_band(band, gain);
                }
            }
            Msg::Seek(secs) => {
                self.player.seek(secs);
                self.position = secs;
            }
            Msg::SetVolume(v) => {
                self.player.set_volume(v);
                // Transient percent readout: show now, hide ~1s after the last
                // change (a generation token cancels stale hide timers).
                self.volume_pct
                    .set_label(&format!("{}%", (v * 100.0).round() as i32));
                self.volume_pct.set_opacity(1.0);
                let token = self.volume_pct_gen.get().wrapping_add(1);
                self.volume_pct_gen.set(token);
                let lbl = self.volume_pct.clone();
                let gen_cell = self.volume_pct_gen.clone();
                glib::timeout_add_local_once(Duration::from_millis(1000), move || {
                    if gen_cell.get() == token {
                        lbl.set_opacity(0.0);
                    }
                });
                // Persist, rounded to 0.01 — skips the flood of redundant writes
                // a slider drag would otherwise produce.
                let rounded = (v * 100.0).round() / 100.0;
                if (self.settings.volume - rounded).abs() > f64::EPSILON {
                    self.settings.volume = rounded;
                    settings::save(&self.settings);
                }
            }
            Msg::Raise => {
                self.window.present();
            }
            Msg::OpenNowPlaying => {
                // Music-only (the NP view is cover + linear visualizer); radio has
                // its own radial panel. Disabled in power-save (visualizer frozen).
                if self.app_mode == AppMode::Music && self.qidx.is_some() && !self.power_save {
                    self.np_open = true;
                }
            }
            Msg::CloseNowPlaying => {
                self.np_open = false;
            }
            Msg::SetAppMode(m) => {
                if self.app_mode != m {
                    // Remember where the music track was so it resumes on return.
                    if self.app_mode == AppMode::Music {
                        self.music_position = self.position;
                    }
                    self.app_mode = m;
                    self.player.stop();
                    self.playing = false;
                    match m {
                        AppMode::Radio => {
                            self.radio_title = None;
                            // Refresh the cached art for whatever station is current
                            // (don't leave a music cover in the MPRIS popup).
                            self.cur_art_url = self
                                .cur_station()
                                .map(|s| s.name.clone())
                                .and_then(|n| art::station_art_file(&n));
                        }
                        AppMode::Music => {
                            // Re-prime the (paused) track so Play resumes it.
                            if let Some(i) = self.qidx {
                                if let Some(t) = self.queue.get(i).cloned() {
                                    self.player.load(&t.path);
                                    self.player.pause();
                                    self.pending_seek = Some(self.music_position);
                                    self.apply_visuals(Some(&t.path));
                                }
                            }
                        }
                    }
                    self.refresh_highlight();
                    self.mpris_sync();
                }
            }
            Msg::PlayStation(id) => {
                if let Some(st) = self.stations_data.iter().find(|s| s.id == id).cloned() {
                    self.player.load_url(&st.url);
                    self.player.play();
                    self.station_id = Some(id);
                    self.radio_title = None;
                    self.radio_connecting = true;
                    self.radio_error = false;
                    self.radio_retries = 0;
                    self.radio_reconnect_pending = false;
                    self.radio_stall_ticks = 0;
                    self.radio_last_pos = 0.0;
                    self.playing = true;
                    self.duration = 0.0;
                    self.cover.set_icon_name(Some("meusic-radio"));
                    self.np_cover.set_paintable(None::<&gdk::Texture>);
                    // Color the gradient + accent from the station's deterministic hue.
                    let (r, g, b) = art::station_color(&st.name);
                    *self.palette.borrow_mut() =
                        vec![(r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0)];
                    self.bg.queue_draw();
                    self.accent_provider.load_from_string(&format!(
                        "@define-color accent_bg_color rgb({r},{g},{b}); @define-color accent_color rgb({r},{g},{b});"
                    ));
                    *self.radio_chip_state.borrow_mut() =
                        (stations::initials(&st.name), (r, g, b));
                    self.radio_vis.queue_draw();
                    // Render the station chip PNG once here; mpris_sync reuses it.
                    self.cur_art_url = art::station_art_file(&st.name);
                    self.refresh_station_highlight();
                    self.mpris_sync();
                }
            }
            Msg::RadioTitle(t) => {
                if self.app_mode == AppMode::Radio {
                    self.radio_title = t;
                    self.radio_connecting = false;
                    self.radio_error = false;
                    self.radio_retries = 0;
                    self.mpris_sync();
                }
            }
            Msg::RadioError(detail) => {
                if self.app_mode == AppMode::Radio && self.station_id.is_some() {
                    let m = detail.to_lowercase();
                    // Permanent failures: bad URL / auth → stop, don't retry.
                    let permanent = m.contains("not found")
                        || m.contains("404")
                        || m.contains("forbidden")
                        || m.contains("403")
                        || m.contains("unauthorized")
                        || m.contains("401");
                    if permanent {
                        self.player.stop();
                        self.playing = false;
                        self.radio_connecting = false;
                        self.radio_error = true;
                    } else {
                        // Transient (drop / Range / stall) → backoff reconnect.
                        self.schedule_reconnect();
                    }
                    self.mpris_sync();
                } else if self.app_mode == AppMode::Music && self.qidx.is_some() {
                    // A music file failed to decode/open — skip it. Guard against a
                    // tight error loop (e.g. a queue full of broken files) by giving
                    // up once we've errored through the whole queue without a single
                    // track playing. The streak resets when a track starts playing
                    // (Tick) or the user picks a track (PlayIndex).
                    self.music_err_streak += 1;
                    if self.music_err_streak as usize >= self.queue.len().max(1) {
                        self.music_err_streak = 0;
                        self.player.stop();
                        self.playing = false;
                        self.qidx = None;
                        self.current_path = None;
                        self.refresh_highlight();
                        self.apply_visuals(None);
                        self.mpris_sync();
                    } else {
                        self.advance(false);
                    }
                }
            }
            Msg::ReconnectRadio => {
                self.radio_reconnect_pending = false;
                if self.app_mode == AppMode::Radio && !self.player.is_active() {
                    if let Some(st) = self.cur_station().cloned() {
                        self.player.load_url(&st.url);
                        self.player.play();
                        self.playing = true;
                    }
                }
            }
            Msg::AddStation => self.open_station_dialog(None),
            Msg::EditStation(id) => {
                if let Some(st) = self.stations_data.iter().find(|s| s.id == id).cloned() {
                    self.open_station_dialog(Some(st));
                }
            }
            Msg::DeleteStation(id) => {
                if self.station_id.as_ref() == Some(&id) {
                    self.player.stop();
                    self.playing = false;
                    self.station_id = None;
                }
                self.stations_data.retain(|s| s.id != id);
                stations::save(&self.stations_data);
                self.rebuild_stations();
            }
            Msg::SaveStation { id, name, url } => {
                match id {
                    Some(id) => {
                        if let Some(s) = self.stations_data.iter_mut().find(|s| s.id == id) {
                            s.name = name;
                            s.url = url;
                        }
                    }
                    None => self.stations_data.push(stations::new_station(&name, &url)),
                }
                stations::save(&self.stations_data);
                self.rebuild_stations();
            }
            Msg::OpenAbout => self.open_about(),
            Msg::CloseRequest => {
                // No system tray on stock GNOME, so "close to tray" is honored as
                // minimize-to-dock: the window goes away but the app keeps playing
                // (restore via the overview / Alt-Tab / dock). Turn the setting off
                // for the conventional X = quit.
                if self.settings.close_to_tray {
                    self.window.minimize();
                } else {
                    self.sender.input(Msg::Quit);
                }
            }
            Msg::Quit => {
                self.save_session();
                relm4::main_application().quit();
            }
            Msg::TogglePowerSave => {
                self.power_save = !self.power_save;
                self.power_save_flag.set(self.power_save);
                self.settings.power_save = self.power_save;
                settings::save(&self.settings);
                // Linux extra saving: stop the spectrum FFT entirely in power-save.
                self.player.set_spectrum_enabled(!self.power_save);
                if self.power_save {
                    self.np_open = false; // close the (now frozen) Now-Playing view
                }
                self.bg.queue_draw();
                self.np_bg.queue_draw();
            }
            Msg::SetBool(field, v) => {
                match field {
                    SettingField::Remember => self.settings.remember_last_played = v,
                    SettingField::ResumePage => self.settings.resume_startup_page = v,
                    SettingField::FollowSong => self.settings.follow_song = v,
                    SettingField::TrayIcon => self.settings.tray_icon = v,
                    SettingField::MinTray => self.settings.minimize_to_tray = v,
                    SettingField::CloseTray => self.settings.close_to_tray = v,
                }
                settings::save(&self.settings);
            }
            Msg::Tick => {
                if self.app_mode == AppMode::Radio {
                    if self.player.is_active() {
                        if self.radio_connecting {
                            self.radio_connecting = false;
                        }
                        self.radio_retries = 0;
                        // Stall watchdog: position not advancing while playing.
                        if self.playing && !self.radio_error && !self.radio_reconnect_pending {
                            let pos = self.player.position();
                            if (pos - self.radio_last_pos).abs() > 0.05 {
                                self.radio_last_pos = pos;
                                self.radio_stall_ticks = 0;
                            } else {
                                self.radio_stall_ticks += 1;
                                // The tick fires 4× slower in power-save, so scale
                                // the threshold to keep the stall timeout ~12s.
                                let limit = if self.power_save {
                                    RADIO_STALL_TICKS / 4
                                } else {
                                    RADIO_STALL_TICKS
                                };
                                if self.radio_stall_ticks > limit {
                                    self.schedule_reconnect();
                                }
                            }
                        }
                    }
                } else if self.qidx.is_some() {
                    if let Some(pos) = self.pending_seek.take() {
                        self.player.seek(pos);
                        self.position = pos;
                    }
                    self.position = self.player.position();
                    // A track that has actually started decoding clears the
                    // consecutive-error guard.
                    if self.position > 0.5 {
                        self.music_err_streak = 0;
                    }
                    let d = self.player.duration();
                    if d > 0.0 {
                        self.duration = d;
                    }
                    self.seek.set_range(0.0, self.duration.max(1.0));
                    self.seek.set_value(self.position);
                    mpris::set_position(self.position);
                }
            }
        }
    }
}

impl App {
    fn sidebar_visible(&self) -> bool {
        self.mode != Mode::Songs && self.query.is_empty()
    }

    fn cur(&self) -> Option<&Track> {
        self.qidx.and_then(|i| self.queue.get(i))
    }

    /// Whether anything is loaded (a music track or a radio station).
    fn has_current(&self) -> bool {
        self.qidx.is_some() || self.station_id.is_some()
    }

    fn logo_paintable(&self) -> Option<&gdk::Texture> {
        if self.power_save {
            self.logo_green.as_ref()
        } else {
            self.logo_white.as_ref()
        }
    }

    fn cur_station(&self) -> Option<&stations::Station> {
        self.station_id
            .as_ref()
            .and_then(|id| self.stations_data.iter().find(|s| &s.id == id))
    }

    fn now_title(&self) -> String {
        match self.app_mode {
            AppMode::Radio => self
                .cur_station()
                .map(|s| s.name.clone())
                .unwrap_or_else(|| "meusic".into()),
            AppMode::Music => self
                .cur()
                .map(|t| t.title.clone())
                .unwrap_or_else(|| "meusic".into()),
        }
    }
    fn now_artist(&self) -> String {
        match self.app_mode {
            AppMode::Radio => {
                if self.cur_station().is_none() {
                    "Belum ada stasiun".into()
                } else if self.radio_error {
                    "Gagal menyambung — stream mungkin mati".into()
                } else if self.radio_connecting {
                    if self.radio_retries > 0 {
                        "Menyambung ulang…".into()
                    } else {
                        "Menyambungkan…".into()
                    }
                } else {
                    self.radio_title
                        .clone()
                        .unwrap_or_else(|| if self.playing { "Live".into() } else { "Radio".into() })
                }
            }
            AppMode::Music => self
                .cur()
                .map(|t| t.artist.clone())
                .unwrap_or_else(|| "Belum ada lagu".into()),
        }
    }
    fn now_album(&self) -> String {
        match self.app_mode {
            AppMode::Radio => String::new(),
            AppMode::Music => self.cur().map(|t| t.album.clone()).unwrap_or_default(),
        }
    }

    /// Header subtitle for the music content card: "N lagu · total durasi".
    fn view_summary(&self) -> String {
        let total: u64 = self.view_tracks.iter().map(|t| t.duration).sum();
        format!("{} lagu · {}", self.view_tracks.len(), library::fmt_total(total))
    }
    fn now_format(&self) -> String {
        if self.app_mode != AppMode::Music {
            return String::new();
        }
        self.cur().map(|t| t.format.clone()).unwrap_or_default()
    }
    fn now_kbps(&self) -> String {
        if self.app_mode != AppMode::Music {
            return String::new();
        }
        match self.cur() {
            Some(t) if t.bitrate > 0 => format!("{} kbps", t.bitrate),
            _ => String::new(),
        }
    }
    fn time_text(&self) -> String {
        format!("{} / {}", fmt_time(self.position as u64), fmt_time(self.duration as u64))
    }

    fn view_title(&self) -> String {
        if !self.query.is_empty() {
            return format!("Pencarian: {}", self.query);
        }
        match self.mode {
            Mode::Songs => "Semua Lagu".into(),
            _ => self.sel_group.clone().unwrap_or_else(|| match self.mode {
                Mode::Folders => "Semua Folder".into(),
                Mode::Albums => "Semua Album".into(),
                Mode::Artists => "Semua Artis".into(),
                Mode::Songs => "Semua Lagu".into(),
            }),
        }
    }

    fn groups(&self) -> Vec<library::Group> {
        match self.mode {
            Mode::Folders => folder_groups(&self.all_tracks),
            Mode::Albums => album_groups(&self.all_tracks),
            Mode::Artists => artist_groups(&self.all_tracks),
            Mode::Songs => Vec::new(),
        }
    }

    fn rebuild_sidebar(&self) {
        while let Some(child) = self.sidebar.first_child() {
            self.sidebar.remove(&child);
        }
        let base_icon = match self.mode {
            Mode::Folders => "meusic-folder",
            Mode::Albums => "meusic-album",
            Mode::Artists => "meusic-artist",
            Mode::Songs => "meusic-music-note",
        };
        let groups = self.groups();
        let mut keys = Vec::with_capacity(groups.len());
        let mut icons = Vec::with_capacity(groups.len());
        for g in &groups {
            let row = adw::ActionRow::builder()
                .title(glib::markup_escape_text(&g.label).as_str())
                .activatable(true)
                .build();
            // Folders: the open folder shows the "folder-open" icon.
            let selected = self.sel_group.as_ref() == Some(&g.key);
            let icon_name = if self.mode == Mode::Folders && selected {
                "meusic-folder-open"
            } else {
                base_icon
            };
            let img = gtk::Image::from_icon_name(icon_name);
            row.add_prefix(&img);
            let count = gtk::Label::new(Some(&g.count.to_string()));
            count.add_css_class("count");
            row.add_suffix(&count);
            self.sidebar.append(&row);
            keys.push(g.key.clone());
            icons.push((g.key.clone(), img));
        }
        *self.sidebar_keys.borrow_mut() = keys;
        *self.sidebar_icons.borrow_mut() = icons;
    }

    /// Update folder icons (open vs closed) without rebuilding the sidebar.
    fn refresh_folder_icons(&self) {
        if self.mode != Mode::Folders {
            return;
        }
        for (key, img) in self.sidebar_icons.borrow().iter() {
            let name = if self.sel_group.as_ref() == Some(key) {
                "meusic-folder-open"
            } else {
                "meusic-folder"
            };
            img.set_icon_name(Some(name));
        }
    }

    fn recompute_view(&mut self) {
        let q = self.query.to_lowercase();
        self.view_tracks = if !q.is_empty() {
            self.all_tracks
                .iter()
                .filter(|t| {
                    t.title.to_lowercase().contains(&q)
                        || t.artist.to_lowercase().contains(&q)
                        || t.album.to_lowercase().contains(&q)
                })
                .cloned()
                .collect()
        } else {
            match (self.mode, &self.sel_group) {
                (Mode::Songs, _) | (_, None) => self.all_tracks.clone(),
                (mode, Some(key)) => self
                    .all_tracks
                    .iter()
                    .filter(|t| track_key(t, mode) == *key)
                    .cloned()
                    .collect(),
            }
        };
        self.rebuild_songlist();
        self.refresh_highlight();
    }

    fn rebuild_songlist(&self) {
        // Swap the whole model in one splice (no per-row widget work — the
        // ListView realizes only the visible rows via the factory).
        let objs: Vec<TrackObject> = self
            .view_tracks
            .iter()
            .cloned()
            .map(TrackObject::new)
            .collect();
        self.song_model
            .splice(0, self.song_model.n_items(), &objs);
    }

    fn refresh_highlight(&self) {
        // Mirror the current path so newly-bound rows (on scroll) pick up the
        // highlight, then re-apply it across the rows bound right now.
        *self.current_path_shared.borrow_mut() = self.current_path.clone();
        let cur = self.current_path.clone();
        let mut found = false;
        for (path, row, eq) in self.bound_rows.borrow().iter() {
            let is_cur = cur.as_deref() == Some(path.as_str());
            apply_row_state(row, eq, is_cur, &self.playing_eq);
            found |= is_cur;
        }
        if !found {
            *self.playing_eq.borrow_mut() = None;
        }
    }

    fn play_at(&mut self, i: usize) {
        if let Some(t) = self.queue.get(i).cloned() {
            self.player.load(&t.path);
            self.player.play();
            self.qidx = Some(i);
            self.current_path = Some(t.path.clone());
            self.playing = true;
            self.position = 0.0;
            self.duration = t.duration as f64;
            self.refresh_highlight();
            self.apply_visuals(Some(&t.path));
            self.mpris_sync();
            self.scroll_to_current();
            self.save_session();
        }
    }

    fn advance(&mut self, _user: bool) {
        let Some(i) = self.qidx else { return };
        let len = self.queue.len();
        if len == 0 {
            return;
        }
        if self.repeat == Repeat::One {
            self.play_at(i);
            return;
        }
        let next = if self.shuffle {
            if len <= 1 {
                i
            } else {
                let r = pseudo_random() % len;
                if r == i {
                    (i + 1) % len
                } else {
                    r
                }
            }
        } else if i + 1 < len {
            i + 1
        } else if self.repeat == Repeat::All {
            0
        } else {
            self.player.stop();
            self.playing = false;
            self.qidx = None;
            self.current_path = None;
            self.refresh_highlight();
            self.apply_visuals(None);
            self.mpris_sync();
            self.save_session();
            return;
        };
        self.play_at(next);
    }

    fn apply_visuals(&mut self, path: Option<&str>) {
        // Read the cover bytes ONCE per track change, then derive everything
        // (display texture, palette, and the MPRIS art file) from the same
        // bytes — previously the cover was read+parsed twice (here + mpris_sync)
        // and the MPRIS art was re-read on every play/pause.
        let bytes = path.and_then(cover_bytes);
        let (texture, palette) = match &bytes {
            Some(b) => (art::texture_from_bytes(b), art::palette_from_bytes(b, 4)),
            None => (None, art::default_palette()),
        };
        self.cur_art_url = match (path, &bytes) {
            (Some(p), Some(b)) => library::art_file_from_bytes(p, b),
            _ => None,
        };
        match &texture {
            Some(tex) => self.cover.set_paintable(Some(tex)),
            None => self.cover.set_icon_name(Some("meusic-music-note")),
        }
        self.np_cover.set_paintable(texture.as_ref());
        *self.palette.borrow_mut() = palette
            .iter()
            .map(|&(r, g, b)| (r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0))
            .collect();
        self.bg.queue_draw();
        self.np_bg.queue_draw();
        let (r, g, b) = palette.first().copied().unwrap_or((90, 90, 140));
        self.accent_provider.load_from_string(&format!(
            "@define-color accent_bg_color rgb({r},{g},{b}); @define-color accent_color rgb({r},{g},{b});"
        ));
    }

    /// follow_song: scroll the list so the playing track is visible (only if it's
    /// present in the current view).
    fn scroll_to_current(&self) {
        if !self.settings.follow_song {
            return;
        }
        let Some(path) = self.current_path.as_deref() else {
            return;
        };
        if let Some(idx) = self.view_tracks.iter().position(|t| t.path == path) {
            self.song_view
                .scroll_to(idx as u32, gtk::ListScrollFlags::NONE, None);
        }
    }

    /// Snapshot the current page + playback into session.json (resume source).
    /// No-op until a folder has been opened (nothing meaningful to resume).
    fn save_session(&self) {
        let Some(root) = self.root_path.clone() else {
            return;
        };
        session::save(&session::Session {
            root_path: root,
            mode: mode_key(self.mode).to_string(),
            sel_group: self.sel_group.clone(),
            track_path: self.current_path.clone(),
            position: self.position,
        });
    }

    /// Apply a saved session after its folder has been (re)scanned: restore the
    /// page (resume_startup_page) and prime the last track paused at its saved
    /// position (remember_last_played).
    fn restore_from_session(&mut self, s: session::Session) {
        if self.settings.resume_startup_page {
            self.mode = mode_from_key(&s.mode);
            self.sel_group = s.sel_group.clone();
        } else {
            self.sel_group = None;
        }
        self.rebuild_sidebar();
        self.recompute_view();

        if self.settings.remember_last_played {
            if let Some(tp) = s.track_path {
                if let Some(idx) = self.all_tracks.iter().position(|t| t.path == tp) {
                    // Resume queue = the full library (matches the original app).
                    self.queue = self.all_tracks.clone();
                    let t = self.queue[idx].clone();
                    self.player.load(&t.path);
                    self.player.pause();
                    self.qidx = Some(idx);
                    self.current_path = Some(tp);
                    self.playing = false;
                    self.position = s.position;
                    self.duration = t.duration as f64;
                    // Seek to the saved spot on the next tick (after the pipeline
                    // has a duration), the same mechanism the radio↔music swap uses.
                    self.pending_seek = Some(s.position);
                    self.refresh_highlight();
                    self.apply_visuals(Some(&t.path));
                    self.mpris_sync();
                    self.scroll_to_current();
                }
            }
        }
    }

    fn mpris_sync(&self) {
        // Central place every playback-state change passes through — also gates
        // the EQ-indicator animation (only animate while actually playing).
        self.playing_flag.set(self.playing);
        match self.app_mode {
            AppMode::Radio => {
                let status = if self.station_id.is_none() {
                    "Stopped"
                } else if self.playing {
                    "Playing"
                } else {
                    "Paused"
                };
                match self.cur_station() {
                    Some(s) => mpris::update(
                        self.radio_title.as_deref().unwrap_or(&s.name),
                        &s.name,
                        "",
                        0.0,
                        status,
                        // Cached station chip PNG (rendered once on station select)
                        // — not re-encoded on every ICY title / pause.
                        self.cur_art_url.clone(),
                        Some(&s.id),
                    ),
                    None => mpris::update("", "", "", 0.0, status, None, None),
                }
            }
            AppMode::Music => {
                let status = if self.qidx.is_none() {
                    "Stopped"
                } else if self.playing {
                    "Playing"
                } else {
                    "Paused"
                };
                match self.cur() {
                    Some(t) => mpris::update(
                        &t.title,
                        &t.artist,
                        &t.album,
                        t.duration as f64,
                        status,
                        // Cached cover file (written once in apply_visuals).
                        self.cur_art_url.clone(),
                        Some(&t.path),
                    ),
                    None => mpris::update("", "", "", 0.0, status, None, None),
                }
            }
        }
    }

    /// Rebuild the radio stations list (colored chip + name + edit/delete).
    fn rebuild_stations(&self) {
        while let Some(c) = self.stations_list.first_child() {
            self.stations_list.remove(&c);
        }
        let q = self.query.to_lowercase();
        let mut keys = Vec::new();
        let mut rows = Vec::new();
        for st in &self.stations_data {
            if !q.is_empty() && !st.name.to_lowercase().contains(&q) {
                continue;
            }
            let row = gtk::ListBoxRow::new();
            row.set_activatable(true);
            if self.station_id.as_ref() == Some(&st.id) {
                row.add_css_class("playing");
            }
            let b = gtk::Box::new(gtk::Orientation::Horizontal, 10);
            b.set_margin_top(5);
            b.set_margin_bottom(5);
            b.set_margin_start(8);
            b.set_margin_end(8);

            let chip = gtk::DrawingArea::new();
            chip.set_size_request(34, 34);
            let (cr8, cg8, cb8) = art::station_color(&st.name);
            let initials = stations::initials(&st.name);
            chip.set_draw_func(move |_, cr, w, h| {
                let (w, h) = (w as f64, h as f64);
                cr.set_source_rgb(cr8 as f64 / 255.0, cg8 as f64 / 255.0, cb8 as f64 / 255.0);
                cr.rectangle(0.0, 0.0, w, h);
                let _ = cr.fill();
                cr.set_source_rgb(1.0, 1.0, 1.0);
                cr.select_font_face(
                    "Sans",
                    gtk::cairo::FontSlant::Normal,
                    gtk::cairo::FontWeight::Bold,
                );
                cr.set_font_size(12.0);
                if let Ok(ext) = cr.text_extents(&initials) {
                    cr.move_to(
                        w / 2.0 - ext.width() / 2.0 - ext.x_bearing(),
                        h / 2.0 - ext.height() / 2.0 - ext.y_bearing(),
                    );
                    let _ = cr.show_text(&initials);
                }
            });
            b.append(&chip);

            let name = gtk::Label::new(Some(&st.name));
            name.set_xalign(0.0);
            name.set_hexpand(true);
            name.set_ellipsize(pango::EllipsizeMode::End);
            b.append(&name);

            let edit = gtk::Button::from_icon_name("meusic-pencil");
            edit.add_css_class("flat");
            let s1 = self.sender.clone();
            let id1 = st.id.clone();
            edit.connect_clicked(move |_| s1.input(Msg::EditStation(id1.clone())));
            b.append(&edit);

            let del = gtk::Button::from_icon_name("meusic-trash");
            del.add_css_class("flat");
            let s2 = self.sender.clone();
            let id2 = st.id.clone();
            del.connect_clicked(move |_| s2.input(Msg::DeleteStation(id2.clone())));
            b.append(&del);

            row.set_child(Some(&b));
            self.stations_list.append(&row);
            keys.push(st.id.clone());
            rows.push((st.id.clone(), row));
        }
        *self.station_keys.borrow_mut() = keys;
        *self.station_rows.borrow_mut() = rows;
    }

    /// Schedule a backoff reconnect for the current station (transient failure /
    /// stall / drop). Never gives up on transient errors — the last backoff step
    /// repeats. One reconnect is in flight at a time.
    fn schedule_reconnect(&mut self) {
        if self.radio_reconnect_pending || self.radio_error || self.station_id.is_none() {
            return;
        }
        let idx = (self.radio_retries as usize).min(RADIO_BACKOFF.len() - 1);
        let delay = RADIO_BACKOFF[idx];
        self.radio_retries += 1;
        self.radio_connecting = true;
        self.radio_reconnect_pending = true;
        self.radio_stall_ticks = 0;
        let sender = self.sender.clone();
        glib::timeout_add_local_once(std::time::Duration::from_secs(delay), move || {
            sender.input(Msg::ReconnectRadio);
        });
        self.mpris_sync();
    }

    /// Toggle the "playing" highlight on station rows without rebuilding (keeps
    /// the scroll position when a station is selected).
    fn refresh_station_highlight(&self) {
        for (id, row) in self.station_rows.borrow().iter() {
            if self.station_id.as_ref() == Some(id) {
                row.add_css_class("playing");
            } else {
                row.remove_css_class("playing");
            }
        }
    }

    /// A compact single-view About dialog (all info visible at once), opened by
    /// clicking the logo.
    fn open_about(&self) {
        let win = adw::Window::builder()
            .modal(true)
            .transient_for(&self.window)
            .default_width(380)
            .title("Tentang meusic")
            .build();
        let tv = adw::ToolbarView::new();
        tv.add_top_bar(&adw::HeaderBar::new());

        let b = gtk::Box::new(gtk::Orientation::Vertical, 6);
        b.set_halign(gtk::Align::Center);
        b.set_margin_top(4);
        b.set_margin_bottom(24);
        b.set_margin_start(24);
        b.set_margin_end(24);

        let icon = gtk::Image::from_icon_name("com.sarta.meusic.gtk");
        icon.set_pixel_size(88);
        b.append(&icon);

        let name = gtk::Label::new(Some("meusic"));
        name.add_css_class("title-1");
        b.append(&name);

        let ver = gtk::Label::new(Some(concat!("v", env!("CARGO_PKG_VERSION"))));
        ver.add_css_class("dim-label");
        b.append(&ver);

        let desc = gtk::Label::new(Some("Beautiful native local music player for Linux."));
        desc.set_wrap(true);
        desc.set_justify(gtk::Justification::Center);
        desc.set_max_width_chars(34);
        desc.set_margin_top(8);
        b.append(&desc);

        let sep = gtk::Separator::new(gtk::Orientation::Horizontal);
        sep.set_margin_top(14);
        sep.set_margin_bottom(10);
        b.append(&sep);

        let by = gtk::Label::new(Some("Dibuat oleh s4rt4  ·  Lisensi MIT"));
        by.add_css_class("dim-label");
        by.add_css_class("caption");
        b.append(&by);

        let stack = gtk::Label::new(Some("GTK4 · libadwaita · relm4 · GStreamer · Rust"));
        stack.add_css_class("dim-label");
        stack.add_css_class("caption");
        stack.set_wrap(true);
        stack.set_justify(gtk::Justification::Center);
        stack.set_max_width_chars(38);
        b.append(&stack);

        let gh = gtk::LinkButton::with_label(
            "https://github.com/s4rt4/meusic-linux",
            "Lihat di GitHub",
        );
        gh.set_margin_top(14);
        b.append(&gh);

        tv.set_content(Some(&b));
        win.set_content(Some(&tv));
        win.present();
    }

    /// Open the add/edit station dialog.
    fn open_station_dialog(&self, station: Option<stations::Station>) {
        let win = adw::Window::new();
        win.set_modal(true);
        win.set_transient_for(Some(&self.window));
        win.set_default_width(420);
        win.set_title(Some(if station.is_some() {
            "Edit Stasiun"
        } else {
            "Tambah Stasiun"
        }));

        let tv = adw::ToolbarView::new();
        tv.add_top_bar(&adw::HeaderBar::new());

        let content = gtk::Box::new(gtk::Orientation::Vertical, 8);
        content.set_margin_top(16);
        content.set_margin_bottom(16);
        content.set_margin_start(16);
        content.set_margin_end(16);

        let name_entry = gtk::Entry::new();
        name_entry.set_placeholder_text(Some("Mis. Prambors FM"));
        let url_entry = gtk::Entry::new();
        url_entry.set_placeholder_text(Some("https://…/stream"));
        if let Some(st) = &station {
            name_entry.set_text(&st.name);
            url_entry.set_text(&st.url);
        }

        let name_lbl = gtk::Label::new(Some("Nama"));
        name_lbl.set_xalign(0.0);
        name_lbl.add_css_class("dim-label");
        let url_lbl = gtk::Label::new(Some("URL Stream"));
        url_lbl.set_xalign(0.0);
        url_lbl.add_css_class("dim-label");
        content.append(&name_lbl);
        content.append(&name_entry);
        content.append(&url_lbl);
        content.append(&url_entry);

        let btns = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        btns.set_halign(gtk::Align::End);
        btns.set_margin_top(8);
        let cancel = gtk::Button::with_label("Batal");
        let save = gtk::Button::with_label("Simpan");
        save.add_css_class("suggested-action");
        btns.append(&cancel);
        btns.append(&save);
        content.append(&btns);

        tv.set_content(Some(&content));
        win.set_content(Some(&tv));

        let w1 = win.clone();
        cancel.connect_clicked(move |_| w1.close());

        let id = station.map(|s| s.id);
        let sender = self.sender.clone();
        let w2 = win.clone();
        let ne = name_entry.clone();
        let ue = url_entry.clone();
        save.connect_clicked(move |_| {
            let name = ne.text().to_string();
            let url = ue.text().to_string();
            if !name.trim().is_empty() && !url.trim().is_empty() {
                sender.input(Msg::SaveStation {
                    id: id.clone(),
                    name,
                    url,
                });
                w2.close();
            }
        });

        win.present();
    }
}

/// The grouping key a track belongs to in the given mode (matches sidebar keys).
fn track_key(t: &Track, m: Mode) -> String {
    match m {
        Mode::Folders => parent_dir(&t.path),
        Mode::Albums => t.album.clone(),
        Mode::Artists => t.album_artist.clone(),
        Mode::Songs => String::new(),
    }
}

/// Persisted string key for a view mode (session.json).
fn mode_key(m: Mode) -> &'static str {
    match m {
        Mode::Folders => "folders",
        Mode::Albums => "albums",
        Mode::Artists => "artists",
        Mode::Songs => "songs",
    }
}

fn mode_from_key(s: &str) -> Mode {
    match s {
        "albums" => Mode::Albums,
        "artists" => Mode::Artists,
        "songs" => Mode::Songs,
        _ => Mode::Folders,
    }
}

/// Reach a song row's child widgets — the factory builds a fixed structure:
/// `[ lead[ eq ], info[ title, sub ], dur ]`.
fn row_widgets(row: &gtk::Box) -> (gtk::DrawingArea, gtk::Label, gtk::Label, gtk::Label) {
    let lead = row.first_child().expect("row.lead");
    let eq = lead
        .first_child()
        .and_downcast::<gtk::DrawingArea>()
        .expect("lead.eq");
    let info = lead.next_sibling().expect("row.info");
    let title = info
        .first_child()
        .and_downcast::<gtk::Label>()
        .expect("info.title");
    let sub = title
        .next_sibling()
        .and_downcast::<gtk::Label>()
        .expect("info.sub");
    let dur = info
        .next_sibling()
        .and_downcast::<gtk::Label>()
        .expect("row.dur");
    (eq, title, sub, dur)
}

/// Toggle a row's "playing" look + register/release the animated EQ indicator.
fn apply_row_state(
    row: &gtk::Box,
    eq: &gtk::DrawingArea,
    is_cur: bool,
    playing_eq: &Rc<RefCell<Option<gtk::DrawingArea>>>,
) {
    if is_cur {
        row.add_css_class("playing");
        eq.set_visible(true);
        *playing_eq.borrow_mut() = Some(eq.clone());
        eq.queue_draw();
    } else {
        row.remove_css_class("playing");
        eq.set_visible(false);
    }
}


/// The adaptive gradient background (soft drifting-style blobs from the cover
/// palette over a near-black base + a vignette). Shared by the main window and
/// the Now-Playing view.
fn build_gradient_bg(palette: Rc<RefCell<PaletteF>>, power_save: Rc<Cell<bool>>) -> gtk::DrawingArea {
    let area = gtk::DrawingArea::new();
    area.set_draw_func(move |_, cr, _w, _h| {
        cr.set_source_rgb(0.039, 0.039, 0.063);
        let _ = cr.paint();
        // Power-save: flat background, no blurred blobs to composite.
        if power_save.get() {
            return;
        }
        let (w, h) = (_w as f64, _h as f64);
        let pal = palette.borrow();
        if pal.is_empty() {
            return;
        }
        let blobs = [(-0.08, -0.10, 0.68), (0.55, 0.30, 0.60), (0.06, 0.54, 0.56)];
        for (i, (lx, ty, sz)) in blobs.iter().enumerate() {
            let (r, g, b) = pal[i % pal.len()];
            let (cx, cy, rad) = (lx * w, ty * h, sz * w);
            let grad = gtk::cairo::RadialGradient::new(cx, cy, 0.0, cx, cy, rad);
            grad.add_color_stop_rgba(0.0, r, g, b, 0.85);
            grad.add_color_stop_rgba(0.68, r, g, b, 0.0);
            let _ = cr.set_source(&grad);
            let _ = cr.paint();
        }
        let vg = gtk::cairo::RadialGradient::new(w * 0.5, h * 0.4, 0.0, w * 0.5, h * 0.4, w.max(h));
        vg.add_color_stop_rgba(0.0, 0.0, 0.0, 0.0, 0.0);
        vg.add_color_stop_rgba(1.0, 0.0, 0.0, 0.0, 0.3);
        let _ = cr.set_source(&vg);
        let _ = cr.paint();
    });
    area
}

/// The Now-Playing visualizer: flat, mirror-balanced spectrum bars that grow up
/// from a baseline (not touching the bottom) with a faded, shorter downward
/// "glass" reflection.
fn build_np_visualizer(
    spectrum: Rc<RefCell<Vec<f64>>>,
    palette: Rc<RefCell<PaletteF>>,
) -> gtk::DrawingArea {
    let area = gtk::DrawingArea::new();
    area.set_draw_func(move |_, cr, w, h| {
        let (w, h) = (w as f64, h as f64);
        let buf = spectrum.borrow();
        if buf.is_empty() {
            return;
        }
        let pal = palette.borrow();
        let (r, g, b) = pal.first().copied().unwrap_or((0.6, 0.6, 0.8));
        let baseline = h * 0.60;
        let up_max = baseline - 4.0;
        const HALF: usize = 40;
        let total = HALF * 2;
        let usable = (buf.len() as f64 * 0.7) as usize;
        let step = (usable / HALF).max(1);
        let slot = w / total as f64;
        let bw = (slot * 0.55).max(1.5);
        for k in 0..total {
            // Loud low freqs in the CENTER, quiet highs at the edges → a
            // mountain shape (not a "U").
            let idx = ((k as i64 - HALF as i64).unsigned_abs() as usize).min(HALF - 1);
            let mut sum = 0.0;
            let mut count = 0.0;
            for j in 0..step {
                if let Some(&x) = buf.get(idx * step + j) {
                    sum += x;
                    count += 1.0;
                }
            }
            let v = if count > 0.0 { sum / count } else { 0.0 };
            let up = (v.powf(1.1) * up_max).max(2.0);
            let x = k as f64 * slot + (slot - bw) / 2.0;
            // Main bar (upward).
            let grad = gtk::cairo::LinearGradient::new(0.0, baseline - up, 0.0, baseline);
            grad.add_color_stop_rgba(0.0, r, g, b, 0.95);
            grad.add_color_stop_rgba(1.0, r, g, b, 0.45);
            let _ = cr.set_source(&grad);
            cr.rectangle(x, baseline - up, bw, up);
            let _ = cr.fill();
            // Reflection (downward, shorter + fainter).
            let rh = up * 0.42;
            let refl = gtk::cairo::LinearGradient::new(0.0, baseline, 0.0, baseline + rh);
            refl.add_color_stop_rgba(0.0, r, g, b, 0.28);
            refl.add_color_stop_rgba(1.0, r, g, b, 0.0);
            let _ = cr.set_source(&refl);
            cr.rectangle(x, baseline, bw, rh);
            let _ = cr.fill();
        }
    });
    area
}

/// A circular radio visualizer: a round station chip (color + initials) in the
/// center, surrounded by mirrored radial spectrum bars with a gap so they never
/// touch the chip.
fn build_radial_visualizer(
    spectrum: Rc<RefCell<Vec<f64>>>,
    palette: Rc<RefCell<PaletteF>>,
    chip: Rc<RefCell<(String, (u8, u8, u8))>>,
) -> gtk::DrawingArea {
    use std::f64::consts::{FRAC_PI_2, TAU};
    let area = gtk::DrawingArea::new();
    area.set_draw_func(move |_, cr, w, h| {
        let (w, h) = (w as f64, h as f64);
        let (cx, cy) = (w / 2.0, h / 2.0);
        let radius = w.min(h) / 2.0;
        let chip_r = (radius * 0.30).clamp(48.0, 92.0);
        let gap = 16.0;
        let inner = chip_r + gap;
        let max_bar = (radius - inner - 4.0).max(8.0);

        let (txt, (ci, cg, cb)) = chip.borrow().clone();
        let pal = palette.borrow();
        let (r, g, b) = pal
            .first()
            .copied()
            .unwrap_or((ci as f64 / 255.0, cg as f64 / 255.0, cb as f64 / 255.0));

        // Mirrored radial bars (balanced left/right).
        let buf = spectrum.borrow();
        const HALF: usize = 54;
        let total = HALF * 2;
        let usable = (buf.len() as f64 * 0.7) as usize;
        let step = (usable / HALF).max(1);
        cr.set_line_cap(gtk::cairo::LineCap::Round);
        cr.set_line_width(3.0);
        cr.set_source_rgba(r, g, b, 0.85);
        for k in 0..total {
            let idx = if k < HALF { k } else { total - 1 - k };
            let mut sum = 0.0;
            let mut count = 0.0;
            for j in 0..step {
                if let Some(&x) = buf.get(idx * step + j) {
                    sum += x;
                    count += 1.0;
                }
            }
            let v = if count > 0.0 { sum / count } else { 0.0 };
            let len = (v.powf(1.05) * max_bar).max(2.0);
            let theta = (k as f64 / total as f64) * TAU - FRAC_PI_2;
            let (ct, stt) = (theta.cos(), theta.sin());
            cr.move_to(cx + inner * ct, cy + inner * stt);
            cr.line_to(cx + (inner + len) * ct, cy + (inner + len) * stt);
            let _ = cr.stroke();
        }

        // Center chip (round) + initials.
        cr.set_source_rgb(ci as f64 / 255.0, cg as f64 / 255.0, cb as f64 / 255.0);
        cr.arc(cx, cy, chip_r, 0.0, TAU);
        let _ = cr.fill();
        cr.set_source_rgb(1.0, 1.0, 1.0);
        cr.select_font_face(
            "Sans",
            gtk::cairo::FontSlant::Normal,
            gtk::cairo::FontWeight::Bold,
        );
        cr.set_font_size(chip_r * 0.7);
        if let Ok(ext) = cr.text_extents(&txt) {
            cr.move_to(
                cx - ext.width() / 2.0 - ext.x_bearing(),
                cy - ext.height() / 2.0 - ext.y_bearing(),
            );
            let _ = cr.show_text(&txt);
        }
    });
    area
}

fn pseudo_random() -> usize {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as usize)
        .unwrap_or(0)
}

fn main() {
    if let Err(e) = gstreamer::init() {
        eprintln!("meusic: failed to init GStreamer: {e}");
    }
    // Icons + logos are baked into the binary as a GResource (see build.rs).
    gio::resources_register_include!("meusic.gresource")
        .expect("failed to register meusic resources");
    // Drop stale temp cover/station art so it doesn't accumulate forever.
    library::prune_temp_art();
    let app = RelmApp::new("com.sarta.meusic.gtk");
    app.run::<App>(());
}
