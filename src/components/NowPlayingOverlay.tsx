import type { RGB, Track } from "../types";
import { rgb } from "../lib/colors";
import { Visualizer } from "./Visualizer";

/**
 * Full-screen "Now Playing" view (the Amberol moment): large cover art, track
 * info, and the spectrum visualizer, over the adaptive gradient background.
 */
export function NowPlayingOverlay({
  open,
  onClose,
  track,
  coverUrl,
  accent,
  active,
}: {
  open: boolean;
  onClose: () => void;
  track: Track | null;
  coverUrl: string | null;
  accent: RGB;
  active: boolean;
}) {
  if (!open) return null;
  return (
    <div className="fixed inset-0 z-30 flex flex-col items-center justify-center gap-7 bg-black/25 p-8 backdrop-blur-2xl">
      <button
        onClick={onClose}
        className="absolute right-6 top-6 flex h-9 w-9 items-center justify-center rounded-full bg-white/10 text-lg text-white/80 transition hover:bg-white/20"
        title="Tutup"
      >
        ✕
      </button>

      <div
        className="aspect-square w-[min(46vh,360px)] overflow-hidden rounded-3xl shadow-2xl"
        style={{ boxShadow: `0 30px 80px -24px ${rgb(accent, 0.8)}` }}
      >
        {coverUrl ? (
          <img src={coverUrl} alt="" className="h-full w-full object-cover" />
        ) : (
          <div
            className="flex h-full w-full items-center justify-center text-7xl text-white/30"
            style={{ background: rgb(accent, 0.25) }}
          >
            ♪
          </div>
        )}
      </div>

      <div className="max-w-[80vw] text-center">
        <h2 className="truncate text-3xl font-bold text-white">
          {track?.title ?? "—"}
        </h2>
        <p className="mt-1 truncate text-lg text-white/75">{track?.artist}</p>
        {track?.album && (
          <p className="mt-0.5 truncate text-sm text-white/45">{track.album}</p>
        )}
      </div>

      <div className="h-24 w-[min(80vw,560px)]">
        <Visualizer color={accent} active={active} />
      </div>
    </div>
  );
}
