import type { ReactNode } from "react";
import type { AppMode, RadioStatus, RepeatMode, RGB, Station, Track } from "../types";
import { fmtTime } from "../lib/format";
import { rgb } from "../lib/colors";
import { stationColor, stationInitials } from "../lib/stationColor";
import { Equalizer } from "./Equalizer";
import {
  Play,
  Pause,
  Next,
  Prev,
  Shuffle,
  Repeat,
  RepeatOne,
  VolumeHigh,
  VolumeLow,
  VolumeMute,
  SlidersVertical,
  AudioLines,
  Leaf,
  Radio,
} from "./icons";

export interface RadioBarState {
  station: Station | null;
  playing: boolean;
  title: string | null;
  codec: string | null;
  bitrate: number | null;
  error: string | null;
  status: RadioStatus;
  onToggle: () => void;
}

/**
 * Dopamine-style bottom now-playing bar:
 *   [cover + title/artist]   [repeat prev PLAY next shuffle]   [time · eq · volume]
 * with a full-width seek line along the top edge. Clicking the cover/info opens
 * the full Now Playing view.
 */
export function BottomBar({
  accent,
  track,
  coverUrl,
  isPlaying,
  currentTime,
  duration,
  volume,
  repeat,
  shuffle,
  hasTrack,
  showEq,
  eqGains,
  onSeek,
  onToggle,
  onNext,
  onPrev,
  onVolume,
  onCycleRepeat,
  onToggleShuffle,
  onToggleEq,
  onEqChange,
  onEqPreset,
  onExpand,
  powerSave,
  onTogglePowerSave,
  volumeStep,
  appMode,
  radio,
}: {
  accent: RGB;
  track: Track | null;
  coverUrl: string | null;
  isPlaying: boolean;
  currentTime: number;
  duration: number;
  volume: number;
  repeat: RepeatMode;
  shuffle: boolean;
  hasTrack: boolean;
  showEq: boolean;
  eqGains: number[];
  onSeek: (t: number) => void;
  onToggle: () => void;
  onNext: () => void;
  onPrev: () => void;
  onVolume: (v: number) => void;
  onCycleRepeat: () => void;
  onToggleShuffle: () => void;
  onToggleEq: () => void;
  onEqChange: (index: number, gainDb: number) => void;
  onEqPreset: (gains: number[]) => void;
  onExpand: () => void;
  powerSave: boolean;
  onTogglePowerSave: () => void;
  volumeStep: number;
  appMode: AppMode;
  radio: RadioBarState;
}) {
  const isRadio = appMode === "radio";
  const pct = duration > 0 ? (currentTime / duration) * 100 : 0;
  const fill = (p: number) =>
    `linear-gradient(to right, ${rgb(accent, 0.95)} ${p}%, rgba(255,255,255,0.16) ${p}%)`;

  const st = radio.station;
  const stColor = st ? stationColor(st.name) : accent;
  const radioQuality = [radio.codec, (radio.bitrate ?? 0) > 0 ? `${radio.bitrate} kbps` : null]
    .filter(Boolean)
    .join(" · ");
  const radioBusy =
    radio.status === "connecting" ||
    radio.status === "reconnecting" ||
    radio.status === "buffering";
  const radioSubtitle =
    radio.error ??
    (radioBusy
      ? radio.status === "reconnecting"
        ? "Menyambung ulang…"
        : radio.status === "buffering"
          ? "Menyangga…"
          : "Menyambungkan…"
      : radio.title ?? (radio.playing ? "Siaran langsung" : st ? "Belum diputar" : "Pilih stasiun"));

  return (
    <footer className="glass relative z-20 shrink-0 rounded-t-2xl">
      {/* Seek line along the very top edge (music only — radio is live) */}
      {!isRadio && (
        <input
          type="range"
          min={0}
          max={duration || 0}
          step={0.1}
          value={Math.min(currentTime, duration || 0)}
          onChange={(e) => onSeek(Number(e.target.value))}
          disabled={!hasTrack}
          className="absolute left-0 right-0 top-0 h-1.5 w-full"
          style={{ background: fill(pct) }}
        />
      )}

      {/* 3-column grid: flexible sides (shrink, never overflow) keep the play
          button perfectly centered regardless of window width. */}
      <div className="grid h-[84px] grid-cols-[1fr_auto_1fr] items-center gap-4 pl-5 pr-5 lg:pl-9 lg:pr-7">
        {/* Left: now-playing identity (music track or radio station). */}
        {isRadio ? (
          <div className="flex min-w-0 items-center gap-3">
            <div
              className="flex h-14 w-14 shrink-0 items-center justify-center rounded-md text-sm font-black text-white/90"
              style={{
                background: st
                  ? `linear-gradient(135deg, rgb(${stColor.join(",")}), rgba(${stColor.join(",")},0.5))`
                  : "rgba(255,255,255,0.1)",
                boxShadow: `0 6px 18px -6px ${rgb(stColor, 0.8)}`,
              }}
            >
              {st ? stationInitials(st.name) : <Radio className="h-6 w-6 text-white/40" />}
            </div>
            <div className="min-w-0">
              <div className="truncate text-sm font-semibold text-white">
                {st?.name ?? "Radio"}
              </div>
              <div
                className={`truncate text-xs ${
                  radio.error ? "text-red-300" : radioBusy ? "text-amber-300" : "text-white/55"
                }`}
              >
                {radioSubtitle}
              </div>
              {radioQuality && (
                <div className="mt-0.5 text-[10px] font-medium uppercase tracking-wide text-white/40">
                  {radioQuality}
                </div>
              )}
            </div>
          </div>
        ) : (
          <div className="flex min-w-0 items-center gap-3">
            <button
              onClick={onExpand}
              disabled={!hasTrack || powerSave}
              className="group relative h-14 w-14 shrink-0 overflow-hidden rounded-md bg-white/10 transition hover:opacity-90 disabled:cursor-default"
              style={{ boxShadow: `0 6px 18px -6px ${rgb(accent, 0.8)}` }}
              title="Buka tampilan Now Playing"
            >
              {coverUrl ? (
                <img src={coverUrl} alt="" className="h-full w-full object-cover" />
              ) : (
                <div className="flex h-full w-full items-center justify-center text-2xl text-white/30">
                  ♪
                </div>
              )}
              {/* Hover affordance: opens the full Now Playing view (off in power-save) */}
              {hasTrack && !powerSave && (
                <div className="absolute inset-0 flex items-center justify-center bg-black/55 text-white opacity-0 transition-opacity duration-150 group-hover:opacity-100">
                  <AudioLines className="h-6 w-6" />
                </div>
              )}
            </button>
            <div className="min-w-0">
              <div className="truncate text-sm font-semibold text-white">
                {track?.title ?? "meusic"}
              </div>
              <div className="truncate text-xs text-white/55">
                {track?.artist ?? "Belum ada lagu"}
              </div>
              {track && (track.format || track.bitrate > 0) && (
                <div className="mt-0.5 flex items-center gap-1.5 text-[10px] font-medium uppercase tracking-wide text-white/40">
                  {track.format && (
                    <span
                      className="rounded px-1.5 py-px"
                      style={{
                        background: rgb(accent, 0.25),
                        color: rgb(accent, 1),
                      }}
                    >
                      {track.format}
                    </span>
                  )}
                  {track.bitrate > 0 && <span>{track.bitrate} kbps</span>}
                </div>
              )}
            </div>
          </div>
        )}

        {/* Center: transport. Radio is live — only play/pause (no seek/repeat/
            shuffle/skip); music keeps the full transport. */}
        {isRadio ? (
          <div className="flex items-center justify-center">
            <button
              onClick={radio.onToggle}
              disabled={!st}
              className="flex h-12 w-12 items-center justify-center rounded-full text-white shadow-lg transition active:scale-95 disabled:opacity-40"
              style={{ background: rgb(accent, 1) }}
              title="Play/Pause"
            >
              {radio.playing ? (
                <Pause className="h-6 w-6" />
              ) : (
                <Play className="h-6 w-6 translate-x-[1px]" />
              )}
            </button>
          </div>
        ) : (
          <div className="flex items-center justify-center gap-2">
            <IconBtn active={repeat !== "off"} onClick={onCycleRepeat} title="Repeat">
              {repeat === "one" ? (
                <RepeatOne className="h-5 w-5" />
              ) : (
                <Repeat className="h-5 w-5" />
              )}
            </IconBtn>
            <IconBtn onClick={onPrev} disabled={!hasTrack} title="Previous">
              <Prev className="h-6 w-6" />
            </IconBtn>
            <button
              onClick={onToggle}
              className="flex h-12 w-12 items-center justify-center rounded-full text-white shadow-lg transition active:scale-95"
              style={{ background: rgb(accent, 1) }}
              title="Play/Pause"
            >
              {isPlaying ? (
                <Pause className="h-6 w-6" />
              ) : (
                <Play className="h-6 w-6 translate-x-[1px]" />
              )}
            </button>
            <IconBtn onClick={onNext} disabled={!hasTrack} title="Next">
              <Next className="h-6 w-6" />
            </IconBtn>
            <IconBtn active={shuffle} onClick={onToggleShuffle} title="Shuffle">
              <Shuffle className="h-5 w-5" />
            </IconBtn>
          </div>
        )}

        {/* Right: time + eq + volume */}
        <div className="flex min-w-0 items-center justify-end gap-3">
          {!isRadio && (
            <span className="hidden shrink-0 text-xs tabular-nums text-white/55 sm:inline">
              {fmtTime(currentTime)} / {fmtTime(duration)}
            </span>
          )}

          <button
            onClick={onTogglePowerSave}
            title={powerSave ? "Hemat daya: aktif" : "Hemat daya: nonaktif"}
            className={`shrink-0 rounded-full p-2 transition hover:bg-white/10 ${
              powerSave ? "text-[#44aa00]" : "text-white/60"
            }`}
          >
            <Leaf className="h-5 w-5" />
          </button>

          <div className="relative shrink-0">
            <IconBtn active={showEq} onClick={onToggleEq} title="Equalizer">
              <SlidersVertical className="h-5 w-5" />
            </IconBtn>
            {showEq && (
              <div className="glass-strong absolute bottom-12 right-0 w-72 rounded-2xl p-4 shadow-2xl">
                <Equalizer
                  gains={eqGains}
                  onChange={onEqChange}
                  onPreset={onEqPreset}
                />
              </div>
            )}
          </div>

          <div
            className="flex shrink-0 items-center gap-2"
            onWheel={(e) => {
              const d = (e.deltaY < 0 ? 1 : -1) * (volumeStep / 100);
              onVolume(Math.min(1, Math.max(0, volume + d)));
            }}
            title={`Scroll untuk ubah volume (${volumeStep}%)`}
          >
            <span className="shrink-0 text-white/60">
              {volume === 0 ? (
                <VolumeMute className="h-5 w-5" />
              ) : volume < 0.5 ? (
                <VolumeLow className="h-5 w-5" />
              ) : (
                <VolumeHigh className="h-5 w-5" />
              )}
            </span>
            <input
              type="range"
              min={0}
              max={1}
              step={0.01}
              value={volume}
              onChange={(e) => onVolume(Number(e.target.value))}
              className="h-1 w-24 rounded-full"
              style={{ background: fill(volume * 100) }}
            />
          </div>
        </div>
      </div>
    </footer>
  );
}

function IconBtn({
  children,
  onClick,
  active,
  disabled,
  title,
}: {
  children: ReactNode;
  onClick: () => void;
  active?: boolean;
  disabled?: boolean;
  title?: string;
}) {
  return (
    <button
      onClick={onClick}
      disabled={disabled}
      title={title}
      className={`rounded-full p-2 transition hover:bg-white/10 disabled:opacity-30 ${
        active ? "text-white" : "text-white/60"
      }`}
    >
      {children}
    </button>
  );
}
