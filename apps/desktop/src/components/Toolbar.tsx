import {
  Circle,
  Eraser,
  Hand,
  PaintBucket,
  Pencil,
  Pipette,
  Slash,
  Square,
  ArrowLeftRight,
  type LucideIcon,
} from "lucide-react";
import { TOOL_SHORTCUTS } from "../app/shortcuts";
import { setEditor, useEditor, type ToolId } from "../state/editor";
import { IconButton } from "./ui/primitives";

const TOOLS: Array<{ id: ToolId; label: string; icon: LucideIcon }> = [
  { id: "pencil", label: "Pencil", icon: Pencil },
  { id: "eraser", label: "Eraser", icon: Eraser },
  { id: "fill", label: "Bucket Fill", icon: PaintBucket },
  { id: "line", label: "Line", icon: Slash },
  { id: "rectangle", label: "Rectangle", icon: Square },
  { id: "ellipse", label: "Ellipse", icon: Circle },
  { id: "eyedropper", label: "Eyedropper", icon: Pipette },
  { id: "hand", label: "Hand", icon: Hand },
];

export function Toolbar() {
  const tool = useEditor((s) => s.tool);
  return (
    <nav
      aria-label="Tools"
      className="flex w-11 shrink-0 flex-col items-center gap-0.5 border-r border-line bg-panel py-2"
    >
      {TOOLS.map(({ id, label, icon: Icon }) => (
        <IconButton
          key={id}
          size="md"
          label={label}
          shortcut={TOOL_SHORTCUTS[id]}
          active={tool === id}
          onClick={() => setEditor({ tool: id })}
        >
          <Icon size={16} strokeWidth={1.75} />
        </IconButton>
      ))}
      <div className="flex-1" />
      <ColorSlots />
    </nav>
  );
}

function ColorSlots() {
  const primary = useEditor((s) => s.primary);
  const secondary = useEditor((s) => s.secondary);
  return (
    <div className="relative mb-1 h-[46px] w-[34px]" aria-label="Colors">
      <div
        title="Secondary color (right click)"
        className="checker absolute right-0 bottom-0 h-[22px] w-[22px] rounded-[4px] border border-line-strong"
      >
        <div className="h-full w-full rounded-[3px]" style={{ background: secondary }} />
      </div>
      <div
        title="Primary color (left click)"
        className="checker absolute top-0 left-0 h-[22px] w-[22px] rounded-[4px] border border-line-strong shadow-[0_0_0_2px_var(--panel)]"
      >
        <div className="h-full w-full rounded-[3px]" style={{ background: primary }} />
      </div>
      <button
        type="button"
        aria-label="Swap colors (X)"
        title="Swap colors (X)"
        onClick={() => setEditor({ primary: secondary, secondary: primary })}
        className="absolute bottom-0 left-0 text-faint hover:text-fg"
      >
        <ArrowLeftRight size={11} />
      </button>
    </div>
  );
}
