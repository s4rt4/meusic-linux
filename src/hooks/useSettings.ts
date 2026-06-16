import { useCallback, useEffect, useRef, useState } from "react";
import { loadStore, saveStore } from "../lib/api";

/** User-configurable settings, persisted to localStorage. */
export interface Settings {
  rememberLastPlayed: boolean; // restore last track + position on startup
  resumeStartupPage: boolean; // restore last mode + folder on startup
  followSong: boolean; // auto-scroll the list to the playing track
  volumeScrollStep: number; // percent the mouse wheel changes volume by
  // System tray (used in a later phase)
  trayIcon: boolean;
  minimizeToTray: boolean;
  closeToTray: boolean;
}

const DEFAULTS: Settings = {
  rememberLastPlayed: true,
  resumeStartupPage: true,
  followSong: true,
  volumeScrollStep: 2,
  trayIcon: true,
  minimizeToTray: true,
  closeToTray: true,
};

const KEY = "meusic.settings";
const STORE = "settings"; // file-backed store name (settings.json)

function parse(raw: string | null): Settings | null {
  if (!raw) return null;
  try {
    return { ...DEFAULTS, ...JSON.parse(raw) };
  } catch {
    return null;
  }
}

/** Synchronous initial read from the localStorage cache (avoids a flash of
 *  defaults before the file store loads). */
function loadCache(): Settings {
  try {
    return parse(localStorage.getItem(KEY)) ?? DEFAULTS;
  } catch {
    return DEFAULTS;
  }
}

export function useSettings() {
  const [settings, setSettings] = useState<Settings>(loadCache);
  const settingsRef = useRef(settings);
  settingsRef.current = settings;

  // The file store is the source of truth (localStorage doesn't survive an OS
  // shutdown reliably). Reconcile from disk once on startup.
  useEffect(() => {
    loadStore(STORE)
      .then((raw) => {
        const fromFile = parse(raw);
        if (fromFile) {
          setSettings(fromFile);
          try {
            localStorage.setItem(KEY, JSON.stringify(fromFile));
          } catch {
            /* ignore */
          }
        } else {
          // No file yet (first run / migrating from localStorage) — seed it.
          void saveStore(STORE, JSON.stringify(settingsRef.current)).catch(
            () => {}
          );
        }
      })
      .catch(() => {});
  }, []);

  const update = useCallback(
    <K extends keyof Settings>(key: K, value: Settings[K]) => {
      setSettings((prev) => {
        const next = { ...prev, [key]: value };
        const json = JSON.stringify(next);
        try {
          localStorage.setItem(KEY, json); // fast cache
        } catch {
          /* ignore */
        }
        void saveStore(STORE, json).catch(() => {}); // durable on disk
        return next;
      });
    },
    []
  );

  return { settings, update };
}
