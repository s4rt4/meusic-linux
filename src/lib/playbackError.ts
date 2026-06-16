/**
 * Formatting for playback failures written to meusic.log.
 *
 * Kept as a pure module (no DOM / Tauri access) so it is unit-testable. The
 * <audio> element and `invoke` live in usePlayer; this only turns a snapshot of
 * the error state into a log line.
 *
 * Why this exists: WebView2/Chromium's media demuxer is far stricter than the
 * ffmpeg-based decoders other players use. e.g. a FLAC whose embedded cover-art
 * PICTURE block has an empty MIME type makes Chromium reject the whole file with
 * MediaError 4 (SRC_NOT_SUPPORTED / DEMUXER_ERROR_COULD_NOT_OPEN), while every
 * full-ffmpeg player plays it fine. These errors were previously swallowed by
 * empty .catch() handlers, so failures were invisible; logging surfaces them.
 */

/** HTMLMediaElement MediaError.code → readable name. */
export const MEDIA_ERR: Record<number, string> = {
  1: "ABORTED",
  2: "NETWORK",
  3: "DECODE",
  4: "SRC_NOT_SUPPORTED",
};

export interface AudioErrorState {
  /** MediaError.code, if any. */
  code?: number;
  /** MediaError.message, if any. */
  mediaMessage?: string;
  readyState: number;
  networkState: number;
  /** Decoded source URL (human-readable path). */
  src: string;
}

/** Build a single log line describing a playback failure. */
export function formatPlaybackError(
  where: string,
  state: AudioErrorState,
  reason?: unknown,
): string {
  const parts = [
    `playback ${where}`,
    state.code ? `MediaError ${state.code} (${MEDIA_ERR[state.code] ?? "?"})` : null,
    state.mediaMessage ? `msg="${state.mediaMessage}"` : null,
    reason ? `reason=${String((reason as Error)?.message ?? reason)}` : null,
    `readyState=${state.readyState} networkState=${state.networkState}`,
    `src=${state.src}`,
  ].filter(Boolean);
  return parts.join(" | ");
}
