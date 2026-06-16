import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { engine } from "../audio/engine";
import { radioProxyUrl, trackUrl } from "../lib/api";
import { formatPlaybackError } from "../lib/playbackError";
import type {
  RadioErrorEvent,
  RadioMeta,
  RadioMetaEvent,
  RadioStatus,
  RepeatMode,
  Station,
  Track,
} from "../types";

// Reconnect backoff (seconds); the last value repeats indefinitely so a long
// network outage keeps being retried (radio-win-style, persistent).
const RADIO_BACKOFF = [2, 5, 10, 30, 60];
// Playing but no playback progress for this long → treat as a silent stall.
const RADIO_STALL_MS = 12000;

const EMPTY_META: RadioMeta = { title: null, codec: null, bitrate: null, name: null };

/** Forward a playback failure to meusic.log so silent decode errors are visible. */
function logPlaybackError(where: string, err?: unknown) {
  const a = engine.audio;
  const me = a.error;
  const message = formatPlaybackError(
    where,
    {
      code: me?.code,
      mediaMessage: me?.message || undefined,
      readyState: a.readyState,
      networkState: a.networkState,
      src: decodeURIComponent(a.currentSrc || a.src),
    },
    err,
  );
  invoke("log_event", { level: "ERROR", message }).catch(() => {});
}

/**
 * Central playback state + controls. Owns the queue and drives the singleton
 * AudioEngine. Mutable values the <audio> event handlers depend on are mirrored
 * into refs so the listeners (attached once) always read current values.
 */
export function usePlayer() {
  const [queue, setQueue] = useState<Track[]>([]);
  const [index, setIndex] = useState(-1);
  // The track that's actually loaded in the engine. Kept separate from
  // queue[index] so retargeting the queue (e.g. applying a search that excludes
  // the playing track) never blanks what's playing in the UI.
  const [currentTrack, setCurrentTrack] = useState<Track | null>(null);
  const [isPlaying, setPlaying] = useState(false);
  const [currentTime, setCurrentTime] = useState(0);
  const [duration, setDuration] = useState(0);
  const [volume, setVolume] = useState(1);
  const [repeat, setRepeat] = useState<RepeatMode>("off");
  const [shuffle, setShuffle] = useState(false);

  // Radio: the shared <audio> element plays either a music track or a radio
  // stream. `mediaKind` says which, so the UI shows the right now-playing state.
  const [mediaKind, setMediaKind] = useState<"music" | "radio">("music");
  const [radioStation, setRadioStation] = useState<Station | null>(null);
  const [radioMeta, setRadioMeta] = useState<RadioMeta>(EMPTY_META);
  const [radioError, setRadioError] = useState<string | null>(null);
  const [radioStatus, setRadioStatus] = useState<RadioStatus>("idle");

  const queueRef = useRef(queue);
  const indexRef = useRef(index);
  const currentTrackRef = useRef(currentTrack);
  const repeatRef = useRef(repeat);
  const shuffleRef = useRef(shuffle);
  const mediaKindRef = useRef(mediaKind);
  const radioStationRef = useRef(radioStation);
  const radioErrorRef = useRef<string | null>(radioError);
  const radioRetryRef = useRef(0);
  const radioTimerRef = useRef<number | null>(null);
  const lastProgressRef = useRef(0); // performance.now() of last timeupdate
  queueRef.current = queue;
  indexRef.current = index;
  currentTrackRef.current = currentTrack;
  repeatRef.current = repeat;
  shuffleRef.current = shuffle;
  mediaKindRef.current = mediaKind;
  radioStationRef.current = radioStation;
  radioErrorRef.current = radioError;

  const current = currentTrack;

  const playAt = useCallback((i: number) => {
    const q = queueRef.current;
    if (i < 0 || i >= q.length) return;
    mediaKindRef.current = "music";
    setMediaKind("music");
    engine.ensureGraph();
    engine.audio.src = trackUrl(q[i].path);
    engine.audio.play().catch((e) => logPlaybackError("play() rejected", e));
    setIndex(i);
    setCurrentTrack(q[i]);
  }, []);

  /**
   * Replace the queue with `list` and start playing item `i`. Used when the
   * user plays a song from a specific view (a folder, album, or artist) so the
   * queue — and therefore next/prev — follows that list. We write the ref
   * synchronously because playAt reads queueRef before the state commit lands.
   */
  const playInList = useCallback((list: Track[], i: number) => {
    if (i < 0 || i >= list.length) return;
    mediaKindRef.current = "music";
    setMediaKind("music");
    queueRef.current = list;
    setQueue(list);
    engine.ensureGraph();
    engine.audio.src = trackUrl(list[i].path);
    engine.audio.play().catch((e) => logPlaybackError("play() rejected", e));
    setIndex(i);
    setCurrentTrack(list[i]);
  }, []);

  /**
   * Point next/prev at `list` (the list currently on screen) WITHOUT disturbing
   * what's playing. Re-anchors the index on the playing track's position in the
   * new list; if it isn't there (e.g. a search that excludes it), index becomes
   * -1 so the next track is the first item of the new list. This mirrors
   * Dopamine, where applying/clearing a search retargets the play queue.
   */
  const syncQueue = useCallback((list: Track[]) => {
    queueRef.current = list;
    setQueue(list);
    const path = currentTrackRef.current?.path;
    const i = path ? list.findIndex((t) => t.path === path) : -1;
    indexRef.current = i;
    setIndex(i);
  }, []);

  /**
   * Load `list[i]` into the engine WITHOUT playing (paused), seeking to
   * `position` once metadata is ready. Used to restore the last session;
   * playback begins only when the user presses play.
   */
  const loadInList = useCallback((list: Track[], i: number, position: number) => {
    if (i < 0 || i >= list.length) return;
    mediaKindRef.current = "music";
    setMediaKind("music");
    queueRef.current = list;
    setQueue(list);
    const a = engine.audio;
    a.src = trackUrl(list[i].path);
    if (position > 0) {
      const seek = () => {
        a.currentTime = position;
        a.removeEventListener("loadedmetadata", seek);
      };
      a.addEventListener("loadedmetadata", seek);
    }
    setIndex(i);
    setCurrentTrack(list[i]);
    setPlaying(false);
  }, []);

  const next = useCallback(() => {
    const q = queueRef.current;
    if (!q.length) return;
    if (shuffleRef.current && q.length > 1) {
      let r = indexRef.current;
      while (r === indexRef.current) r = Math.floor(Math.random() * q.length);
      playAt(r);
      return;
    }
    const i = indexRef.current;
    if (i + 1 < q.length) playAt(i + 1);
    else if (repeatRef.current === "all") playAt(0);
    else {
      engine.audio.pause();
      setPlaying(false);
    }
  }, [playAt]);

  const prev = useCallback(() => {
    if (mediaKindRef.current !== "music") {
      playAt(indexRef.current >= 0 ? indexRef.current : 0);
      return;
    }
    if (engine.audio.currentTime > 3 || indexRef.current <= 0) {
      engine.audio.currentTime = 0;
      return;
    }
    playAt(indexRef.current - 1);
  }, [playAt]);

  const toggle = useCallback(() => {
    // If the engine currently holds a radio stream (user stopped radio and came
    // back to the music player), a music play must (re)load the music track —
    // otherwise engine.audio.play() would just resume the radio source.
    if (mediaKindRef.current !== "music") {
      playAt(indexRef.current >= 0 ? indexRef.current : 0);
      return;
    }
    if (indexRef.current < 0) {
      playAt(0);
      return;
    }
    if (engine.audio.paused) {
      engine.ensureGraph();
      engine.audio.play().catch(() => {});
    } else {
      engine.audio.pause();
    }
  }, [playAt]);

  const seek = useCallback((t: number) => {
    engine.audio.currentTime = t;
    setCurrentTime(t);
  }, []);

  const cycleRepeat = useCallback(() => {
    setRepeat((r) => (r === "off" ? "all" : r === "all" ? "one" : "off"));
  }, []);

  // ---- Radio --------------------------------------------------------------
  // Resilient streaming: exponential-backoff reconnect for transient failures
  // (network drop, server blip, silent stall), a hard stop for permanent ones
  // (bad URL / auth), and immediate retry when the network returns.

  const setStatus = useCallback((s: RadioStatus) => setRadioStatus(s), []);

  const cancelRadioRetry = useCallback(() => {
    if (radioTimerRef.current != null) {
      window.clearTimeout(radioTimerRef.current);
      radioTimerRef.current = null;
    }
  }, []);

  /** Point the engine at a station via the proxy. `retry` keeps the backoff
   *  counter (so consecutive failures slow down). */
  const tuneStation = useCallback(
    async (s: Station, retry: boolean) => {
      cancelRadioRetry();
      mediaKindRef.current = "radio";
      radioStationRef.current = s;
      setMediaKind("radio");
      setRadioStation(s);
      if (!retry) {
        radioRetryRef.current = 0;
        radioErrorRef.current = null;
        setRadioMeta(EMPTY_META);
        setRadioError(null);
      }
      setStatus(retry ? "reconnecting" : "connecting");
      lastProgressRef.current = performance.now();
      engine.ensureGraph();
      try {
        const url = await radioProxyUrl(s.url);
        // The user may have switched stations while we awaited the port.
        if (radioStationRef.current?.id !== s.id) return;
        // Cache-bust on retry so the element re-requests (proxy ignores extras).
        engine.audio.src = retry ? `${url}&_r=${radioRetryRef.current}` : url;
        await engine.audio.play();
      } catch (e) {
        logPlaybackError("radio play() rejected", e);
      }
    },
    [cancelRadioRetry, setStatus]
  );

  const tuneStationRef = useRef(tuneStation);
  tuneStationRef.current = tuneStation;

  /** Schedule a backoff reconnect (transient failures only — never gives up,
   *  since a dropped network can come back). */
  const scheduleRadioReconnect = useCallback(() => {
    const s = radioStationRef.current;
    if (!s || radioTimerRef.current != null) return; // one already in flight
    if (radioErrorRef.current) return; // permanent failure — don't retry
    const delay = RADIO_BACKOFF[Math.min(radioRetryRef.current, RADIO_BACKOFF.length - 1)];
    radioRetryRef.current += 1;
    setStatus("reconnecting");
    radioTimerRef.current = window.setTimeout(() => {
      radioTimerRef.current = null;
      if (mediaKindRef.current === "radio" && radioStationRef.current?.id === s.id) {
        void tuneStationRef.current(s, true);
      }
    }, delay * 1000);
  }, [setStatus]);

  const playStation = useCallback((s: Station) => tuneStation(s, false), [tuneStation]);

  /** Stop radio + cancel any pending reconnect (when pausing or leaving radio). */
  const stopRadio = useCallback(() => {
    cancelRadioRetry();
    engine.audio.pause();
    setStatus("idle");
  }, [cancelRadioRetry, setStatus]);

  const radioToggle = useCallback(() => {
    const s = radioStationRef.current;
    if (!s) return;
    if (engine.audio.paused) {
      // A manual press after a permanent error is a fresh attempt.
      if (radioErrorRef.current) {
        void tuneStationRef.current(s, false);
        return;
      }
      engine.ensureGraph();
      engine.audio.play().catch((e) => logPlaybackError("radio play() rejected", e));
    } else {
      cancelRadioRetry();
      engine.audio.pause();
    }
  }, [cancelRadioRetry]);

  /** Merge a `radio:meta` event, ignoring events for a station we've left. */
  const applyRadioMeta = useCallback((m: RadioMetaEvent) => {
    if (radioStationRef.current?.url !== m.url) return;
    radioErrorRef.current = null; // a successful response clears any prior error
    setRadioError(null);
    setRadioMeta((prev) => ({
      title: m.title ?? prev.title,
      codec: m.codec ?? prev.codec,
      bitrate: m.bitrate ?? prev.bitrate,
      name: m.name ?? prev.name,
    }));
  }, []);

  /** Apply a `radio:error` event. Permanent → stop; transient → keep retrying. */
  const applyRadioError = useCallback(
    (e: RadioErrorEvent) => {
      if (radioStationRef.current?.url !== e.url) return;
      if (e.permanent) {
        cancelRadioRetry();
        radioRetryRef.current = 0;
        radioErrorRef.current = e.message;
        setRadioError(e.message);
        setStatus("error");
      } else {
        scheduleRadioReconnect();
      }
    },
    [cancelRadioRetry, scheduleRadioReconnect, setStatus]
  );

  // Wire <audio> events exactly once.
  useEffect(() => {
    const a = engine.audio;
    const onTime = () => {
      lastProgressRef.current = performance.now(); // feeds the stall watchdog
      setCurrentTime(a.currentTime);
    };
    const onMeta = () => setDuration(a.duration || 0);
    const onPlay = () => {
      setPlaying(true);
      if (mediaKindRef.current === "radio") {
        radioRetryRef.current = 0;
        radioErrorRef.current = null;
        setRadioError(null);
        setRadioStatus("playing");
      }
    };
    const onPlaying = () => {
      if (mediaKindRef.current === "radio") setRadioStatus("playing");
    };
    // Buffering (data not arriving fast enough) — informational, not a failure.
    const onWaiting = () => {
      if (mediaKindRef.current === "radio" && !radioErrorRef.current)
        setRadioStatus("buffering");
    };
    const onPause = () => setPlaying(false);
    const onEnd = () => {
      // A live radio stream shouldn't "end" — treat it as a drop and reconnect.
      if (mediaKindRef.current === "radio") {
        scheduleRadioReconnect();
        return;
      }
      if (repeatRef.current === "one") {
        a.currentTime = 0;
        a.play().catch(() => {});
        return;
      }
      next();
    };
    // Surface decode/load failures (otherwise swallowed): some FLACs play in
    // ffmpeg-based players but Chromium/WebView2's stricter decoder rejects them.
    const onError = () => {
      logPlaybackError("error event");
      // For radio, the proxy's radio:error (with permanent flag) decides whether
      // to retry; only fall back to a reconnect here if no permanent error stuck.
      if (mediaKindRef.current === "radio" && !radioErrorRef.current)
        scheduleRadioReconnect();
    };
    a.addEventListener("timeupdate", onTime);
    a.addEventListener("loadedmetadata", onMeta);
    a.addEventListener("play", onPlay);
    a.addEventListener("playing", onPlaying);
    a.addEventListener("waiting", onWaiting);
    a.addEventListener("pause", onPause);
    a.addEventListener("ended", onEnd);
    a.addEventListener("error", onError);
    return () => {
      a.removeEventListener("timeupdate", onTime);
      a.removeEventListener("loadedmetadata", onMeta);
      a.removeEventListener("play", onPlay);
      a.removeEventListener("playing", onPlaying);
      a.removeEventListener("waiting", onWaiting);
      a.removeEventListener("pause", onPause);
      a.removeEventListener("ended", onEnd);
      a.removeEventListener("error", onError);
    };
  }, [next, scheduleRadioReconnect]);

  // Stall watchdog: radio "playing" but no progress for RADIO_STALL_MS → the
  // stream went silent without firing an error; force a backoff reconnect.
  useEffect(() => {
    const id = window.setInterval(() => {
      if (mediaKindRef.current !== "radio") return;
      if (radioErrorRef.current || engine.audio.paused) return;
      if (performance.now() - lastProgressRef.current > RADIO_STALL_MS) {
        scheduleRadioReconnect();
      }
    }, 4000);
    return () => window.clearInterval(id);
  }, [scheduleRadioReconnect]);

  // Network awareness: pause-ish status when offline, immediate retry on return.
  useEffect(() => {
    const onOffline = () => {
      if (mediaKindRef.current === "radio" && !engine.audio.paused && !radioErrorRef.current)
        setRadioStatus("reconnecting");
    };
    const onOnline = () => {
      const s = radioStationRef.current;
      if (mediaKindRef.current !== "radio" || !s || radioErrorRef.current) return;
      if (engine.audio.paused) return; // user paused — leave it
      radioRetryRef.current = 0; // network's back — retry now, fast
      cancelRadioRetry();
      void tuneStationRef.current(s, true);
    };
    window.addEventListener("offline", onOffline);
    window.addEventListener("online", onOnline);
    return () => {
      window.removeEventListener("offline", onOffline);
      window.removeEventListener("online", onOnline);
    };
  }, [cancelRadioRetry]);

  useEffect(() => {
    engine.audio.volume = volume;
  }, [volume]);

  return {
    queue,
    setQueue,
    index,
    current,
    isPlaying,
    currentTime,
    duration,
    volume,
    repeat,
    shuffle,
    playAt,
    playInList,
    syncQueue,
    loadInList,
    next,
    prev,
    toggle,
    seek,
    setVolume,
    cycleRepeat,
    toggleShuffle: () => setShuffle((s) => !s),
    // Radio
    mediaKind,
    radioStation,
    radioPlaying: isPlaying && mediaKind === "radio",
    radioMeta,
    radioError,
    radioStatus,
    playStation,
    radioToggle,
    stopRadio,
    applyRadioMeta,
    applyRadioError,
  };
}
