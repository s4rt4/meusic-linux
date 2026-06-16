import { useEffect, useRef } from "react";
import { engine } from "../audio/engine";
import type { RGB } from "../types";

/**
 * Spectrum visualizer. Reads the engine's analyser every animation frame and
 * draws rounded frequency bars tinted with the cover-art accent color.
 */
export function Visualizer({ color, active }: { color: RGB; active: boolean }) {
  const ref = useRef<HTMLCanvasElement>(null);
  const colorRef = useRef(color);
  colorRef.current = color;

  useEffect(() => {
    const canvas = ref.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    if (!active) return; // paused: stop the rAF loop entirely

    let raf = 0;
    const BINS = 56;

    const draw = () => {
      raf = requestAnimationFrame(draw);

      // Match backing resolution to displayed size (DPR-aware).
      const dpr = window.devicePixelRatio || 1;
      const w = canvas.clientWidth * dpr;
      const h = canvas.clientHeight * dpr;
      if (canvas.width !== w || canvas.height !== h) {
        canvas.width = w;
        canvas.height = h;
      }

      ctx.clearRect(0, 0, w, h);
      if (!engine.hasGraph) return;

      const data = engine.getSpectrum();
      const step = Math.max(1, Math.floor((data.length * 0.7) / BINS));
      const bw = w / BINS;
      const [r, g, b] = colorRef.current;

      for (let i = 0; i < BINS; i++) {
        let v = 0;
        for (let j = 0; j < step; j++) v += data[i * step + j] ?? 0;
        v /= step;
        const bh = Math.max(2 * dpr, (v / 255) ** 1.4 * h);
        const grd = ctx.createLinearGradient(0, h, 0, h - bh);
        grd.addColorStop(0, `rgba(${r},${g},${b},0.18)`);
        grd.addColorStop(1, `rgba(${r},${g},${b},0.92)`);
        ctx.fillStyle = grd;
        const bar = bw * 0.6;
        const x = i * bw + (bw - bar) / 2;
        ctx.beginPath();
        ctx.roundRect(x, h - bh, bar, bh, bar / 2);
        ctx.fill();
      }
    };
    draw();
    return () => cancelAnimationFrame(raf);
  }, [active]);

  return <canvas ref={ref} className="h-full w-full" />;
}
