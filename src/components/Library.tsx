import { useEffect, useRef } from "react";
import type { Track } from "../types";
import { fmtTime } from "../lib/format";

/**
 * Scrollable song list. `tracks` is already the resolved view (filtered when
 * searching), so a row's position IS its index in the play queue. Highlights
 * the currently playing track by path (robust across view changes).
 */
export function Library({
  tracks,
  currentPath,
  isPlaying,
  onPlay,
  emptyMessage,
  followSong,
}: {
  tracks: Track[];
  currentPath: string | undefined;
  isPlaying: boolean;
  onPlay: (index: number) => void;
  emptyMessage?: string;
  followSong: boolean;
}) {
  const activeRef = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    if (followSong && activeRef.current) {
      activeRef.current.scrollIntoView({ block: "nearest", behavior: "smooth" });
    }
  }, [currentPath, followSong]);

  if (!tracks.length) {
    return (
      <div className="flex h-full min-h-[200px] flex-col items-center justify-center gap-3 p-6 text-center text-white/40">
        <div className="text-4xl">🎵</div>
        <p className="text-sm">{emptyMessage ?? "Tidak ada lagu di sini."}</p>
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-0.5 p-2">
      {tracks.map((t, pos) => {
        const active = t.path === currentPath;
        return (
          <button
            key={t.path + pos}
            ref={active ? activeRef : undefined}
            onClick={() => onPlay(pos)}
            className={`group flex items-center gap-3 rounded-xl px-3 py-2 text-left transition ${
              active ? "bg-white/15" : "hover:bg-white/8"
            }`}
          >
            <div className="flex w-6 shrink-0 justify-center text-xs text-white/40">
              {active && isPlaying ? (
                <span className="text-white">▶</span>
              ) : (
                <span className="tabular-nums">{pos + 1}</span>
              )}
            </div>
            <div className="min-w-0 flex-1">
              <div
                className={`truncate text-sm ${active ? "font-semibold text-white" : "text-white/90"}`}
              >
                {t.title}
              </div>
              <div className="truncate text-xs text-white/45">
                {t.artist} <span className="text-white/30">|</span> {t.album}
              </div>
            </div>
            <div className="shrink-0 text-xs tabular-nums text-white/40">
              {fmtTime(t.duration)}
            </div>
          </button>
        );
      })}
    </div>
  );
}
