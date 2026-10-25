import { useEffect, useRef, useState, type DragEvent, type ReactNode } from "react";
import { Menu } from "@tauri-apps/api/menu";
import { ChevronDown, FolderPlus, LayoutGrid, Plus, Search, Trash2, X } from "lucide-react";
import { controller } from "../app/controller";
import { groupArtboards, moveCommand, type DropTarget } from "../app/library";
import { matchesSearch, UNGROUPED_TITLE } from "../canvas/overviewLayout";
import type { ArtboardId, ArtboardSummary, Folder, FolderId } from "../engine/types";
import {
  getEditor,
  selectActiveArtboard,
  selectArtboards,
  selectFolders,
  setEditor,
  useEditor,
} from "../state/editor";
import { IconButton, inputClass, plainText } from "./ui/primitives";

const DRAG_TYPE = "application/x-dotify-artboard";

/** Library sidebar: every artboard of the project, grouped by folder. */
export function LibraryPanel() {
  const width = useEditor((s) => s.libraryWidth);
  const artboards = useEditor(selectArtboards);
  const folders = useEditor(selectFolders);
  const search = useEditor((s) => s.librarySearch);
  const mode = useEditor((s) => s.mode);
  const [collapsed, setCollapsed] = useState<Set<FolderId | null>>(new Set());

  const visible = artboards.filter((a) => matchesSearch(a, folders, search));
  const groups = groupArtboards(
    visible,
    folders.map((f) => f.id),
  );
  const searching = search.trim() !== "";
  const toggle = (id: FolderId | null) =>
    setCollapsed((c) => {
      const next = new Set(c);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });

  const startResize = (e: React.PointerEvent) => {
    const startX = e.clientX;
    const start = width;
    const move = (ev: PointerEvent) =>
      setEditor({ libraryWidth: Math.min(400, Math.max(180, start + (ev.clientX - startX))) });
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };

  return (
    <aside
      aria-label="Sprites"
      style={{ width }}
      className="relative flex shrink-0 flex-col border-r border-line bg-panel"
    >
      <div
        role="separator"
        aria-orientation="vertical"
        aria-label="Resize sprites panel"
        onPointerDown={startResize}
        className="absolute top-0 -right-1 bottom-0 z-10 w-2 cursor-col-resize"
      />
      <header className="flex h-8 shrink-0 items-center gap-1 pr-1.5 pl-2.5">
        <span className="flex-1 text-[11px] font-semibold tracking-wide text-dim uppercase">
          Sprites
        </span>
        <IconButton
          label="Sprite overview"
          shortcut="⇧⌘O"
          active={mode === "overview"}
          onClick={() => controller.setMode(mode === "overview" ? "editor" : "overview")}
        >
          <LayoutGrid size={13} />
        </IconButton>
        <IconButton
          label="New folder"
          onClick={() => void controller.runProject({ type: "create_folder" })}
        >
          <FolderPlus size={13} />
        </IconButton>
      </header>
      <div className="shrink-0 px-2 pb-2">
        <button
          type="button"
          title="New sprite (⌘N)"
          onClick={() => setEditor({ newArtboardOpen: true })}
          className="flex h-7 w-full items-center justify-center gap-1.5 rounded-[5px] border border-dashed border-line-strong text-[12px] text-dim hover:border-accent hover:text-fg"
        >
          <Plus size={13} /> New Sprite
          <span className="font-mono text-[10px] text-faint">⌘N</span>
        </button>
      </div>
      <div className="relative shrink-0 px-2 pb-2">
        <Search
          size={12}
          className="pointer-events-none absolute top-[8px] left-[15px] text-faint"
        />
        <input
          {...plainText}
          type="text"
          aria-label="Search sprites"
          placeholder="Search"
          value={search}
          onChange={(e) => setEditor({ librarySearch: e.target.value })}
          onKeyDown={(e) => e.key === "Escape" && setEditor({ librarySearch: "" })}
          className={`${inputClass} pr-6 pl-6`}
        />
        {searching && (
          <IconButton
            label="Clear search"
            onClick={() => setEditor({ librarySearch: "" })}
            className="absolute top-[3px] right-[11px]"
          >
            <X size={11} />
          </IconButton>
        )}
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto px-1 pb-2">
        <Group
          folder={null}
          title={UNGROUPED_TITLE}
          items={groups.get(null) ?? []}
          collapsed={collapsed.has(null)}
          onToggle={() => toggle(null)}
          hideWhenEmpty={folders.length === 0 || searching}
        />
        {folders.map((f) => {
          const items = groups.get(f.id) ?? [];
          if (searching && items.length === 0) return null;
          return (
            <Group
              key={f.id}
              folder={f}
              title={f.name}
              items={items}
              collapsed={collapsed.has(f.id) && !searching}
              onToggle={() => toggle(f.id)}
            />
          );
        })}
        {searching && visible.length === 0 && (
          <p className="px-2 py-3 text-center text-faint">No artboards match “{search}”.</p>
        )}
      </div>
    </aside>
  );
}

function draggedId(e: DragEvent): ArtboardId | null {
  const raw = e.dataTransfer.getData(DRAG_TYPE);
  return raw ? Number(raw) : null;
}

function drop(id: ArtboardId | null, target: DropTarget) {
  if (id == null) return;
  const cmd = moveCommand(getEditor().session?.project.artboards ?? [], id, target);
  if (cmd) void controller.runProject(cmd);
}

function Group({
  folder,
  title,
  items,
  collapsed,
  onToggle,
  hideWhenEmpty,
}: {
  folder: Folder | null;
  title: string;
  items: ArtboardSummary[];
  collapsed: boolean;
  onToggle: () => void;
  hideWhenEmpty?: boolean;
}) {
  const [over, setOver] = useState(false);
  const folderId = folder?.id ?? null;
  // Without folders the ungrouped list needs no header.
  const showHeader = folder !== null || !hideWhenEmpty;
  if (folder === null && hideWhenEmpty && items.length === 0) return null;
  return (
    <div
      onDragOver={(e) => {
        if (!e.dataTransfer.types.includes(DRAG_TYPE)) return;
        e.preventDefault();
        setOver(true);
      }}
      onDragLeave={(e) => {
        if (!e.currentTarget.contains(e.relatedTarget as Node | null)) setOver(false);
      }}
      onDrop={(e) => {
        e.preventDefault();
        setOver(false);
        drop(draggedId(e), { folder: folderId });
      }}
      className={`rounded-[5px] ${over ? "bg-accent-soft/40" : ""}`}
    >
      {showHeader && (
        <FolderHeader
          folder={folder}
          title={title}
          count={items.length}
          collapsed={collapsed}
          onToggle={onToggle}
        />
      )}
      {!collapsed && (
        <ul role="listbox" aria-label={title} className={showHeader ? "pl-2" : ""}>
          {items.map((a) => (
            <ArtboardRow key={a.id} artboard={a} />
          ))}
          {items.length === 0 && (
            <li className="h-7 pl-3 leading-7 text-faint">Drop sprites here</li>
          )}
        </ul>
      )}
    </div>
  );
}

function FolderHeader({
  folder,
  title,
  count,
  collapsed,
  onToggle,
}: {
  folder: Folder | null;
  title: string;
  count: number;
  collapsed: boolean;
  onToggle: () => void;
}) {
  const [editing, setEditing] = useState(false);
  return (
    <div className="group flex h-7 items-center gap-1 pr-1">
      <button
        type="button"
        aria-expanded={!collapsed}
        onClick={onToggle}
        className="flex h-6 w-5 shrink-0 items-center justify-center text-dim hover:text-fg"
      >
        <ChevronDown
          size={12}
          className={`transition-transform ${collapsed ? "-rotate-90" : ""}`}
        />
      </button>
      {editing && folder ? (
        <RenameInput
          label="Folder name"
          value={folder.name}
          onDone={(name) => {
            setEditing(false);
            if (name)
              void controller.runProject({ type: "rename_folder", folder: folder.id, name });
          }}
        />
      ) : (
        <span
          onDoubleClick={() => folder && setEditing(true)}
          title={folder ? "Double-click to rename" : undefined}
          className="min-w-0 flex-1 truncate text-[11px] font-semibold tracking-wide text-dim uppercase"
        >
          {title}
        </span>
      )}
      <span className="font-mono text-[10px] text-faint tabular-nums">{count}</span>
      {folder && (
        <IconButton
          label="Delete folder (keeps its sprites)"
          onClick={() => void controller.runProject({ type: "delete_folder", folder: folder.id })}
          className="opacity-0 group-hover:opacity-100 focus-visible:opacity-100"
        >
          <Trash2 size={11} />
        </IconButton>
      )}
    </div>
  );
}

function ArtboardRow({ artboard }: { artboard: ArtboardSummary }) {
  const active = useEditor((s) => selectActiveArtboard(s) === artboard.id);
  const [editing, setEditing] = useState(false);
  const [dropSide, setDropSide] = useState<"before" | "after" | null>(null);
  const rename = (name: string) =>
    void controller.runProject({ type: "rename_artboard", artboard: artboard.id, name });

  const openMenu = async (e: React.MouseEvent) => {
    e.preventDefault();
    void controller.selectArtboard(artboard.id);
    const s = getEditor();
    const folders = s.session?.project.folders ?? [];
    const many = (s.session?.project.artboards.length ?? 0) > 1;
    const move = (folder: FolderId | null) => () => drop(artboard.id, { folder });
    const menu = await Menu.new({
      items: [
        { text: "Open in Editor", action: () => void controller.selectArtboard(artboard.id, true) },
        { text: "Rename", action: () => setEditing(true) },
        {
          text: "Duplicate",
          action: () =>
            void controller.runProject({ type: "duplicate_artboard", artboard: artboard.id }),
        },
        {
          text: "Move to",
          items: [
            { text: UNGROUPED_TITLE, enabled: artboard.folder !== null, action: move(null) },
            ...(folders.length ? [{ item: "Separator" as const }] : []),
            ...folders.map((f) => ({
              text: f.name,
              enabled: f.id !== artboard.folder,
              action: move(f.id),
            })),
          ],
        },
        { item: "Separator" },
        {
          text: "Delete",
          enabled: many,
          action: () =>
            void controller.runProject({ type: "delete_artboard", artboard: artboard.id }),
        },
      ],
    });
    await menu.popup();
  };

  return (
    <li
      role="option"
      aria-selected={active}
      draggable={!editing}
      onDragStart={(e) => {
        e.dataTransfer.setData(DRAG_TYPE, String(artboard.id));
        e.dataTransfer.effectAllowed = "move";
      }}
      onDragOver={(e) => {
        if (!e.dataTransfer.types.includes(DRAG_TYPE)) return;
        e.preventDefault();
        e.stopPropagation();
        const r = e.currentTarget.getBoundingClientRect();
        setDropSide(e.clientY < r.top + r.height / 2 ? "before" : "after");
      }}
      onDragLeave={() => setDropSide(null)}
      onDrop={(e) => {
        e.preventDefault();
        e.stopPropagation();
        const side = dropSide;
        setDropSide(null);
        drop(draggedId(e), side === "before" ? { before: artboard.id } : { after: artboard.id });
      }}
      onClick={() => void controller.selectArtboard(artboard.id)}
      onDoubleClick={() => void controller.selectArtboard(artboard.id, true)}
      onContextMenu={(e) => void openMenu(e)}
      className={`relative flex h-9 items-center gap-2 rounded-[5px] pr-1.5 pl-1 ${
        active ? "bg-accent-soft" : "hover:bg-hover"
      }`}
    >
      {dropSide && (
        <span
          className={`pointer-events-none absolute right-1 left-1 h-0.5 rounded bg-accent ${
            dropSide === "before" ? "-top-px" : "-bottom-px"
          }`}
        />
      )}
      <ArtboardThumb id={artboard.id} />
      {editing ? (
        <RenameInput
          label="Sprite name"
          value={artboard.name}
          onDone={(name) => {
            setEditing(false);
            if (name) rename(name);
          }}
        />
      ) : (
        <div className="min-w-0 flex-1">
          <div
            onDoubleClick={(e) => {
              e.stopPropagation();
              setEditing(true);
            }}
            title="Double-click to rename"
            className={`truncate ${active ? "text-fg" : "text-dim"}`}
          >
            {artboard.name}
          </div>
          <div className="font-mono text-[10px] text-faint tabular-nums">
            {artboard.width}×{artboard.height}
            {artboard.frames.length > 1 && ` · ${artboard.frames.length}f`}
          </div>
        </div>
      )}
    </li>
  );
}

function RenameInput({
  label,
  value,
  onDone,
}: {
  label: string;
  value: string;
  onDone: (name: string | null) => void;
}) {
  return (
    <input
      {...plainText}
      autoFocus
      aria-label={label}
      defaultValue={value}
      className={`${inputClass} h-6`}
      onClick={(e) => e.stopPropagation()}
      onDoubleClick={(e) => e.stopPropagation()}
      onBlur={(e) => {
        const name = e.target.value.trim();
        onDone(name && name !== value ? name : null);
      }}
      onKeyDown={(e) => {
        e.stopPropagation();
        if (e.key === "Enter") (e.target as HTMLInputElement).blur();
        if (e.key === "Escape") {
          (e.target as HTMLInputElement).value = value;
          (e.target as HTMLInputElement).blur();
        }
      }}
    />
  );
}

/** Thumbnail drawn from the shared artboard image cache, outside React. */
function ArtboardThumb({ id }: { id: ArtboardId }): ReactNode {
  const ref = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    const images = controller.images;
    const draw = () => {
      const c = ref.current;
      const src = images.get(id);
      if (!c) return;
      const g = c.getContext("2d")!;
      g.clearRect(0, 0, c.width, c.height);
      if (!src) return;
      const k = Math.min(c.width / src.width, c.height / src.height);
      const w = Math.max(1, Math.round(src.width * k));
      const h = Math.max(1, Math.round(src.height * k));
      g.imageSmoothingEnabled = k < 1;
      g.drawImage(src, (c.width - w) / 2, (c.height - h) / 2, w, h);
    };
    draw();
    return images.onChange((changed) => {
      if (changed === null || changed === id) draw();
    });
  }, [id]);
  return (
    <canvas
      ref={ref}
      width={56}
      height={56}
      className="checker pixelated h-7 w-7 shrink-0 rounded-[3px] border border-line"
    />
  );
}
