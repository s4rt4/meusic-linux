import { useMemo } from "react";
import type { RGB, Station } from "../types";
import { stationColor, stationInitials } from "../lib/stationColor";
import { AudioLines, Pencil, Plus, Trash } from "./icons";

/**
 * Sidebar list of radio stations (replaces the music browse tree in radio
 * mode). Each row shows a deterministic-color chip + name, plays on click, and
 * reveals edit/delete on hover. A pinned button at the top adds new stations.
 * Filtered live by the shared search query.
 */
export function RadioList({
  stations,
  query,
  currentId,
  accent,
  onPlay,
  onAdd,
  onEdit,
  onDelete,
}: {
  stations: Station[];
  query: string;
  currentId: string | null;
  accent: RGB;
  onPlay: (station: Station) => void;
  onAdd: () => void;
  onEdit: (station: Station) => void;
  onDelete: (station: Station) => void;
}) {
  const accentCss = `rgb(${accent.join(",")})`;
  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return stations;
    return stations.filter((s) => s.name.toLowerCase().includes(q));
  }, [stations, query]);

  return (
    <div className="flex h-full flex-col">
      <div className="shrink-0 p-2">
        <button
          onClick={onAdd}
          className="flex w-full items-center justify-center gap-2 rounded-lg py-2.5 text-sm font-semibold text-white transition hover:brightness-110"
          style={{ background: accentCss }}
        >
          <Plus className="h-[18px] w-[18px]" />
          Tambah stasiun
        </button>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto px-2 pb-2">
        {filtered.length === 0 ? (
          <div className="px-2 py-8 text-center text-sm text-white/40">
            {query.trim() ? "Tidak ada stasiun yang cocok." : "Belum ada stasiun."}
          </div>
        ) : (
          <div className="flex flex-col gap-0.5">
            {filtered.map((s) => {
              const sel = s.id === currentId;
              const c = stationColor(s.name);
              return (
                <div key={s.id} className="group relative">
                  <button
                    onClick={() => onPlay(s)}
                    title={`Putar ${s.name}`}
                    className={`flex w-full items-center gap-3 rounded-lg py-2 pl-2 pr-16 text-left transition ${
                      sel ? "bg-white/15" : "hover:bg-white/8"
                    }`}
                  >
                    <span
                      className="flex h-9 w-9 shrink-0 items-center justify-center rounded-md text-[11px] font-black text-white/90"
                      style={{
                        background: `linear-gradient(135deg, rgb(${c.join(",")}), rgba(${c.join(",")},0.55))`,
                      }}
                    >
                      {stationInitials(s.name)}
                    </span>
                    <span
                      className={`truncate text-sm ${
                        sel ? "font-semibold text-white" : "text-white/85"
                      }`}
                    >
                      {s.name}
                    </span>
                    {sel && <AudioLines className="ml-auto h-4 w-4 shrink-0" />}
                  </button>

                  {/* Hover actions (edit / delete) */}
                  <span className="absolute right-1.5 top-1/2 flex -translate-y-1/2 gap-1 opacity-0 transition group-hover:opacity-100">
                    <button
                      type="button"
                      title="Edit"
                      onClick={() => onEdit(s)}
                      className="flex h-7 w-7 items-center justify-center rounded-md bg-white/10 text-white/80 transition hover:bg-white/20"
                    >
                      <Pencil className="h-3.5 w-3.5" />
                    </button>
                    <button
                      type="button"
                      title="Hapus"
                      onClick={() => onDelete(s)}
                      className="flex h-7 w-7 items-center justify-center rounded-md bg-white/10 text-white/80 transition hover:bg-red-500/80"
                    >
                      <Trash className="h-3.5 w-3.5" />
                    </button>
                  </span>
                </div>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}
