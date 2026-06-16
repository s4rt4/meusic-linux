import type { RGB } from "../types";

function loadImage(src: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.crossOrigin = "anonymous";
    img.onload = () => resolve(img);
    img.onerror = reject;
    img.src = src;
  });
}

function saturation(r: number, g: number, b: number): number {
  const max = Math.max(r, g, b) / 255;
  const min = Math.min(r, g, b) / 255;
  if (max === 0) return 0;
  return (max - min) / max;
}

function colorDist(a: RGB, b: RGB): number {
  return Math.sqrt(
    (a[0] - b[0]) ** 2 + (a[1] - b[1]) ** 2 + (a[2] - b[2]) ** 2
  );
}

function rgbToHsl([r, g, b]: RGB): [number, number, number] {
  r /= 255;
  g /= 255;
  b /= 255;
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const l = (max + min) / 2;
  let h = 0;
  let s = 0;
  const d = max - min;
  if (d !== 0) {
    s = d / (1 - Math.abs(2 * l - 1));
    switch (max) {
      case r:
        h = ((g - b) / d) % 6;
        break;
      case g:
        h = (b - r) / d + 2;
        break;
      default:
        h = (r - g) / d + 4;
    }
    h *= 60;
    if (h < 0) h += 360;
  }
  return [h, s, l];
}

export function hslToRgb(h: number, s: number, l: number): RGB {
  const c = (1 - Math.abs(2 * l - 1)) * s;
  const x = c * (1 - Math.abs(((h / 60) % 2) - 1));
  const m = l - c / 2;
  let r = 0;
  let g = 0;
  let b = 0;
  if (h < 60) [r, g, b] = [c, x, 0];
  else if (h < 120) [r, g, b] = [x, c, 0];
  else if (h < 180) [r, g, b] = [0, c, x];
  else if (h < 240) [r, g, b] = [0, x, c];
  else if (h < 300) [r, g, b] = [x, 0, c];
  else [r, g, b] = [c, 0, x];
  return [
    Math.round((r + m) * 255),
    Math.round((g + m) * 255),
    Math.round((b + m) * 255),
  ];
}

/**
 * Push a color toward a richer, more luminous version so even muted covers
 * yield a visible gradient. Saturation is boosted and lightness clamped to a
 * mid range (never near-black or washed out).
 */
export function vivify(c: RGB): RGB {
  const [h, s, l] = rgbToHsl(c);
  const ns = Math.min(1, s * 1.45 + 0.12);
  const nl = Math.min(0.6, Math.max(0.4, l));
  return hslToRgb(h, ns, nl);
}

/**
 * Extract a small palette of dominant, vivid colors from a cover-art image.
 *
 * The image is downscaled to 64×64, pixels are bucketed by a coarse 4-bit
 * quantization, and buckets are ranked by population weighted toward more
 * saturated colors — so an album's signature accent wins over muddy
 * backgrounds. Visually similar colors are de-duplicated. This is what feeds
 * the Amberol-style adaptive gradient background.
 */
export async function extractPalette(src: string, count = 4): Promise<RGB[]> {
  const size = 64;
  const canvas = document.createElement("canvas");
  canvas.width = size;
  canvas.height = size;
  const ctx = canvas.getContext("2d", { willReadFrequently: true });
  if (!ctx) return [[40, 40, 55]];

  let data: Uint8ClampedArray;
  try {
    const img = await loadImage(src);
    ctx.drawImage(img, 0, 0, size, size);
    data = ctx.getImageData(0, 0, size, size).data;
  } catch {
    return [[40, 40, 55]];
  }

  const buckets = new Map<string, { r: number; g: number; b: number; n: number }>();
  for (let i = 0; i < data.length; i += 4) {
    if (data[i + 3] < 125) continue; // skip transparent
    const r = data[i];
    const g = data[i + 1];
    const b = data[i + 2];
    const key = `${r >> 4},${g >> 4},${b >> 4}`;
    const bk = buckets.get(key) ?? { r: 0, g: 0, b: 0, n: 0 };
    bk.r += r;
    bk.g += g;
    bk.b += b;
    bk.n += 1;
    buckets.set(key, bk);
  }

  const scored = [...buckets.values()].map((bk) => {
    const r = bk.r / bk.n;
    const g = bk.g / bk.n;
    const b = bk.b / bk.n;
    return {
      rgb: [Math.round(r), Math.round(g), Math.round(b)] as RGB,
      score: bk.n * (0.4 + saturation(r, g, b)),
    };
  });
  scored.sort((a, b) => b.score - a.score);

  const picked: RGB[] = [];
  for (const c of scored) {
    if (picked.every((p) => colorDist(p, c.rgb) > 48)) picked.push(c.rgb);
    if (picked.length >= count) break;
  }

  const result = picked.length ? picked : [[60, 50, 90] as RGB];
  return result.map(vivify);
}

export const rgb = (c: RGB, alpha = 1) =>
  `rgba(${c[0]}, ${c[1]}, ${c[2]}, ${alpha})`;
