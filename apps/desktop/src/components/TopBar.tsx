import {
  Download,
  FlipHorizontal2,
  FlipVertical2,
  Ghost,
  Grid3x3,
  Minus,
  Moon,
  Plus,
  Redo2,
  Ruler,
  Sun,
  Undo2,
} from "lucide-react";
import { controller } from "../app/controller";
import { setEditor, useEditor } from "../state/editor";
import { Divider, IconButton } from "./ui/primitives";

export function TopBar() {
  const fileName = useEditor((s) => s.session?.file_name ?? "");
  const dirty = useEditor((s) => s.session?.summary.dirty ?? false);
  const canUndo = useEditor((s) => s.session?.summary.can_undo ?? false);
  const canRedo = useEditor((s) => s.session?.summary.can_redo ?? false);
  const undoLabel = useEditor((s) => s.session?.summary.undo_label);
  const redoLabel = useEditor((s) => s.session?.summary.redo_label);
  const zoom = useEditor((s) => s.zoom);
  const showGrid = useEditor((s) => s.showGrid);
  const symX = useEditor((s) => s.symmetryX);
  const symY = useEditor((s) => s.symmetryY);
  const symMode = useEditor((s) => s.symmetryMode);
  const showRulers = useEditor((s) => s.showRulers);
  const theme = useEditor((s) => s.theme);

  return (
    <header
      data-tauri-drag-region
      className="flex h-[38px] shrink-0 items-center gap-1 border-b border-line bg-panel pr-2 pl-[84px]"
    >
      <div data-tauri-drag-region className="flex min-w-0 items-center gap-1.5 pr-2">
        <span data-tauri-drag-region className="truncate text-[12px] font-medium">
          {fileName}
        </span>
        {dirty && (
          <span title="Unsaved changes" className="h-1.5 w-1.5 shrink-0 rounded-full bg-dim" />
        )}
      </div>
      <Divider />
      <ToolOptions />
      <div data-tauri-drag-region className="h-full flex-1" />
      <IconButton
        label="Horizontal symmetry"
        active={symX}
        onClick={() => setEditor({ symmetryX: !symX })}
      >
        <FlipHorizontal2 size={15} strokeWidth={1.75} />
      </IconButton>
      <IconButton
        label="Vertical symmetry"
        active={symY}
        onClick={() => setEditor({ symmetryY: !symY })}
      >
        <FlipVertical2 size={15} strokeWidth={1.75} />
      </IconButton>
      <IconButton
        label="Mirror guide only — show the mirrored position without painting it"
        active={symMode === "guide"}
        disabled={!symX && !symY}
        onClick={() => setEditor({ symmetryMode: symMode === "guide" ? "paint" : "guide" })}
      >
        <Ghost size={15} strokeWidth={1.75} />
      </IconButton>
      <IconButton
        label="Rulers"
        shortcut="⇧⌘R"
        active={showRulers}
        onClick={() => setEditor({ showRulers: !showRulers })}
      >
        <Ruler size={15} strokeWidth={1.75} />
      </IconButton>
      <IconButton
        label="Pixel grid"
        shortcut="⌘'"
        active={showGrid}
        onClick={() => setEditor({ showGrid: !showGrid })}
      >
        <Grid3x3 size={15} strokeWidth={1.75} />
      </IconButton>
      <Divider />
      <IconButton label="Zoom out" shortcut="⌘−" onClick={() => controller.view?.zoomStep(-1)}>
        <Minus size={14} />
      </IconButton>
      <button
        type="button"
        title="Fit to window (⌘0)"
        onClick={() => controller.view?.fitToWindow()}
        className="h-6 w-12 rounded-[5px] text-center font-mono text-[11px] text-dim tabular-nums hover:bg-hover hover:text-fg"
      >
        {zoom * 100}%
      </button>
      <IconButton label="Zoom in" shortcut="⌘=" onClick={() => controller.view?.zoomStep(1)}>
        <Plus size={14} />
      </IconButton>
      <Divider />
      <IconButton
        label={undoLabel ? `Undo ${undoLabel}` : "Undo"}
        shortcut="⌘Z"
        disabled={!canUndo}
        onClick={() => void controller.undo()}
      >
        <Undo2 size={15} strokeWidth={1.75} />
      </IconButton>
      <IconButton
        label={redoLabel ? `Redo ${redoLabel}` : "Redo"}
        shortcut="⇧⌘Z"
        disabled={!canRedo}
        onClick={() => void controller.redo()}
      >
        <Redo2 size={15} strokeWidth={1.75} />
      </IconButton>
      <Divider />
      <IconButton label="Export" shortcut="⌘E" onClick={() => setEditor({ exportOpen: true })}>
        <Download size={15} strokeWidth={1.75} />
      </IconButton>
      <IconButton
        label={theme === "dark" ? "Light theme" : "Dark theme"}
        onClick={() => setEditor({ theme: theme === "dark" ? "light" : "dark" })}
      >
        {theme === "dark" ? (
          <Sun size={15} strokeWidth={1.75} />
        ) : (
          <Moon size={15} strokeWidth={1.75} />
        )}
      </IconButton>
    </header>
  );
}

function Toggle({
  label,
  value,
  onChange,
}: {
  label: string;
  value: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <label className="flex h-6 items-center gap-1.5 rounded-[5px] px-1.5 text-dim hover:text-fg">
      <input
        type="checkbox"
        checked={value}
        onChange={(e) => onChange(e.target.checked)}
        className="h-3 w-3 accent-[var(--accent)]"
      />
      {label}
    </label>
  );
}

function ToolOptions() {
  const tool = useEditor((s) => s.tool);
  const brushSize = useEditor((s) => s.brushSize);
  const filled = useEditor((s) => s.filledShapes);
  const contiguous = useEditor((s) => s.fillContiguous);

  if (tool === "pencil" || tool === "eraser") {
    return (
      <div className="flex items-center gap-1 pl-1 text-dim">
        <span className="pr-1">Size</span>
        <IconButton
          label="Smaller brush"
          shortcut="["
          disabled={brushSize <= 1}
          onClick={() => setEditor({ brushSize: brushSize - 1 })}
        >
          <Minus size={12} />
        </IconButton>
        <span className="w-6 text-center font-mono text-[11px] text-fg tabular-nums">
          {brushSize}
        </span>
        <IconButton
          label="Larger brush"
          shortcut="]"
          disabled={brushSize >= 32}
          onClick={() => setEditor({ brushSize: brushSize + 1 })}
        >
          <Plus size={12} />
        </IconButton>
      </div>
    );
  }
  if (tool === "rectangle" || tool === "ellipse") {
    return (
      <div className="flex items-center gap-2 pl-1">
        <Toggle label="Filled" value={filled} onChange={(v) => setEditor({ filledShapes: v })} />
        <span className="text-faint">Shift for square</span>
      </div>
    );
  }
  if (tool === "fill") {
    return (
      <div className="pl-1">
        <Toggle
          label="Contiguous"
          value={contiguous}
          onChange={(v) => setEditor({ fillContiguous: v })}
        />
      </div>
    );
  }
  const hints: Partial<Record<typeof tool, string>> = {
    line: "Click and drag to draw a line",
    eyedropper: "Left click picks primary, right click secondary",
    hand: "Drag to pan · Space also pans",
  };
  return <span className="pl-2 text-faint">{hints[tool]}</span>;
}
