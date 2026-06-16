import type { ComponentType } from "react";
import type { Group } from "../lib/views";

/** Sidebar list for Albums / Artists modes: selectable grouped entries. */
export function GroupList({
  items,
  selectedKey,
  onSelect,
  icon: Icon,
}: {
  items: Group[];
  selectedKey: string;
  onSelect: (key: string) => void;
  icon: ComponentType<{ className?: string }>;
}) {
  return (
    <div className="flex flex-col gap-0.5 p-2">
      {items.map((g) => {
        const isSel = g.key === selectedKey;
        return (
          <button
            key={g.key}
            onClick={() => onSelect(g.key)}
            className={`flex items-center gap-3 rounded-lg px-2 py-2 text-left transition ${
              isSel ? "bg-white/15" : "hover:bg-white/8"
            }`}
          >
            <Icon className="h-5 w-5 shrink-0 text-white/45" />
            <div className="min-w-0">
              <div
                className={`truncate text-sm ${
                  isSel ? "font-medium text-white" : "text-white/85"
                }`}
              >
                {g.label}
              </div>
              {g.sublabel && (
                <div className="truncate text-xs text-white/45">{g.sublabel}</div>
              )}
            </div>
          </button>
        );
      })}
    </div>
  );
}
