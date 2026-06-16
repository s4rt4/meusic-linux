import { useEffect } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { RGB } from "../types";
import { rgb } from "../lib/colors";
import { APP } from "../lib/appInfo";
import { Github, Heart } from "./icons";
import logo from "../assets/logo.svg";

/**
 * "About meusic" dialog — a small frosted card over the adaptive gradient,
 * opened by clicking the logo. Closes on ✕, backdrop click, or Esc.
 */
export function AboutDialog({
  open,
  onClose,
  accent,
}: {
  open: boolean;
  onClose: () => void;
  accent: RGB;
}) {
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, onClose]);

  if (!open) return null;
  const accentCss = rgb(accent, 1);

  return (
    <div
      className="fixed inset-0 z-[60] flex items-center justify-center bg-black/45 p-8 backdrop-blur-md"
      onClick={onClose}
    >
      <div
        className="glass relative w-full max-w-sm rounded-3xl p-8 text-center shadow-2xl"
        style={{
          boxShadow: `0 24px 70px -20px ${rgb(accent, 0.7)}`,
          animation: "aboutPop 0.18s ease-out",
        }}
        onClick={(e) => e.stopPropagation()}
      >
        <button
          onClick={onClose}
          title="Tutup"
          className="absolute right-4 top-4 flex h-8 w-8 items-center justify-center rounded-full bg-white/10 text-white/70 transition hover:bg-white/20 hover:text-white"
        >
          ✕
        </button>

        {/* Brand */}
        <div
          className="mx-auto mb-5 flex h-20 w-20 items-center justify-center rounded-2xl"
          style={{
            background: rgb(accent, 0.16),
            boxShadow: `0 12px 36px -10px ${rgb(accent, 0.6)}`,
          }}
        >
          <img src={logo} alt="meusic" className="h-9 select-none" draggable={false} />
        </div>

        <div className="flex items-center justify-center gap-2">
          <h2 className="text-2xl font-bold tracking-tight text-white">{APP.name}</h2>
          <span
            className="rounded-full px-2 py-0.5 text-xs font-semibold text-white"
            style={{ background: rgb(accent, 0.35) }}
          >
            v{APP.version}
          </span>
        </div>

        <p className="mx-auto mt-2 max-w-[18rem] text-sm leading-relaxed text-white/65">
          {APP.description}
        </p>

        <div className="my-6 h-px w-full bg-white/10" />

        {/* Facts */}
        <dl className="space-y-2 text-sm">
          <Row label="Dibuat oleh" value={APP.author} />
          <Row label="Lisensi" value={APP.license} />
          <Row label="Dibangun dengan" value="Tauri · React · Rust" />
        </dl>

        {/* Actions */}
        <button
          onClick={() => void openUrl(APP.repo).catch(() => {})}
          className="mt-6 flex w-full items-center justify-center gap-2 rounded-xl py-2.5 text-sm font-semibold text-white transition hover:brightness-110"
          style={{ background: accentCss }}
        >
          <Github className="h-[18px] w-[18px]" />
          Lihat di GitHub
        </button>

        <p className="mt-5 flex items-center justify-center gap-1.5 text-xs text-white/40">
          Dibuat dengan
          <span style={{ color: accentCss }}>
            <Heart className="h-3.5 w-3.5" />
          </span>
          untuk pendengar musik
        </p>
      </div>
    </div>
  );
}

function Row({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-center justify-between gap-4">
      <dt className="text-white/45">{label}</dt>
      <dd className="font-medium text-white/85">{value}</dd>
    </div>
  );
}
