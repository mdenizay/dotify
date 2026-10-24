import { Plus, Trash2 } from "lucide-react";
import { controller } from "../app/controller";
import { setEditor, useEditor } from "../state/editor";
import { IconButton } from "./ui/primitives";

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

export function PalettePanel() {
  const palette = useEditor((s) => s.session?.summary.palette);
  const primary = useEditor((s) => s.primary);
  const secondary = useEditor((s) => s.secondary);
  if (!palette) return null;
  return (
    <div className="px-2 pb-2.5">
      <div className="mb-1.5 text-[11px] text-faint">{palette.name}</div>
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
