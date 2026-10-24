import { ChevronDown, ChevronLeft, ChevronRight, Copy, Plus, Trash2 } from "lucide-react";
import { controller } from "../app/controller";
import { selectFrames, setEditor, useEditor } from "../state/editor";
import { Divider, IconButton } from "./ui/primitives";

/** Frame strip. Playback, onion skinning and tags arrive in Phase 2. */
export function Timeline() {
  const open = useEditor((s) => s.timelineOpen);
  const frames = useEditor(selectFrames);
  const activeId = useEditor((s) => s.activeFrameId);
  const index = frames.findIndex((f) => f.id === activeId);
  const active = frames[index];
  const run = controller.run;

  return (
    <section aria-label="Timeline" className="shrink-0 border-t border-line bg-panel">
      <header className="flex h-8 items-center gap-1 px-2">
        <button
          type="button"
          aria-expanded={open}
          onClick={() => setEditor({ timelineOpen: !open })}
          className="flex items-center gap-1 text-[11px] font-semibold tracking-wide text-dim uppercase hover:text-fg"
        >
          <ChevronDown size={12} className={`transition-transform ${open ? "" : "-rotate-90"}`} />
          Frames
        </button>
        <span className="pl-1 font-mono text-[11px] text-faint tabular-nums">
          {index + 1}/{frames.length}
        </span>
        <div className="flex-1" />
        {active && (
          <label className="flex items-center gap-1.5 pr-1 text-dim">
            Duration
            <input
              key={`${active.id}-${active.duration_ms}`}
              type="number"
              min={1}
              max={60000}
              defaultValue={active.duration_ms}
              aria-label="Frame duration in milliseconds"
              className="h-6 w-16 rounded-[5px] border border-line-strong bg-panel px-1.5 text-right font-mono text-[11px] text-fg outline-none focus:border-accent"
              onKeyDown={(e) => e.key === "Enter" && (e.target as HTMLInputElement).blur()}
              onBlur={(e) => {
                const v = Math.round(Number(e.target.value));
                if (v >= 1 && v <= 60000 && v !== active.duration_ms) {
                  void run({ type: "set_frame_duration", frame: active.id, duration_ms: v });
                } else e.target.value = String(active.duration_ms);
              }}
            />
            <span className="text-faint">ms</span>
          </label>
        )}
        <Divider />
        <IconButton
          label="Move frame left"
          disabled={index <= 0}
          onClick={() =>
            active && void run({ type: "move_frame", frame: active.id, index: index - 1 })
          }
        >
          <ChevronLeft size={14} />
        </IconButton>
        <IconButton
          label="Move frame right"
          disabled={index < 0 || index >= frames.length - 1}
          onClick={() =>
            active && void run({ type: "move_frame", frame: active.id, index: index + 1 })
          }
        >
          <ChevronRight size={14} />
        </IconButton>
        <IconButton
          label="New frame"
          onClick={() => void run({ type: "add_frame", index: index + 1 })}
        >
          <Plus size={13} />
        </IconButton>
        <IconButton
          label="Duplicate frame"
          disabled={!active}
          onClick={() => active && void run({ type: "duplicate_frame", frame: active.id })}
        >
          <Copy size={12} />
        </IconButton>
        <IconButton
          label="Delete frame"
          disabled={!active || frames.length <= 1}
          onClick={() => active && void run({ type: "delete_frame", frame: active.id })}
        >
          <Trash2 size={12} />
        </IconButton>
      </header>
      {open && (
        <div
          role="listbox"
          aria-label="Frames"
          aria-orientation="horizontal"
          className="flex h-11 items-center gap-1 overflow-x-auto px-2 pb-2"
        >
          {frames.map((f, i) => (
            <button
              key={f.id}
              type="button"
              role="option"
              aria-selected={f.id === activeId}
              title={`Frame ${i + 1} · ${f.duration_ms} ms (, and . to step)`}
              onClick={() => void controller.selectFrame(f.id)}
              className={`flex h-8 min-w-8 shrink-0 items-center justify-center rounded-[5px] border px-2 font-mono text-[11px] tabular-nums ${
                f.id === activeId
                  ? "border-accent bg-accent-soft text-fg"
                  : "border-line text-dim hover:border-line-strong hover:text-fg"
              }`}
            >
              {i + 1}
            </button>
          ))}
        </div>
      )}
    </section>
  );
}
