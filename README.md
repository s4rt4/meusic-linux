<p align="center">
  <img src="assets/banner/banner.png" alt="meusic" width="660">
</p>

<p align="center">
  A lightweight, native music &amp; internet-radio player for Windows<br>
  built with Tauri, React, and Rust.
</p>

<p align="center">
  <img src="https://img.shields.io/badge/Tauri-2-24C8DB?logo=tauri&logoColor=white" alt="Tauri 2">
  <img src="https://img.shields.io/badge/React-19-61DAFB?logo=react&logoColor=black" alt="React 19">
  <img src="https://img.shields.io/badge/TypeScript-5-3178C6?logo=typescript&logoColor=white" alt="TypeScript">
  <img src="https://img.shields.io/badge/Rust-stable-000000?logo=rust&logoColor=white" alt="Rust">
  <img src="https://img.shields.io/badge/Platform-Windows-0078D6?logo=windows&logoColor=white" alt="Windows">
  <img src="https://img.shields.io/badge/License-MIT-ED1E79" alt="MIT License">
</p>

---

## Overview

**meusic** is a fast, low-footprint desktop player for your local music library. Point it at a
folder and it recursively scans every track inside — including all subfolders — reading tags and
cover art, then lets you browse by folder, album, artist, or song.

The visual identity is **inspired by [Amberol](https://gitlab.gnome.org/World/amberol)**: an
adaptive background whose colors flow from the album art of whatever is playing, with a live
spectrum visualizer. The browsing layout is **inspired by [Dopamine](https://github.com/digimezzo/dopamine-windows)**:
a clean top-bar mode switcher (Folders / Albums / Artists / Songs) with a Windows Explorer–style
folder tree on the left and a track list on the right.

It also doubles as an **internet-radio player**: a built-in Music / Radio switch turns the same
window into a station browser with a live now-playing pane (station name, current song via ICY
metadata, stream quality) and the same adaptive gradient + visualizer. Stations are fully
editable (add / edit / delete) and stream through a tiny in-process Rust proxy, so the equalizer
and visualizer work on radio too and reconnect is resilient to network drops.

Because it runs on Tauri (a Rust core with the system WebView), it stays light — roughly a quarter
of the memory a comparable Electron player would use, with idle animations paused to keep the CPU
and GPU quiet. It lives in the system tray, resumes your last session on launch, reports
now-playing to Windows (so the media flyout, media keys, and desktop widgets pick it up), and
offers a power-save mode for the lightest possible footprint.

## Screenshots

The background palette is extracted from each track's cover art and transitions smoothly as the
music changes.

<table>
  <tr>
    <td><img src="assets/ss/ss1.png" alt="meusic - green theme"></td>
    <td><img src="assets/ss/ss2.png" alt="meusic - blue theme"></td>
    <td><img src="assets/ss/ss3.png" alt="meusic - purple theme"></td>
  </tr>
  <tr>
    <td><img src="assets/ss/ss4.png" alt="meusic - amber theme"></td>
    <td><img src="assets/ss/ss5.png" alt="meusic - red theme"></td>
    <td><img src="assets/ss/ss6.png" alt="meusic - teal theme"></td>
  </tr>
</table>

## Features

- **Recursive folder scanning** — finds every track in a folder and all its subfolders.
- **Wide format support** — MP3, FLAC, M4A / AAC, OGG, Opus, WAV, AIFF, WMA.
- **Adaptive gradient UI** — the background and accent colors are derived from the current
  cover art and cross-fade on track change.
- **Four browsing modes** — Folders (Explorer-style tree, with an "open folder" icon on the
  active folder and a visualizer badge on the one playing), Albums, Artists, and Songs.
- **Cover art** — read from embedded tags, with a fallback to folder images
  (`cover.jpg`, `folder.jpg`, and similar); downscaled and cached to stay light.
- **Now-playing details** — title, artist, album, audio format, and bitrate; folder headers
  show total runtime and artist/album counts.
- **Full transport** — play / pause, next / previous, seek, volume, shuffle, and repeat
  (off / all / one); mouse-wheel over the volume control adjusts it.
- **6-band equalizer** with presets (Flat, Bass, Vocal, Treble) and a spectrum visualizer.
- **Now Playing view** — full-screen cover + visualizer, opened from the bottom bar.
- **System tray** — tray icon with minimize-to-tray and close-to-tray.
- **Tray mini-player** — a compact popup (cover, seek, volume, transport) from the tray icon.
- **Resume** — remembers the last folder, page, track, and playback position across restarts.
- **Follow song** — the list auto-scrolls to the track that's playing.
- **Power-save mode** — flat background and paused animations for the lightest footprint.
- **Global search** across title, artist, and album.
- **Responsive chrome** — the top and bottom bars collapse to icons on narrow windows.
- **Settings** — toggles for resume, follow-song, tray behavior, and volume step.
- **Windows media controls (SMTC)** — now-playing (title / artist / album / cover and
  position) is published to Windows, so the media flyout, keyboard media keys, and desktop
  now-playing widgets read it; media keys control playback.

### Radio

- **Internet-radio mode** — a Music / Radio switch (top of the Settings menu) turns the window
  into a station browser: a station list on the left and a now-playing pane on the right with the
  live spectrum visualizer and adaptive gradient (tinted from the station's color).
- **Live stream info** — current song via ICY metadata, plus stream codec and bitrate.
- **Editable stations** — add, edit, and delete stations; saved to disk and seeded from a bundled
  Indonesian-radio list on first run.
- **In-process stream proxy** — a small Rust loopback server pipes the stream so the equalizer and
  visualizer work on radio, plain-`http://` stations play, and ICY metadata is parsed out.
- **Resilient connection** — exponential-backoff reconnect, a stall watchdog, and automatic
  recovery when the network returns; permanent failures (bad URL / auth) are surfaced clearly.

## Tech Stack

| Layer    | Technology                          | Responsibility                                            |
| -------- | ----------------------------------- | --------------------------------------------------------- |
| Backend  | Rust (`lofty`, `walkdir`, `rayon`)  | Recursive scan, tag and cover-art extraction (parallel)   |
| Radio    | Rust (`tiny_http`, `ureq`)          | Loopback streaming proxy: CORS-clean piping + ICY metadata parsing |
| Bridge   | Tauri 2 (commands, tray, 2 windows) | `scan_folder` / `get_cover`, asset protocol, system tray, main↔mini-player events |
| Frontend | React + TypeScript + Vite + Tailwind | UI, state, settings, session persistence                 |
| Audio    | Web Audio API + Media Session       | Playback, 6-band equalizer, analyser for the visualizer, Windows SMTC now-playing |

## Getting Started

### Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) (stable)
- [Node.js](https://nodejs.org/) 18 or newer
- [pnpm](https://pnpm.io/installation)
- WebView2 runtime (preinstalled on Windows 11)

### Development

```bash
pnpm install
pnpm tauri dev
```

### Build

```bash
pnpm tauri build
```

The installer is produced under `src-tauri/target/release/bundle/`.

## Project Structure

```
src/
  audio/engine.ts        Web Audio graph (equalizer + analyser), singleton
  hooks/usePlayer.ts     Playback state and queue (music + radio, reconnect)
  hooks/useSettings.ts   Persisted user settings
  hooks/useStations.ts   Radio station list (persisted, seeded from bundled JSON)
  lib/                   api, colors (palette), stationColor, views (tree/groups),
                         format, image (cover downscale), miniState (mini-player IPC types)
  components/            TopBar, FolderTree, GroupList, Library, BottomBar,
                         NowPlayingOverlay, GradientBackground, Visualizer, Equalizer,
                         SettingsMenu, MiniPlayer, RadioList, RadioNowPlaying,
                         StationDialog, icons
  assets/radio-stations.json   Bundled seed station list
  App.tsx                Main-window orchestration (modes, session, tray, IPC, SMTC)
  main.tsx               Renders App or MiniPlayer based on the window label
src-tauri/
  src/lib.rs             scan_folder + get_cover commands, system tray, mini-player
  src/radio.rs           Loopback radio streaming proxy (ICY strip, reconnect-friendly)
  tauri.conf.json        main + miniplayer windows, asset-protocol config
  capabilities/          window/event/dialog permissions
```

## Acknowledgments

- **UI inspired by [Amberol](https://gitlab.gnome.org/World/amberol)** — the adaptive,
  cover-art-reactive background and visualizer.
- **Layout inspired by [Dopamine](https://github.com/digimezzo/dopamine-windows)** — the
  Folders / Albums / Artists / Songs browsing model.

These projects are independent works under their own licenses; meusic shares none of their code
and only draws on them as design inspiration.

## License

Released under the [MIT License](LICENSE).
