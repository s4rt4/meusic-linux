import type { FolderNode } from "../lib/views";
import type { RGB } from "../types";
import { Chevron, Folder, FolderOpen, AudioLines } from "./icons";

/**
 * Windows-Explorer-style folder tree. Folders with subfolders expand/collapse
 * via the chevron; clicking a row selects that folder (its songs show in the
 * right pane). Expansion + selection state live in the parent so they persist.
 */
export function FolderTree({
  root,
  selectedPath,
  playingPath,
  accent,
  expanded,
  onSelect,
  onToggle,
}: {
  root: FolderNode;
  selectedPath: string;
  playingPath: string | null;
  accent: RGB;
  expanded: Set<string>;
  onSelect: (path: string) => void;
  onToggle: (path: string) => void;
}) {
  return (
    <div className="flex flex-col gap-0.5 p-2">
      <TreeRow
        node={root}
        depth={0}
        selectedPath={selectedPath}
        playingPath={playingPath}
        accent={accent}
        expanded={expanded}
        onSelect={onSelect}
        onToggle={onToggle}
      />
    </div>
  );
}

function TreeRow({
  node,
  depth,
  selectedPath,
  playingPath,
  accent,
  expanded,
  onSelect,
  onToggle,
}: {
  node: FolderNode;
  depth: number;
  selectedPath: string;
  playingPath: string | null;
  accent: RGB;
  expanded: Set<string>;
  onSelect: (path: string) => void;
  onToggle: (path: string) => void;
}) {
  const hasChildren = node.children.length > 0;
  const isOpen = expanded.has(node.path);
  const isSel = node.path === selectedPath;
  const isPlaying = node.path === playingPath;
  // Open-folder icon when this folder is expanded (a parent showing its tree)
  // or selected (its songs are listed on the right).
  const FolderIcon = isSel || (hasChildren && isOpen) ? FolderOpen : Folder;

  return (
    <>
      <div
        className={`group flex items-center rounded-lg pr-2 transition ${
          isSel ? "bg-white/15" : "hover:bg-white/8"
        }`}
        style={{ paddingLeft: depth * 14 + 2 }}
      >
        <button
          onClick={() => hasChildren && onToggle(node.path)}
          className="flex h-7 w-6 shrink-0 items-center justify-center text-white/40"
          tabIndex={hasChildren ? 0 : -1}
        >
          {hasChildren && (
            <Chevron
              className={`h-3.5 w-3.5 transition-transform ${isOpen ? "rotate-90" : ""}`}
            />
          )}
        </button>
        <button
          onClick={() => onSelect(node.path)}
          className="flex min-w-0 flex-1 items-center gap-2 py-1.5 text-left"
        >
          <FolderIcon className="h-4 w-4 shrink-0 text-sky-300/80" />
          <span
            className={`truncate text-sm ${
              isSel ? "font-medium text-white" : "text-white/80"
            }`}
          >
            {node.name}
          </span>
          <span className="ml-auto flex shrink-0 items-center gap-1.5 pl-2">
            {isPlaying && (
              <span style={{ color: `rgb(${accent.join(",")})` }}>
                <AudioLines className="h-4 w-4 animate-pulse" />
              </span>
            )}
            {node.tracks.length > 0 && (
              <span className="text-[10px] tabular-nums text-white/35">
                {node.tracks.length}
              </span>
            )}
          </span>
        </button>
      </div>
      {isOpen &&
        node.children.map((c) => (
          <TreeRow
            key={c.path}
            node={c}
            depth={depth + 1}
            selectedPath={selectedPath}
            playingPath={playingPath}
            accent={accent}
            expanded={expanded}
            onSelect={onSelect}
            onToggle={onToggle}
          />
        ))}
    </>
  );
}
