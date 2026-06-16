import { useCallback, useEffect, useRef, useState } from "react";
import { loadStore, saveStore } from "../lib/api";
import type { Station } from "../types";
import seed from "../assets/radio-stations.json";

const KEY = "meusic.stations"; // fast localStorage cache
const STORE = "stations"; // durable file-backed store (stations.json)

function newId(): string {
  try {
    return crypto.randomUUID();
  } catch {
    // Fallback for environments without crypto.randomUUID.
    return `st-${Date.now().toString(36)}-${Math.floor(Math.random() * 1e9).toString(36)}`;
  }
}

/** Bundled stations are the first-run seed; ids are assigned on import. */
function seedStations(): Station[] {
  const list = (seed as { stations: { name: string; url: string }[] }).stations;
  return list.map((s) => ({ id: newId(), name: s.name, url: s.url }));
}

function parse(raw: string | null): Station[] | null {
  if (!raw) return null;
  try {
    const arr = JSON.parse(raw);
    if (!Array.isArray(arr)) return null;
    return arr.filter(
      (s): s is Station =>
        s && typeof s.id === "string" && typeof s.name === "string" && typeof s.url === "string"
    );
  } catch {
    return null;
  }
}

function loadCache(): Station[] {
  try {
    return parse(localStorage.getItem(KEY)) ?? seedStations();
  } catch {
    return seedStations();
  }
}

/**
 * User-editable radio station list. The file store is the source of truth
 * (localStorage isn't flushed reliably on OS shutdown — same reasoning as
 * settings/session); localStorage is a fast startup cache. On first run the
 * bundled JSON seeds the store.
 */
export function useStations() {
  const [stations, setStations] = useState<Station[]>(loadCache);
  const ref = useRef(stations);
  ref.current = stations;

  const persist = useCallback((next: Station[]) => {
    const json = JSON.stringify(next);
    try {
      localStorage.setItem(KEY, json);
    } catch {
      /* ignore */
    }
    void saveStore(STORE, json).catch(() => {});
  }, []);

  // Reconcile from disk once on startup; seed the store if there's no file yet.
  useEffect(() => {
    loadStore(STORE)
      .then((raw) => {
        const fromFile = parse(raw);
        if (fromFile) {
          setStations(fromFile);
          try {
            localStorage.setItem(KEY, JSON.stringify(fromFile));
          } catch {
            /* ignore */
          }
        } else {
          persist(ref.current); // first run / migrating — seed it
        }
      })
      .catch(() => {});
  }, [persist]);

  const add = useCallback(
    (name: string, url: string): Station => {
      const station: Station = { id: newId(), name: name.trim(), url: url.trim() };
      setStations((prev) => {
        const next = [...prev, station];
        persist(next);
        return next;
      });
      return station;
    },
    [persist]
  );

  const update = useCallback(
    (id: string, name: string, url: string) => {
      setStations((prev) => {
        const next = prev.map((s) =>
          s.id === id ? { ...s, name: name.trim(), url: url.trim() } : s
        );
        persist(next);
        return next;
      });
    },
    [persist]
  );

  const remove = useCallback(
    (id: string) => {
      setStations((prev) => {
        const next = prev.filter((s) => s.id !== id);
        persist(next);
        return next;
      });
    },
    [persist]
  );

  return { stations, add, update, remove };
}
