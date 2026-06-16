import type { RGB } from "../types";
import { rgb } from "../lib/colors";

// Fewer, slightly smaller blobs + a smaller blur radius keep the look while
// cutting GPU compositing cost during playback.
const BLOBS = [
  { top: "-10%", left: "-8%", size: "68vw", dur: "28s" },
  { top: "30%", left: "55%", size: "60vw", dur: "34s" },
  { top: "54%", left: "6%", size: "56vw", dur: "40s" },
];

/**
 * Amberol-style adaptive background: several large, blurred color blobs that
 * drift slowly and cross-fade (CSS transition) whenever the palette changes
 * — i.e. whenever the playing track's cover art changes.
 *
 * The drift animation is paused when `active` is false (nothing playing / window
 * unfocused) so the GPU isn't compositing the heavy blur every frame at idle.
 */
export function GradientBackground({
  palette,
  active,
  enabled,
}: {
  palette: RGB[];
  active: boolean;
  enabled: boolean;
}) {
  const colors = palette.length ? palette : ([[34, 34, 50]] as RGB[]);

  // Power-saving: a flat dark background, no blurred blobs to composite.
  if (!enabled) {
    return <div className="fixed inset-0 -z-10 bg-[#0a0a10]" />;
  }

  return (
    <div className="fixed inset-0 -z-10 overflow-hidden bg-[#07070b]">
      {BLOBS.map((b, i) => {
        const c = colors[i % colors.length];
        return (
          <div
            key={i}
            style={{
              position: "absolute",
              top: b.top,
              left: b.left,
              width: b.size,
              height: b.size,
              borderRadius: "50%",
              background: `radial-gradient(circle, ${rgb(c, 0.85)} 0%, ${rgb(c, 0)} 68%)`,
              filter: "blur(30px)",
              animation: `drift ${b.dur} ease-in-out infinite`,
              animationPlayState: active ? "running" : "paused",
              transition: "background 1.2s ease",
              willChange: "transform",
            }}
          />
        );
      })}
      {/* Vignette to keep foreground text legible */}
      <div
        className="absolute inset-0"
        style={{
          background:
            "radial-gradient(circle at 50% 40%, rgba(0,0,0,0) 0%, rgba(0,0,0,0.3) 100%)",
        }}
      />
    </div>
  );
}
