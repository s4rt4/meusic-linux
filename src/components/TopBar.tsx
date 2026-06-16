import { useState, type ComponentType } from "react";
import type { AppMode, RGB } from "../types";
import { Folder, FolderPlus, Search, Album, Artist, MusicNote, Menu } from "./icons";
import { SettingsMenu } from "./SettingsMenu";
import type { Settings } from "../hooks/useSettings";
import logo from "../assets/logo.svg";
import logoGreen from "../assets/logo-green.svg";

export type Mode = "folders" | "albums" | "artists" | "songs";

const TABS: { key: Mode; label: string; icon: ComponentType<{ className?: string }> }[] = [
  { key: "folders", label: "Folders", icon: Folder },
  { key: "albums", label: "Albums", icon: Album },
  { key: "artists", label: "Artists", icon: Artist },
  { key: "songs", label: "Songs", icon: MusicNote },
];

/** Top ribbon: logo, mode tabs (with icons), search, open-folder. */
export function TopBar({
  accent,
  mode,
  onMode,
  query,
  onQuery,
  onPick,
  scanning,
  powerSave,
  settings,
  onUpdateSetting,
  onAbout,
  appMode,
  onAppMode,
}: {
  accent: RGB;
  mode: Mode;
  onMode: (m: Mode) => void;
  query: string;
  onQuery: (q: string) => void;
  onPick: () => void;
  scanning: boolean;
  powerSave: boolean;
  settings: Settings;
  onUpdateSetting: <K extends keyof Settings>(key: K, value: Settings[K]) => void;
  onAbout: () => void;
  appMode: AppMode;
  onAppMode: (m: AppMode) => void;
}) {
  const accentCss = `rgb(${accent.join(",")})`;
  const [menuOpen, setMenuOpen] = useState(false);
  const isRadio = appMode === "radio";
  return (
    <header className="flex h-[68px] shrink-0 items-center gap-4 pl-5 pr-5 lg:gap-8 lg:pl-9 lg:pr-7">
      <button
        onClick={onAbout}
        title="Tentang meusic"
        className="shrink-0 rounded-md transition hover:opacity-80 focus:outline-none focus-visible:ring-2 focus-visible:ring-white/30"
      >
        <img
          src={powerSave ? logoGreen : logo}
          alt="meusic"
          className="h-7 select-none lg:h-8"
          draggable={false}
        />
      </button>

      <nav className={`shrink-0 items-center ${isRadio ? "hidden" : "flex"}`}>
        {TABS.map((t, i) => {
          const active = t.key === mode;
          const Icon = t.icon;
          return (
            <div key={t.key} className="flex items-center">
              {i > 0 && <span className="mx-1 h-4 w-px bg-white/15 lg:mx-1.5" />}
              <button
                onClick={() => onMode(t.key)}
                title={t.label}
                className={`relative flex items-center gap-2 px-2.5 py-2 text-sm font-semibold transition ${
                  active ? "text-white" : "text-white/55 hover:text-white/85"
                }`}
              >
                <Icon className="h-[18px] w-[18px] shrink-0" />
                <span className="hidden lg:inline">{t.label}</span>
                {active && (
                  <span
                    className="absolute -bottom-1 left-0 right-0 h-[3px] rounded-full"
                    style={{ background: accentCss }}
                  />
                )}
              </button>
            </div>
          );
        })}
      </nav>

      {/* Search grows to fill but is capped, and shrinks freely on narrow windows */}
      <div className="ml-auto flex min-w-0 flex-1 items-center justify-end gap-4 lg:gap-5">
        <div className="flex min-w-0 max-w-72 flex-1 items-center gap-2.5 rounded-full border border-white/15 bg-white/12 px-4 py-2.5 transition focus-within:border-white/35 focus-within:bg-white/[0.18] lg:px-5">
          <input
            value={query}
            onChange={(e) => onQuery(e.target.value)}
            placeholder={isRadio ? "Cari stasiun…" : "Cari…"}
            className="w-full min-w-0 bg-transparent text-sm text-white outline-none placeholder:text-white/40"
          />
          <Search className="h-[18px] w-[18px] shrink-0 text-white/45" />
        </div>

        {!isRadio && (
          <button
            onClick={onPick}
            disabled={scanning}
            title="Buka Folder"
            className="flex shrink-0 items-center gap-2 text-sm font-semibold text-white/80 transition hover:text-white disabled:opacity-60"
          >
            <FolderPlus className="h-[18px] w-[18px] shrink-0" />
            <span className="hidden lg:inline">{scanning ? "Memindai…" : "Buka Folder"}</span>
          </button>
        )}

        <div className="relative shrink-0">
          <button
            onClick={() => setMenuOpen((o) => !o)}
            title="Menu"
            className={`rounded-lg p-2 transition hover:bg-white/10 hover:text-white ${
              menuOpen ? "text-white" : "text-white/75"
            }`}
          >
            <Menu className="h-[18px] w-[18px]" />
          </button>
          {menuOpen && (
            <>
              <div className="fixed inset-0 z-40" onClick={() => setMenuOpen(false)} />
              <div className="glass-strong absolute right-0 top-full z-50 mt-2 w-80 rounded-xl p-2 shadow-2xl">
                <SettingsMenu
                  settings={settings}
                  onUpdate={onUpdateSetting}
                  accent={accent}
                  appMode={appMode}
                  onAppMode={onAppMode}
                />
              </div>
            </>
          )}
        </div>
      </div>
    </header>
  );
}
