import { EQ_BANDS } from "../audio/engine";

const PRESETS: Record<string, number[]> = {
  Flat: [0, 0, 0, 0, 0, 0],
  Bass: [7, 5, 2, 0, 0, 0],
  Vocal: [-2, 0, 2, 4, 3, 0],
  Treble: [0, 0, 0, 2, 5, 7],
};

const label = (hz: number) => (hz >= 1000 ? `${hz / 1000}k` : `${hz}`);

/** 6-band graphic equalizer with a few presets. Gains are in dB (-12..12). */
export function Equalizer({
  gains,
  onChange,
  onPreset,
}: {
  gains: number[];
  onChange: (index: number, gainDb: number) => void;
  onPreset: (gains: number[]) => void;
}) {
  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-wrap gap-1.5">
        {Object.entries(PRESETS).map(([name, vals]) => (
          <button
            key={name}
            onClick={() => onPreset(vals)}
            className="rounded-full bg-white/10 px-3 py-1 text-xs font-medium text-white/80 transition hover:bg-white/20"
          >
            {name}
          </button>
        ))}
      </div>
      <div className="flex items-end gap-1">
        {EQ_BANDS.map((hz, i) => (
          <div key={hz} className="flex flex-1 flex-col items-center gap-2">
            <span className="w-full text-center text-[10px] tabular-nums text-white/45">
              {gains[i] > 0 ? "+" : ""}
              {gains[i]}
            </span>
            <input
              type="range"
              min={-12}
              max={12}
              step={1}
              value={gains[i]}
              onChange={(e) => onChange(i, Number(e.target.value))}
              className="h-24 w-5"
              style={{ writingMode: "vertical-lr", direction: "rtl" }}
            />
            <span className="w-full text-center text-[10px] tabular-nums text-white/55">
              {label(hz)}
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}
