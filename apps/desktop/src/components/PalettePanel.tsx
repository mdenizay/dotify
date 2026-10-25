import { useEffect, useRef, useState, type ReactNode } from "react";
import { MoreHorizontal, Plus, Trash2 } from "lucide-react";
import { controller } from "../app/controller";
import { engine, errorMessage } from "../engine/bridge";
import type { LibraryPalette, Palette } from "../engine/types";
import { setEditor, toast, useEditor } from "../state/editor";
import { IconButton, inputClass, plainText } from "./ui/primitives";

export function PaletteActions() {
  const palette = useEditor((s) => s.session?.summary.palette);
  const primary = useEditor((s) => s.primary);
  if (!palette) return null;
  const index = palette.colors.indexOf(primary);
  const setColors = (colors: string[]) =>
    void controller.run({ type: "set_palette", palette: { ...palette, colors } });
  return (
    <>
      <IconButton
        label="Add primary color to palette"
        disabled={index >= 0 || palette.colors.length >= 256}
        onClick={() => setColors([...palette.colors, primary])}
      >
        <Plus size={13} />
      </IconButton>
      <IconButton
        label="Remove primary color from palette"
        disabled={index < 0}
        onClick={() => setColors(palette.colors.filter((_, i) => i !== index))}
      >
        <Trash2 size={12} />
      </IconButton>
    </>
  );
}

const samePalette = (a: Palette, b: Palette) =>
  a.name === b.name &&
  a.colors.length === b.colors.length &&
  a.colors.every((c, i) => c === b.colors[i]);

/** Pick, import, save and export palettes. Choosing one is an undoable edit. */
function PaletteLibrary({ palette }: { palette: Palette }) {
  const [library, setLibrary] = useState<LibraryPalette[]>([]);
  const [menuOpen, setMenuOpen] = useState(false);
  const [naming, setNaming] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    engine.paletteLibrary().then(setLibrary, (e) => toast(errorMessage(e), "error"));
  }, []);
  useEffect(() => {
    if (!menuOpen) return;
    const close = (e: PointerEvent) => {
      if (!menuRef.current?.contains(e.target as Node)) setMenuOpen(false);
    };
    window.addEventListener("pointerdown", close);
    return () => window.removeEventListener("pointerdown", close);
  }, [menuOpen]);

  const current = library.find((p) => samePalette(p, palette));
  const saved = library.find((p) => !p.builtin && p.name === palette.name);
  const apply = (p: Palette) =>
    void controller.run({ type: "set_palette", palette: { name: p.name, colors: p.colors } });
  const guard = async <T,>(fn: () => Promise<T>) => {
    setMenuOpen(false);
    try {
      return await fn();
    } catch (e) {
      toast(errorMessage(e), "error");
      return null;
    }
  };
  const importFile = () =>
    guard(async () => {
      const result = await engine.paletteImport();
      if (!result) return;
      setLibrary(result.library);
      apply(result.palette);
      toast(`Imported “${result.palette.name}” (${result.palette.colors.length} colors)`);
    });
  const saveAs = (name: string) =>
    guard(async () => {
      const next = { name, colors: palette.colors };
      setLibrary(await engine.paletteSave(next));
      if (name !== palette.name) apply(next);
      toast(`Saved “${name}” to your palettes`);
    });

  return (
    <div className="mb-1.5 flex items-center gap-1">
      {naming ? (
        <input
          {...plainText}
          autoFocus
          aria-label="Palette name"
          defaultValue={palette.name}
          className={`${inputClass} h-6`}
          onKeyDown={(e) => {
            const input = e.target as HTMLInputElement;
            if (e.key === "Escape") setNaming(false);
            if (e.key === "Enter" && input.value.trim()) {
              setNaming(false);
              void saveAs(input.value.trim());
            }
          }}
          onBlur={() => setNaming(false)}
        />
      ) : (
        <select
          aria-label="Palette"
          title="Switch palette"
          className="h-6 min-w-0 flex-1 rounded-[5px] border border-line-strong bg-panel px-1 text-[11px] text-fg outline-none focus:border-accent"
          value={current ? current.name : ""}
          onChange={(e) => {
            const p = library.find((x) => x.name === e.target.value);
            if (p) apply(p);
          }}
        >
          {!current && <option value="">{palette.name} (this project)</option>}
          <optgroup label="Built-in">
            {library
              .filter((p) => p.builtin)
              .map((p) => (
                <option key={p.name} value={p.name}>
                  {p.name} · {p.colors.length}
                </option>
              ))}
          </optgroup>
          {library.some((p) => !p.builtin) && (
            <optgroup label="My palettes">
              {library
                .filter((p) => !p.builtin)
                .map((p) => (
                  <option key={p.name} value={p.name}>
                    {p.name} · {p.colors.length}
                  </option>
                ))}
            </optgroup>
          )}
        </select>
      )}
      <div ref={menuRef} className="relative">
        <IconButton
          label="Palette options"
          active={menuOpen}
          onClick={() => setMenuOpen(!menuOpen)}
        >
          <MoreHorizontal size={14} />
        </IconButton>
        {menuOpen && (
          <div
            role="menu"
            className="absolute top-7 right-0 z-40 flex w-48 flex-col rounded-md border border-line-strong bg-raised py-1 shadow-xl shadow-black/30"
          >
            <MenuItem onClick={() => void importFile()}>
              Import palette file…
              <span className="text-faint">.gpl .hex .pal .png</span>
            </MenuItem>
            <MenuItem
              onClick={() => {
                setMenuOpen(false);
                setNaming(true);
              }}
            >
              Save as my palette…
            </MenuItem>
            <MenuItem
              onClick={() =>
                void guard(async () => {
                  const path = await engine.paletteExport(palette);
                  if (path) toast(`Exported ${path.split("/").pop()}`);
                })
              }
            >
              Export as .gpl…
            </MenuItem>
            {saved && (
              <MenuItem
                onClick={() =>
                  void guard(async () => {
                    setLibrary(await engine.paletteDelete(saved.name));
                    toast(`Removed “${saved.name}” from your palettes`);
                  })
                }
              >
                Remove “{saved.name}” from my palettes
              </MenuItem>
            )}
          </div>
        )}
      </div>
    </div>
  );
}

function MenuItem({ onClick, children }: { onClick: () => void; children: ReactNode }) {
  return (
    <button
      type="button"
      role="menuitem"
      onClick={onClick}
      className="flex items-center justify-between gap-2 px-3 py-1.5 text-left text-[12px] text-fg hover:bg-hover"
    >
      {children}
    </button>
  );
}

export function PalettePanel() {
  const palette = useEditor((s) => s.session?.summary.palette);
  const primary = useEditor((s) => s.primary);
  const secondary = useEditor((s) => s.secondary);
  if (!palette) return null;
  return (
    <div className="px-2 pb-2.5">
      <PaletteLibrary palette={palette} />
      <div
        role="listbox"
        aria-label="Palette"
        className="grid gap-[3px]"
        style={{ gridTemplateColumns: "repeat(auto-fill, minmax(18px, 1fr))" }}
      >
        {palette.colors.map((c, i) => {
          const isPrimary = c === primary;
          const isSecondary = c === secondary;
          return (
            <button
              key={`${c}-${i}`}
              type="button"
              role="option"
              aria-selected={isPrimary}
              aria-label={c}
              title={`${c} — click: primary, right click: secondary`}
              onClick={() => setEditor({ primary: c })}
              onContextMenu={(e) => {
                e.preventDefault();
                setEditor({ secondary: c });
              }}
              className={`checker relative aspect-square rounded-[3px] ${
                isPrimary
                  ? "ring-2 ring-fg ring-offset-1 ring-offset-panel"
                  : isSecondary
                    ? "ring-1 ring-dim"
                    : "hover:ring-1 hover:ring-line-strong"
              }`}
            >
              <span className="absolute inset-0 rounded-[3px]" style={{ background: c }} />
            </button>
          );
        })}
      </div>
    </div>
  );
}
