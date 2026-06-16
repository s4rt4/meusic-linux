import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { GradientBackground } from "./components/GradientBackground";
import { Library } from "./components/Library";
import { TopBar, type Mode } from "./components/TopBar";
import { BottomBar } from "./components/BottomBar";
import { NowPlayingOverlay } from "./components/NowPlayingOverlay";
import { AboutDialog } from "./components/AboutDialog";
import { FolderTree } from "./components/FolderTree";
import { GroupList } from "./components/GroupList";
import { RadioList } from "./components/RadioList";
import { RadioNowPlaying } from "./components/RadioNowPlaying";
import { StationDialog } from "./components/StationDialog";
import { Album, Artist } from "./components/icons";
import { usePlayer } from "./hooks/usePlayer";
import { useSettings } from "./hooks/useSettings";
import { useStations } from "./hooks/useStations";
import { engine } from "./audio/engine";
import {
  getCover,
  loadStore,
  pickFolder,
  prefetchRadioProxy,
  saveStore,
  scanFolder,
  setTrayVisible,
} from "./lib/api";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { emit, listen } from "@tauri-apps/api/event";
import {
  MP_STATE,
  MP_CMD,
  MP_REQUEST,
  type MiniState,
  type MiniCmd,
} from "./lib/miniState";
import { extractPalette } from "./lib/colors";
import { stationPalette } from "./lib/stationColor";
import { fmtDurationLong } from "./lib/format";
import { downscaleDataUri } from "./lib/image";
import {
  ancestorPaths,
  buildFolderTree,
  groupByAlbum,
  groupByArtist,
  indexTree,
  type FolderNode,
} from "./lib/views";
import type {
  AppMode,
  RadioErrorEvent,
  RadioMetaEvent,
  RGB,
  Station,
  Track,
} from "./types";

const DEFAULT_PALETTE: RGB[] = [
  [108, 99, 196],
  [70, 120, 170],
  [150, 90, 160],
];

const SESSION_KEY = "meusic.session"; // fast localStorage cache
const SESSION_STORE = "session"; // durable file-backed store (session.json)

interface Session {
  rootPath: string;
  mode: Mode;
  selFolder: string;
  trackPath: string | null;
  position: number;
  volume: number;
}

function parseSession(raw: string | null): Session | null {
  if (!raw) return null;
  try {
    return JSON.parse(raw) as Session;
  } catch {
    return null;
  }
}

// The file store is the source of truth (localStorage isn't flushed reliably on
// OS shutdown); fall back to the localStorage cache if the file isn't there yet.
async function loadSession(): Promise<Session | null> {
  try {
    const fromFile = parseSession(await loadStore(SESSION_STORE));
    if (fromFile) return fromFile;
  } catch {
    /* ignore */
  }
  return parseSession(localStorage.getItem(SESSION_KEY));
}

function App() {
  const player = usePlayer();
  const { settings, update } = useSettings();
  const stations = useStations();

  // Top-level view (local music vs radio). Persisted to localStorage like
  // powerSave — a trivial preference, not part of the durable session.
  const [appMode, setAppMode] = useState<AppMode>(() =>
    localStorage.getItem("meusic.appMode") === "radio" ? "radio" : "music"
  );
  const onAppMode = useCallback((m: AppMode) => {
    setAppMode(m);
    localStorage.setItem("meusic.appMode", m);
    // One audio engine, one thing playing: switching views stops whatever's on
    // air (and cancels any pending radio reconnect) so the visible bottom bar
    // always matches what you hear.
    playerRef.current.stopRadio();
  }, []);

  // Radio: the add/edit dialog state (the playing station lives in usePlayer).
  const [stationDialog, setStationDialog] = useState<{
    open: boolean;
    editing: Station | null;
  }>({ open: false, editing: null });
  const currentStation = player.radioStation;

  // The full scanned library — independent of the playback queue, so browsing
  // never disturbs what's playing and vice-versa.
  const [library, setLibrary] = useState<Track[]>([]);
  const [rootPath, setRootPath] = useState<string | null>(null);

  const [mode, setMode] = useState<Mode>("folders");
  const [query, setQuery] = useState("");
  const [scanning, setScanning] = useState(false);
  const [coverUrl, setCoverUrl] = useState<string | null>(null);
  const [palette, setPalette] = useState<RGB[]>(DEFAULT_PALETTE);
  const [showEq, setShowEq] = useState(false);
  const [eqGains, setEqGains] = useState<number[]>(Array(6).fill(0));
  const [overlayOpen, setOverlayOpen] = useState(false);
  const [aboutOpen, setAboutOpen] = useState(false);
  const [windowActive, setWindowActive] = useState(true);
  const [powerSave, setPowerSave] = useState(
    () => localStorage.getItem("meusic.powerSave") === "1"
  );

  // Per-mode selection.
  const [selFolder, setSelFolder] = useState("");
  const [selAlbum, setSelAlbum] = useState("");
  const [selArtist, setSelArtist] = useState("");
  const [expanded, setExpanded] = useState<Set<string>>(new Set());

  const accent = palette[0] ?? DEFAULT_PALETTE[0];
  const reqId = useRef(0);
  const coverCache = useRef<Map<string, string | null>>(new Map());
  const sessionRef = useRef<Session | null>(null);
  const settingsRef = useRef(settings);
  settingsRef.current = settings;
  const playerRef = useRef(player);
  playerRef.current = player;
  const miniStateRef = useRef<MiniState | null>(null);

  // Sync tray icon visibility with the setting.
  useEffect(() => {
    setTrayVisible(settings.trayIcon).catch(() => {});
  }, [settings.trayIcon]);

  // Minimize-to-tray / close-to-tray (gated on tray icon being enabled).
  useEffect(() => {
    const win = getCurrentWindow();
    let unClose: (() => void) | undefined;
    let unResize: (() => void) | undefined;
    (async () => {
      unClose = await win.onCloseRequested(async (e) => {
        const s = settingsRef.current;
        if (s.trayIcon && s.closeToTray) {
          e.preventDefault();
          await win.hide();
        }
      });
      unResize = await win.onResized(async () => {
        const s = settingsRef.current;
        if (s.trayIcon && s.minimizeToTray && (await win.isMinimized())) {
          await win.hide();
        }
      });
    })();
    return () => {
      unClose?.();
      unResize?.();
    };
  }, []);

  // Mini-player bridge: handle control commands and state requests from the
  // tray popup (uses refs so listeners attach once).
  useEffect(() => {
    let unCmd: (() => void) | undefined;
    let unReq: (() => void) | undefined;
    listen<MiniCmd>(MP_CMD, (e) => {
      const p = playerRef.current;
      const c = e.payload;
      switch (c.action) {
        case "toggle":
          p.toggle();
          break;
        case "next":
          p.next();
          break;
        case "prev":
          p.prev();
          break;
        case "seek":
          p.seek(c.value);
          break;
        case "volume":
          p.setVolume(c.value);
          break;
        case "show-main": {
          const w = getCurrentWindow();
          void w.show();
          void w.unminimize();
          void w.setFocus();
          break;
        }
      }
    }).then((u) => (unCmd = u));
    listen(MP_REQUEST, () => {
      if (miniStateRef.current) void emit(MP_STATE, miniStateRef.current);
    }).then((u) => (unReq = u));
    return () => {
      unCmd?.();
      unReq?.();
    };
  }, []);

  // Radio stream metadata + errors pushed by the proxy.
  useEffect(() => {
    prefetchRadioProxy();
    const uns: Array<() => void> = [];
    listen<RadioMetaEvent>("radio:meta", (e) => playerRef.current.applyRadioMeta(e.payload)).then(
      (u) => uns.push(u)
    );
    listen<RadioErrorEvent>("radio:error", (e) =>
      playerRef.current.applyRadioError(e.payload)
    ).then((u) => uns.push(u));
    return () => uns.forEach((u) => u());
  }, []);

  // Persist the session periodically + on hide/close, so resume works.
  useEffect(() => {
    const save = () => {
      const s = sessionRef.current;
      if (!s?.rootPath) return; // nothing loaded yet — don't clobber saved data
      const json = JSON.stringify(s);
      try {
        localStorage.setItem(SESSION_KEY, json); // fast cache
      } catch {
        /* ignore */
      }
      void saveStore(SESSION_STORE, json).catch(() => {}); // durable on disk
    };
    const id = window.setInterval(save, 5000);
    const onHide = () => document.hidden && save();
    document.addEventListener("visibilitychange", onHide);
    window.addEventListener("beforeunload", save);
    return () => {
      window.clearInterval(id);
      document.removeEventListener("visibilitychange", onHide);
      window.removeEventListener("beforeunload", save);
      save();
    };
  }, []);

  // Derived browse structures.
  const tree = useMemo(
    () => (rootPath ? buildFolderTree(library, rootPath) : null),
    [library, rootPath]
  );
  const treeIndex = useMemo(
    () => (tree ? indexTree(tree) : new Map<string, FolderNode>()),
    [tree]
  );
  const albums = useMemo(() => groupByAlbum(library), [library]);
  const artists = useMemo(() => groupByArtist(library), [library]);

  // Default the folder selection to the root only when the current selection
  // isn't valid for the loaded tree (new folder / first load). A restored
  // selFolder that exists in the tree is kept, so resume works.
  useEffect(() => {
    if (tree && !treeIndex.has(selFolder)) {
      setSelFolder(tree.path);
      setExpanded(new Set([tree.path]));
    }
  }, [tree, treeIndex, selFolder]);
  useEffect(() => {
    if (albums.length && !albums.some((a) => a.key === selAlbum))
      setSelAlbum(albums[0].key);
  }, [albums, selAlbum]);
  useEffect(() => {
    if (artists.length && !artists.some((a) => a.key === selArtist))
      setSelArtist(artists[0].key);
  }, [artists, selArtist]);

  // Restore last session once on startup: re-scan the last folder, restore the
  // last page/folder, and load the last track (paused) at its saved position.
  const restoredRef = useRef(false);
  useEffect(() => {
    if (restoredRef.current) return;
    restoredRef.current = true;
    (async () => {
      const s = await loadSession();
      if (!s?.rootPath) return;
      setScanning(true);
      try {
        const tracks = await scanFolder(s.rootPath);
        setLibrary(tracks);
        setRootPath(s.rootPath);
        if (typeof s.volume === "number") player.setVolume(s.volume);
        if (settings.resumeStartupPage) {
          if (s.mode) setMode(s.mode);
          if (s.selFolder) {
            setSelFolder(s.selFolder);
            setExpanded(new Set(ancestorPaths(s.selFolder, s.rootPath)));
          }
        }
        if (settings.rememberLastPlayed && s.trackPath) {
          const idx = tracks.findIndex((t) => t.path === s.trackPath);
          if (idx >= 0) player.loadInList(tracks, idx, s.position || 0);
        }
      } finally {
        setScanning(false);
      }
    })();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Cover + adaptive palette for the playing track.
  const currentPath = player.current?.path;
  useEffect(() => {
    const id = ++reqId.current;
    // Radio mode drives the palette from the station color (see effect below).
    if (appMode === "radio") return;
    if (!currentPath) {
      setCoverUrl(null);
      setPalette(DEFAULT_PALETTE);
      return;
    }
    const apply = async (cover: string | null) => {
      if (id !== reqId.current) return;
      setCoverUrl(cover);
      if (cover) {
        const pal = await extractPalette(cover, 4);
        if (id === reqId.current) setPalette(pal);
      } else {
        setPalette(DEFAULT_PALETTE);
      }
    };
    const cache = coverCache.current;
    const cached = cache.get(currentPath);
    if (cached !== undefined) {
      void apply(cached);
      return;
    }
    (async () => {
      const raw = await getCover(currentPath).catch(() => null);
      // Downscale before caching/displaying to keep memory bounded.
      const cover = raw ? await downscaleDataUri(raw, 512) : null;
      if (id !== reqId.current) return;
      cache.set(currentPath, cover);
      // LRU-ish cap: drop oldest entries so the cache can't grow unbounded.
      if (cache.size > 24) {
        const oldest = cache.keys().next().value;
        if (oldest !== undefined) cache.delete(oldest);
      }
      void apply(cover);
    })();
  }, [currentPath, appMode]);

  // Radio mode: the adaptive gradient follows the station's tile color.
  useEffect(() => {
    if (appMode !== "radio") return;
    setPalette(currentStation ? stationPalette(currentStation.name) : DEFAULT_PALETTE);
  }, [appMode, currentStation]);

  // ---- Windows media controls (SMTC) via the Media Session API ----------
  // WebView2/Chromium bridges navigator.mediaSession → Windows System Media
  // Transport Controls, which is what desktop widgets / the media flyout /
  // media keys read. Without this, only the app name ("meusic") shows.

  // Media-key / flyout action handlers (set once).
  useEffect(() => {
    const ms = navigator.mediaSession;
    if (!ms) return;
    const set = (a: MediaSessionAction, cb: (() => void) | null) => {
      try {
        ms.setActionHandler(a, cb);
      } catch {
        /* unsupported action — ignore */
      }
    };
    const toggle = () => {
      const p = playerRef.current;
      if (p.mediaKind === "radio") p.radioToggle();
      else p.toggle();
    };
    set("play", toggle);
    set("pause", toggle);
    set("nexttrack", () => {
      const p = playerRef.current;
      if (p.mediaKind !== "radio") p.next();
    });
    set("previoustrack", () => {
      const p = playerRef.current;
      if (p.mediaKind !== "radio") p.prev();
    });
    return () => {
      (["play", "pause", "nexttrack", "previoustrack"] as MediaSessionAction[]).forEach((a) =>
        set(a, null)
      );
    };
  }, []);

  // Now-playing metadata + playback state.
  useEffect(() => {
    const ms = navigator.mediaSession;
    if (!ms) return;
    if (player.mediaKind === "radio" && player.radioStation) {
      const st = player.radioStation;
      const song = player.radioMeta.title;
      ms.metadata = new MediaMetadata({
        title: song || st.name,
        artist: song ? st.name : player.radioMeta.name || "Radio",
        album: "",
      });
      ms.playbackState = player.radioPlaying ? "playing" : "paused";
    } else if (player.current) {
      const t = player.current;
      ms.metadata = new MediaMetadata({
        title: t.title || "",
        artist: t.artist || "",
        album: t.album || "",
        artwork: coverUrl ? [{ src: coverUrl, sizes: "512x512", type: "image/jpeg" }] : [],
      });
      ms.playbackState = player.isPlaying ? "playing" : "paused";
    } else {
      ms.metadata = null;
      ms.playbackState = "none";
    }
  }, [
    player.mediaKind,
    player.current,
    player.isPlaying,
    player.radioStation,
    player.radioPlaying,
    player.radioMeta.title,
    player.radioMeta.name,
    coverUrl,
  ]);

  // Timeline position (music only — radio is live, so clear it). Windows
  // extrapolates between updates, so a per-second refresh keeps it accurate.
  useEffect(() => {
    const ms = navigator.mediaSession;
    if (!ms || typeof ms.setPositionState !== "function") return;
    const a = engine.audio;
    try {
      if (player.mediaKind === "music" && Number.isFinite(a.duration) && a.duration > 0) {
        ms.setPositionState({
          duration: a.duration,
          position: Math.min(Math.max(0, a.currentTime), a.duration),
          playbackRate: a.playbackRate || 1,
        });
      } else {
        ms.setPositionState();
      }
    } catch {
      /* invalid state (e.g. live stream) — ignore */
    }
  }, [player.mediaKind, player.currentTime, player.duration]);

  // Mini-player: keep a fresh snapshot and broadcast it on change + every 1s.
  miniStateRef.current = {
    hasTrack: player.current !== null,
    title: player.current?.title ?? "meusic",
    artist: player.current?.artist ?? "",
    coverUrl,
    isPlaying: player.isPlaying,
    position: player.currentTime,
    duration: player.duration,
    volume: player.volume,
    accent,
  };
  useEffect(() => {
    if (miniStateRef.current) void emit(MP_STATE, miniStateRef.current);
  }, [player.isPlaying, currentPath, player.duration, player.volume, coverUrl, accent]);
  useEffect(() => {
    const id = window.setInterval(() => {
      if (miniStateRef.current) void emit(MP_STATE, miniStateRef.current);
    }, 1000);
    return () => window.clearInterval(id);
  }, []);

  const handlePickFolder = useCallback(async () => {
    const path = await pickFolder();
    if (!path) return;
    setScanning(true);
    try {
      const tracks = await scanFolder(path);
      setLibrary(tracks);
      setRootPath(path);
    } finally {
      setScanning(false);
    }
  }, []);

  // Radio actions.
  const handlePlayStation = useCallback(
    (s: Station) => void player.playStation(s),
    [player]
  );
  const handleAddStation = useCallback(
    () => setStationDialog({ open: true, editing: null }),
    []
  );
  const handleEditStation = useCallback(
    (s: Station) => setStationDialog({ open: true, editing: s }),
    []
  );
  const handleDeleteStation = useCallback(
    (s: Station) => {
      if (!window.confirm(`Hapus stasiun "${s.name}"?`)) return;
      // Stop it if it's the one playing.
      if (player.radioStation?.id === s.id) engine.audio.pause();
      stations.remove(s.id);
    },
    [stations, player.radioStation]
  );
  const handleSaveStation = useCallback(
    (name: string, url: string) => {
      setStationDialog((d) => {
        if (d.editing) stations.update(d.editing.id, name, url);
        else stations.add(name, url);
        return { open: false, editing: null };
      });
    },
    [stations]
  );

  const handleEqChange = useCallback((index: number, gainDb: number) => {
    engine.setEq(index, gainDb);
    setEqGains((g) => g.map((v, i) => (i === index ? gainDb : v)));
  }, []);

  const handleEqPreset = useCallback((gains: number[]) => {
    gains.forEach((g, i) => engine.setEq(i, g));
    setEqGains(gains);
  }, []);

  const toggleExpand = useCallback((path: string) => {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  }, []);

  const selectFolder = useCallback((path: string) => {
    setSelFolder(path);
    setExpanded((prev) => new Set(prev).add(path));
  }, []);

  // Resolve the current view's tracks + title. A non-empty search overrides the
  // mode and filters the whole library; the filtered list is what gets shown,
  // counted, and played — so the queue and header always match the view.
  const searching = query.trim().length > 0;
  const { viewTracks, viewTitle } = useMemo<{
    viewTracks: Track[];
    viewTitle: string;
  }>(() => {
    if (searching) {
      const q = query.trim().toLowerCase();
      return {
        viewTracks: library.filter(
          (t) =>
            t.title.toLowerCase().includes(q) ||
            t.artist.toLowerCase().includes(q) ||
            t.album.toLowerCase().includes(q)
        ),
        viewTitle: `Pencarian "${query.trim()}"`,
      };
    }
    if (mode === "songs") return { viewTracks: library, viewTitle: "Semua Lagu" };
    if (mode === "folders") {
      const node = treeIndex.get(selFolder) ?? tree;
      return { viewTracks: node?.tracks ?? [], viewTitle: node?.name ?? "" };
    }
    if (mode === "albums") {
      const g = albums.find((a) => a.key === selAlbum);
      return { viewTracks: g?.tracks ?? [], viewTitle: g?.label ?? "" };
    }
    const g = artists.find((a) => a.key === selArtist);
    return { viewTracks: g?.tracks ?? [], viewTitle: g?.label ?? "" };
  }, [searching, query, mode, library, treeIndex, tree, selFolder, albums, selAlbum, artists, selArtist]);

  const onPlay = useCallback(
    (i: number) => player.playInList(viewTracks, i),
    [player, viewTracks]
  );

  // Retarget the play queue when a search is applied, refined, or cleared, so
  // next/prev follow what's on screen (Dopamine behavior). Folder/album/artist
  // browsing deliberately leaves the queue alone, so casual browsing never
  // changes what plays next. Skips the first run so it can't clobber the queue
  // restored on startup.
  const viewTracksRef = useRef(viewTracks);
  viewTracksRef.current = viewTracks;
  const appModeRef = useRef(appMode);
  appModeRef.current = appMode;
  const skipFirstSync = useRef(true);
  useEffect(() => {
    if (skipFirstSync.current) {
      skipFirstSync.current = false;
      return;
    }
    // In radio mode the search box filters stations, not the music library —
    // don't let it retarget the playback queue.
    if (appModeRef.current !== "music") return;
    playerRef.current.syncQueue(viewTracksRef.current);
  }, [query]);

  // Header summary: count, total runtime, and (when meaningful) artist/album counts.
  const summary = (() => {
    const totalSec = viewTracks.reduce((a, t) => a + (t.duration || 0), 0);
    const artists = new Set(viewTracks.map((t) => t.album_artist || t.artist)).size;
    const albums = new Set(viewTracks.map((t) => t.album)).size;
    const parts = [`${viewTracks.length} lagu`];
    if (totalSec > 0) parts.push(fmtDurationLong(totalSec));
    if (artists > 1) parts.push(`${artists} artis`);
    if (albums > 1) parts.push(`${albums} album`);
    return parts.join(" · ");
  })();

  // Pause heavy animations (gradient + visualizer) when the window is unfocused
  // or minimized, to spare GPU/CPU in the background.
  useEffect(() => {
    const onFocus = () => setWindowActive(true);
    const onBlur = () => setWindowActive(false);
    const onVis = () => setWindowActive(!document.hidden);
    window.addEventListener("focus", onFocus);
    window.addEventListener("blur", onBlur);
    document.addEventListener("visibilitychange", onVis);
    return () => {
      window.removeEventListener("focus", onFocus);
      window.removeEventListener("blur", onBlur);
      document.removeEventListener("visibilitychange", onVis);
    };
  }, []);

  // Animations run only while playing, with the window focused, and power-save off.
  const animationsActive = player.isPlaying && windowActive && !powerSave;

  const togglePowerSave = useCallback(() => {
    setPowerSave((p) => {
      const next = !p;
      localStorage.setItem("meusic.powerSave", next ? "1" : "0");
      return next;
    });
  }, []);

  // Folder (normalized path) that contains the currently-playing track.
  const playingFolderPath = currentPath
    ? currentPath.replace(/\\/g, "/").replace(/\/[^/]*$/, "")
    : null;

  const showSidebar = !searching && mode !== "songs";
  const hasLibrary = library.length > 0;

  // Keep the latest session snapshot for the periodic saver.
  sessionRef.current = rootPath
    ? {
        rootPath,
        mode,
        selFolder,
        trackPath: player.current?.path ?? null,
        position: player.currentTime,
        volume: player.volume,
      }
    : null;

  return (
    <div className="relative flex h-screen w-screen flex-col overflow-hidden">
      <GradientBackground
        palette={palette}
        active={animationsActive}
        enabled={!powerSave}
      />

      <TopBar
        accent={accent}
        mode={mode}
        onMode={setMode}
        query={query}
        onQuery={setQuery}
        onPick={handlePickFolder}
        scanning={scanning}
        powerSave={powerSave}
        settings={settings}
        onUpdateSetting={update}
        onAbout={() => setAboutOpen(true)}
        appMode={appMode}
        onAppMode={onAppMode}
      />

      <main className="min-h-0 flex-1 overflow-hidden px-6 pb-4 pt-1">
        {appMode === "radio" ? (
          <div className="flex h-full gap-5">
            <aside className="glass w-[320px] shrink-0 overflow-hidden rounded-2xl">
              <RadioList
                stations={stations.stations}
                query={query}
                currentId={currentStation?.id ?? null}
                accent={accent}
                onPlay={handlePlayStation}
                onAdd={handleAddStation}
                onEdit={handleEditStation}
                onDelete={handleDeleteStation}
              />
            </aside>
            <RadioNowPlaying
              station={currentStation}
              accent={accent}
              playing={player.radioPlaying}
              title={player.radioMeta.title}
              codec={player.radioMeta.codec}
              bitrate={player.radioMeta.bitrate}
              error={player.radioError}
              status={player.radioStatus}
              active={player.radioPlaying && windowActive && !powerSave}
              powerSave={powerSave}
            />
          </div>
        ) : !hasLibrary ? (
          <div className="glass flex h-full flex-col items-center justify-center gap-3 rounded-2xl text-center text-white/45">
            <div className="text-5xl">🎵</div>
            <p className="text-sm">
              Belum ada lagu. Klik <b className="text-white/75">Buka Folder</b> untuk
              memindai koleksi musikmu (termasuk semua subfolder).
            </p>
          </div>
        ) : (
          <div className="flex h-full gap-5">
            {showSidebar && (
              <aside className="glass w-[300px] shrink-0 overflow-hidden rounded-2xl">
                <div className="h-full overflow-y-auto">
                  {mode === "folders" && tree && (
                    <FolderTree
                      root={tree}
                      selectedPath={selFolder}
                      playingPath={playingFolderPath}
                      accent={accent}
                      expanded={expanded}
                      onSelect={selectFolder}
                      onToggle={toggleExpand}
                    />
                  )}
                  {mode === "albums" && (
                    <GroupList
                      items={albums}
                      selectedKey={selAlbum}
                      onSelect={setSelAlbum}
                      icon={Album}
                    />
                  )}
                  {mode === "artists" && (
                    <GroupList
                      items={artists}
                      selectedKey={selArtist}
                      onSelect={setSelArtist}
                      icon={Artist}
                    />
                  )}
                </div>
              </aside>
            )}

            <section className="glass flex min-w-0 flex-1 flex-col overflow-hidden rounded-2xl">
              <header className="flex shrink-0 items-center gap-2 border-b border-white/8 px-4 py-3">
                <div className="min-w-0">
                  <div className="truncate text-sm font-semibold text-white">
                    {viewTitle}
                  </div>
                  <div className="text-xs text-white/45">{summary}</div>
                </div>
              </header>
              <div className="min-h-0 flex-1 overflow-y-auto">
                <Library
                  tracks={viewTracks}
                  currentPath={currentPath}
                  isPlaying={player.isPlaying}
                  onPlay={onPlay}
                  emptyMessage={
                    searching
                      ? "Tidak ada hasil."
                      : "Folder ini tidak punya lagu langsung — buka subfoldernya."
                  }
                  followSong={settings.followSong}
                />
              </div>
            </section>
          </div>
        )}
      </main>

      <BottomBar
        accent={accent}
        track={player.current}
        coverUrl={coverUrl}
        isPlaying={player.isPlaying}
        currentTime={player.currentTime}
        duration={player.duration}
        volume={player.volume}
        repeat={player.repeat}
        shuffle={player.shuffle}
        hasTrack={player.current !== null}
        showEq={showEq}
        eqGains={eqGains}
        onSeek={player.seek}
        onToggle={player.toggle}
        onNext={player.next}
        onPrev={player.prev}
        onVolume={player.setVolume}
        onCycleRepeat={player.cycleRepeat}
        onToggleShuffle={player.toggleShuffle}
        onToggleEq={() => setShowEq((s) => !s)}
        onEqChange={handleEqChange}
        onEqPreset={handleEqPreset}
        onExpand={() => !powerSave && setOverlayOpen(true)}
        powerSave={powerSave}
        onTogglePowerSave={togglePowerSave}
        volumeStep={settings.volumeScrollStep}
        appMode={appMode}
        radio={{
          station: currentStation,
          playing: player.radioPlaying,
          title: player.radioMeta.title,
          codec: player.radioMeta.codec,
          bitrate: player.radioMeta.bitrate,
          error: player.radioError,
          status: player.radioStatus,
          onToggle: player.radioToggle,
        }}
      />

      <NowPlayingOverlay
        open={overlayOpen && !powerSave}
        onClose={() => setOverlayOpen(false)}
        track={player.current}
        coverUrl={coverUrl}
        accent={accent}
        active={animationsActive}
      />

      <AboutDialog open={aboutOpen} onClose={() => setAboutOpen(false)} accent={accent} />

      <StationDialog
        open={stationDialog.open}
        station={stationDialog.editing}
        accent={accent}
        onClose={() => setStationDialog({ open: false, editing: null })}
        onSave={handleSaveStation}
      />
    </div>
  );
}

export default App;
