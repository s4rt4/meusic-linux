/** Human-readable total duration, e.g. "1 jam 22 menit", "45 menit", "30 detik". */
export function fmtDurationLong(totalSeconds: number): string {
  const s = Math.floor(totalSeconds);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  if (h > 0) return m > 0 ? `${h} jam ${m} menit` : `${h} jam`;
  if (m > 0) return `${m} menit`;
  return `${s} detik`;
}

/** Format seconds as m:ss (or h:mm:ss for long tracks). */
export function fmtTime(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds < 0) seconds = 0;
  const s = Math.floor(seconds % 60);
  const m = Math.floor((seconds / 60) % 60);
  const h = Math.floor(seconds / 3600);
  const pad = (n: number) => n.toString().padStart(2, "0");
  return h > 0 ? `${h}:${pad(m)}:${pad(s)}` : `${m}:${pad(s)}`;
}
