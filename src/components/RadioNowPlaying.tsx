import type { RadioStatus, RGB, Station } from "../types";
import { rgb } from "../lib/colors";
import { stationColor, stationInitials } from "../lib/stationColor";
import { Leaf, Radio } from "./icons";
import { Visualizer } from "./Visualizer";

/**
 * Radio "now playing" main pane: a big station chip, the station name, the live
 * ICY song title + stream quality, and the spectrum visualizer. Title/codec/
 * bitrate come from the stream once playback lands (Fase 2); until then they
 * show placeholders.
 */
export function RadioNowPlaying({
  station,
  accent,
  playing,
  title,
  codec,
  bitrate,
  error,
  status,
  active,
  powerSave,
}: {
  station: Station | null;
  accent: RGB;
  playing: boolean;
  title: string | null;
  codec: string | null;
  bitrate: number | null;
  error: string | null;
  status: RadioStatus;
  active: boolean;
  powerSave: boolean;
}) {
  if (!station) {
    return (
      <section className="glass flex h-full min-w-0 flex-1 flex-col items-center justify-center gap-3 rounded-2xl text-center text-white/40">
        <Radio className="h-12 w-12" />
        <p className="text-sm">Pilih stasiun radio dari daftar di kiri.</p>
      </section>
    );
  }

  const c = stationColor(station.name);
  const hasQuality = Boolean(codec) || (bitrate ?? 0) > 0;
  const busy = status === "connecting" || status === "reconnecting" || status === "buffering";
  const busyText =
    status === "reconnecting"
      ? "Menyambung ulang…"
      : status === "buffering"
        ? "Menyangga…"
        : "Menyambungkan…";
  const badgeLabel = error ? "Gagal" : busy ? "Menyambung" : playing ? "Live" : "Siap diputar";
  // dot/text/bg colors per state: error=red, busy=amber, live=accent, idle=grey
  const dotColor = error ? "#ef4444" : busy ? "#f59e0b" : playing ? "#ff5b5b" : "rgba(255,255,255,0.4)";
  const badgeText = error ? "#fca5a5" : busy ? "#fbbf24" : playing ? "#fff" : "rgba(255,255,255,0.6)";
  const badgeBg = error
    ? "rgba(239,68,68,0.25)"
    : busy
      ? "rgba(245,158,11,0.2)"
      : playing
        ? rgb(accent, 0.3)
        : "rgba(255,255,255,0.1)";
  const statusLine = error ?? (busy ? busyText : title ?? (playing ? "Siaran langsung" : "Tidak ada info lagu"));

  return (
    <section className="glass flex h-full min-w-0 flex-1 flex-col items-center justify-center gap-6 overflow-hidden rounded-2xl p-8">
      {/* Station chip */}
      <div
        className="flex aspect-square w-[min(34vh,260px)] items-center justify-center rounded-3xl text-6xl font-black text-white/90 shadow-2xl"
        style={{
          background: `linear-gradient(135deg, rgb(${c.join(",")}), rgba(${c.join(",")},0.5))`,
          boxShadow: `0 30px 80px -24px ${rgb(c, 0.8)}`,
        }}
      >
        {stationInitials(station.name)}
      </div>

      {/* Name + status */}
      <div className="max-w-[80%] text-center">
        <div className="mb-2 flex items-center justify-center gap-2">
          <span
            className="flex items-center gap-1.5 rounded-full px-2.5 py-0.5 text-[11px] font-semibold uppercase tracking-wide"
            style={{ background: badgeBg, color: badgeText }}
          >
            <span
              className={`h-2 w-2 rounded-full ${busy ? "animate-pulse" : ""}`}
              style={{ background: dotColor }}
            />
            {badgeLabel}
          </span>
        </div>

        <h2 className="truncate text-3xl font-bold text-white">{station.name}</h2>

        <p
          className={`mt-1 truncate text-base ${
            error ? "text-red-300" : busy ? "text-amber-300" : "text-white/70"
          }`}
        >
          {statusLine}
        </p>

        {/* Stream quality */}
        <div className="mt-3 flex items-center justify-center gap-2 text-xs text-white/45">
          {hasQuality ? (
            <>
              {codec && (
                <span
                  className="rounded px-1.5 py-px font-medium uppercase"
                  style={{ background: rgb(accent, 0.25), color: rgb(accent, 1) }}
                >
                  {codec}
                </span>
              )}
              {(bitrate ?? 0) > 0 && <span>{bitrate} kbps</span>}
            </>
          ) : (
            <span className="text-white/30">Kualitas stream —</span>
          )}
        </div>
      </div>

      {/* Visualizer — replaced by a static indicator in power-save (no animations). */}
      {powerSave ? (
        <div className="flex h-20 w-[min(80%,520px)] items-center justify-center gap-2 text-sm font-medium text-[#44aa00]">
          <Leaf className="h-5 w-5" />
          Hemat daya aktif — visualizer dimatikan
        </div>
      ) : (
        <div className="h-20 w-[min(80%,520px)]">
          <Visualizer color={accent} active={active} />
        </div>
      )}
    </section>
  );
}
