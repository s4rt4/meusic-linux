import { hslToRgb } from "./colors";
import type { RGB } from "../types";

/**
 * Deterministic, vivid color for a radio station derived from its name — radio
 * streams have no cover art, so each station gets a stable colored tile (the
 * same name always yields the same hue). Mid saturation/lightness keeps tiles
 * legible against the adaptive gradient.
 */
export function stationColor(name: string): RGB {
  let h = 0;
  for (let i = 0; i < name.length; i++) h = (h * 31 + name.charCodeAt(i)) >>> 0;
  return hslToRgb(h % 360, 0.55, 0.5);
}

/**
 * A small adaptive-gradient palette for a station (no cover art exists), built
 * from its name's hue plus two neighboring hues — so the background matches the
 * station's tile color while it plays.
 */
export function stationPalette(name: string): RGB[] {
  let h = 0;
  for (let i = 0; i < name.length; i++) h = (h * 31 + name.charCodeAt(i)) >>> 0;
  const hue = h % 360;
  return [
    hslToRgb(hue, 0.6, 0.5),
    hslToRgb((hue + 32) % 360, 0.55, 0.45),
    hslToRgb((hue + 328) % 360, 0.5, 0.4),
  ];
}

/** Up to two uppercase initials for a station tile (e.g. "Gen FM" → "GF"). */
export function stationInitials(name: string): string {
  const words = name.replace(/[^\p{L}\p{N}\s]/gu, " ").trim().split(/\s+/);
  const letters = words
    .filter((w) => /\p{L}|\p{N}/u.test(w))
    .map((w) => w[0]);
  return (letters[0] ?? "?").concat(letters[1] ?? "").toUpperCase();
}
