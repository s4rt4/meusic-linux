import { useEffect, useState } from "react";
import { emit, listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  MP_STATE,
  MP_CMD,
  MP_REQUEST,
  type MiniState,
  type MiniCmd,
} from "../lib/miniState";
import { fmtTime } from "../lib/format";
import { rgb } from "../lib/colors";
import { Play, Pause, Prev, Next, VolumeHigh, VolumeLow, VolumeMute } from "./icons";

const cmd = (c: MiniCmd) => {
  void emit(MP_CMD, c);
};

/** Compact now-playing popup shown from the system tray (its own window). */
export function MiniPlayer() {
  const [s, setS] = useState<MiniState | null>(null);

  useEffect(() => {
    const win = getCurrentWindow();
    let unState: (() => void) | undefined;
    let unFocus: (() => void) | undefined;
    listen<MiniState>(MP_STATE, (e) => setS(e.payload)).then((u) => (unState = u));
    win.onFocusChanged(({ payload: focused }) => {
      if (!focused) void win.hide();
    }).then((u) => (unFocus = u));
    void emit(MP_REQUEST); // ask main for current state on open
    return () => {
      unState?.();
      unFocus?.();
    };
  }, []);

  const accent = s?.accent ?? [108, 99, 196];
  const accentCss = `rgb(${accent.join(",")})`;
  const pct = s && s.duration > 0 ? (s.position / s.duration) * 100 : 0;
  const fill = (p: number) =>
    `linear-gradient(to right, ${rgb(accent, 0.95)} ${p}%, rgba(255,255,255,0.18) ${p}%)`;
  const vol = s?.volume ?? 1;

  return (
    <div className="flex h-screen w-screen flex-col gap-3 bg-[#0d0d12] p-4 text-white">
      {/* cover + info */}
      <div className="flex items-center gap-3">
        <div className="h-16 w-16 shrink-0 overflow-hidden rounded-lg bg-white/10">
          {s?.coverUrl ? (
            <img src={s.coverUrl} alt="" className="h-full w-full object-cover" />
          ) : (
            <div className="flex h-full w-full items-center justify-center text-2xl text-white/30">
              ♪
            </div>
          )}
        </div>
        <div className="min-w-0">
          <div className="truncate text-sm font-semibold">{s?.title ?? "meusic"}</div>
          <div className="truncate text-xs text-white/55">
            {s?.artist || (s?.hasTrack ? "" : "Belum ada lagu")}
          </div>
        </div>
      </div>

      {/* volume */}
      <div className="flex items-center gap-2">
        <span className="text-white/60">
          {vol === 0 ? (
            <VolumeMute className="h-4 w-4" />
          ) : vol < 0.5 ? (
            <VolumeLow className="h-4 w-4" />
          ) : (
            <VolumeHigh className="h-4 w-4" />
          )}
        </span>
        <input
          type="range"
          min={0}
          max={1}
          step={0.01}
          value={vol}
          onChange={(e) => cmd({ action: "volume", value: Number(e.target.value) })}
          className="h-1 flex-1 rounded-full"
          style={{ background: fill(vol * 100) }}
        />
        <span className="w-8 text-right text-[11px] tabular-nums text-white/55">
          {Math.round(vol * 100)}
        </span>
      </div>

      {/* seek */}
      <div className="flex items-center gap-2">
        <span className="w-9 text-right text-[11px] tabular-nums text-white/55">
          {fmtTime(s?.position ?? 0)}
        </span>
        <input
          type="range"
          min={0}
          max={s?.duration || 0}
          step={0.1}
          value={Math.min(s?.position ?? 0, s?.duration || 0)}
          onChange={(e) => cmd({ action: "seek", value: Number(e.target.value) })}
          disabled={!s?.hasTrack}
          className="h-1 flex-1 rounded-full"
          style={{ background: fill(pct) }}
        />
        <span className="w-9 text-[11px] tabular-nums text-white/55">
          {fmtTime(s?.duration ?? 0)}
        </span>
      </div>

      {/* transport */}
      <div className="flex items-center justify-center gap-4">
        <button
          onClick={() => cmd({ action: "prev" })}
          disabled={!s?.hasTrack}
          className="rounded-full p-2 text-white/70 transition hover:bg-white/10 hover:text-white disabled:opacity-30"
        >
          <Prev className="h-6 w-6" />
        </button>
        <button
          onClick={() => cmd({ action: "toggle" })}
          disabled={!s?.hasTrack}
          className="flex h-12 w-12 items-center justify-center rounded-full text-white shadow-lg transition active:scale-95 disabled:opacity-40"
          style={{ background: accentCss }}
        >
          {s?.isPlaying ? (
            <Pause className="h-6 w-6" />
          ) : (
            <Play className="h-6 w-6 translate-x-[1px]" />
          )}
        </button>
        <button
          onClick={() => cmd({ action: "next" })}
          disabled={!s?.hasTrack}
          className="rounded-full p-2 text-white/70 transition hover:bg-white/10 hover:text-white disabled:opacity-30"
        >
          <Next className="h-6 w-6" />
        </button>
      </div>

      {/* show main */}
      <button
        onClick={() => {
          cmd({ action: "show-main" });
          void getCurrentWindow().hide();
        }}
        className="mt-auto self-end text-xs font-medium transition hover:underline"
        style={{ color: accentCss }}
      >
        Tampilkan meusic
      </button>
    </div>
  );
}
