import { useEffect, useState } from "react";
import type { RGB, Station } from "../types";
import { rgb } from "../lib/colors";

/**
 * Add / edit a radio station. A small frosted form over the adaptive gradient.
 * `station` null → "add" mode; otherwise pre-fills for editing. Closes on ✕,
 * backdrop click, or Esc.
 */
export function StationDialog({
  open,
  station,
  accent,
  onClose,
  onSave,
}: {
  open: boolean;
  station: Station | null;
  accent: RGB;
  onClose: () => void;
  onSave: (name: string, url: string) => void;
}) {
  const [name, setName] = useState("");
  const [url, setUrl] = useState("");

  // Reset the form whenever the dialog opens (for add) or targets a station.
  useEffect(() => {
    if (open) {
      setName(station?.name ?? "");
      setUrl(station?.url ?? "");
    }
  }, [open, station]);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, onClose]);

  if (!open) return null;
  const accentCss = rgb(accent, 1);
  const valid = name.trim().length > 0 && url.trim().length > 0;

  const submit = () => {
    if (valid) onSave(name.trim(), url.trim());
  };

  return (
    <div
      className="fixed inset-0 z-[60] flex items-center justify-center bg-black/45 p-8 backdrop-blur-md"
      onClick={onClose}
    >
      <form
        className="glass relative w-full max-w-sm rounded-3xl p-7 shadow-2xl"
        style={{
          boxShadow: `0 24px 70px -20px ${rgb(accent, 0.7)}`,
          animation: "aboutPop 0.18s ease-out",
        }}
        onClick={(e) => e.stopPropagation()}
        onSubmit={(e) => {
          e.preventDefault();
          submit();
        }}
      >
        <button
          type="button"
          onClick={onClose}
          title="Tutup"
          className="absolute right-4 top-4 flex h-8 w-8 items-center justify-center rounded-full bg-white/10 text-white/70 transition hover:bg-white/20 hover:text-white"
        >
          ✕
        </button>

        <h2 className="mb-5 text-lg font-bold tracking-tight text-white">
          {station ? "Edit Stasiun" : "Tambah Stasiun"}
        </h2>

        <label className="mb-1 block text-xs font-semibold uppercase tracking-wide text-white/45">
          Nama
        </label>
        <input
          autoFocus
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="Mis. Prambors FM"
          className="mb-4 w-full rounded-xl border border-white/15 bg-white/10 px-3 py-2.5 text-sm text-white outline-none transition focus:border-white/40"
        />

        <label className="mb-1 block text-xs font-semibold uppercase tracking-wide text-white/45">
          URL Stream
        </label>
        <input
          value={url}
          onChange={(e) => setUrl(e.target.value)}
          placeholder="https://…/stream"
          className="mb-6 w-full rounded-xl border border-white/15 bg-white/10 px-3 py-2.5 text-sm text-white outline-none transition focus:border-white/40"
        />

        <div className="flex gap-2">
          <button
            type="button"
            onClick={onClose}
            className="flex-1 rounded-xl border border-white/15 py-2.5 text-sm font-semibold text-white/80 transition hover:bg-white/10"
          >
            Batal
          </button>
          <button
            type="submit"
            disabled={!valid}
            className="flex-1 rounded-xl py-2.5 text-sm font-semibold text-white transition hover:brightness-110 disabled:opacity-40"
            style={{ background: accentCss }}
          >
            Simpan
          </button>
        </div>
      </form>
    </div>
  );
}
