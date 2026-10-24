import { useEffect, useRef, useState } from "react";
import { ArrowDown, ArrowUp, Copy, Eye, EyeOff, Lock, LockOpen, Plus, Trash2 } from "lucide-react";
import { controller } from "../app/controller";
import type { Layer, LayerId } from "../engine/types";
import { activeLayer, selectLayers, setEditor, useEditor } from "../state/editor";
import { IconButton, inputClass } from "./ui/primitives";

export function LayerActions() {
  const layers = useEditor(selectLayers);
  const active = useEditor(activeLayer);
  const index = active ? layers.findIndex((l) => l.id === active.id) : -1;
  const run = controller.run;
  return (
    <>
      <IconButton
        label="New layer"
        shortcut="⇧N"
        onClick={() => void run({ type: "create_layer", index: index + 1 })}
      >
        <Plus size={13} />
      </IconButton>
      <IconButton
        label="Duplicate layer"
        disabled={!active}
        onClick={() => active && void run({ type: "duplicate_layer", layer: active.id })}
      >
        <Copy size={12} />
      </IconButton>
      <IconButton
        label="Move layer up"
        disabled={!active || index >= layers.length - 1}
        onClick={() =>
          active && void run({ type: "move_layer", layer: active.id, index: index + 1 })
        }
      >
        <ArrowUp size={13} />
      </IconButton>
      <IconButton
        label="Move layer down"
        disabled={!active || index <= 0}
        onClick={() =>
          active && void run({ type: "move_layer", layer: active.id, index: index - 1 })
        }
      >
        <ArrowDown size={13} />
      </IconButton>
      <IconButton
        label="Delete layer"
        disabled={!active || layers.length <= 1}
        onClick={() => active && void run({ type: "delete_layer", layer: active.id })}
      >
        <Trash2 size={12} />
      </IconButton>
    </>
  );
}

export function LayersPanel() {
  const layers = useEditor(selectLayers);
  const activeId = useEditor((s) => s.activeLayerId);
  return (
    <div className="flex h-full min-h-0 flex-col">
      <ul role="listbox" aria-label="Layers" className="min-h-0 flex-1 overflow-y-auto px-1 pb-1">
        {[...layers].reverse().map((l) => (
          <LayerRow key={l.id} layer={l} active={l.id === activeId} />
        ))}
      </ul>
      <OpacityControl />
    </div>
  );
}

function LayerRow({ layer, active }: { layer: Layer; active: boolean }) {
  const [editing, setEditing] = useState(false);
  const update = (patch: { name?: string; visible?: boolean; locked?: boolean }) =>
    void controller.run({ type: "update_layer", layer: layer.id, ...patch });
  return (
    <li
      role="option"
      aria-selected={active}
      onPointerDown={() => setEditor({ activeLayerId: layer.id })}
      className={`group flex h-8 items-center gap-1 rounded-[5px] pr-1 pl-0.5 ${
        active ? "bg-accent-soft" : "hover:bg-hover"
      }`}
    >
      <IconButton
        label={layer.visible ? "Hide layer" : "Show layer"}
        onClick={() => update({ visible: !layer.visible })}
      >
        {layer.visible ? <Eye size={13} /> : <EyeOff size={13} className="text-faint" />}
      </IconButton>
      <LayerThumb id={layer.id} />
      {editing ? (
        <input
          autoFocus
          aria-label="Layer name"
          defaultValue={layer.name}
          className={`${inputClass} h-6`}
          onBlur={(e) => {
            setEditing(false);
            if (e.target.value.trim() && e.target.value !== layer.name)
              update({ name: e.target.value });
          }}
          onKeyDown={(e) => {
            if (e.key === "Enter") (e.target as HTMLInputElement).blur();
            if (e.key === "Escape") {
              (e.target as HTMLInputElement).value = layer.name;
              (e.target as HTMLInputElement).blur();
            }
          }}
        />
      ) : (
        <span
          onDoubleClick={() => setEditing(true)}
          title="Double-click to rename"
          className={`min-w-0 flex-1 truncate pl-1 ${layer.visible ? "" : "text-faint"} ${
            active ? "text-fg" : "text-dim"
          }`}
        >
          {layer.name}
        </span>
      )}
      {layer.opacity < 255 && (
        <span className="font-mono text-[10px] text-faint tabular-nums">
          {Math.round((layer.opacity / 255) * 100)}%
        </span>
      )}
      <IconButton
        label={layer.locked ? "Unlock layer" : "Lock layer"}
        onClick={() => update({ locked: !layer.locked })}
        className={
          layer.locked ? "" : "opacity-0 group-hover:opacity-100 focus-visible:opacity-100"
        }
      >
        {layer.locked ? <Lock size={12} /> : <LockOpen size={12} />}
      </IconButton>
    </li>
  );
}

/** Small preview drawn straight from the render cache, outside React. */
function LayerThumb({ id }: { id: LayerId }) {
  const ref = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    const cache = controller.cache;
    let raf = 0;
    const draw = () => {
      raf = 0;
      const c = ref.current;
      if (!c || !cache.has(id)) return;
      const g = c.getContext("2d")!;
      const src = cache.surface(id).canvas;
      const k = Math.min(c.width / src.width, c.height / src.height);
      const w = Math.max(1, Math.round(src.width * k)),
        h = Math.max(1, Math.round(src.height * k));
      g.clearRect(0, 0, c.width, c.height);
      g.imageSmoothingEnabled = k < 1;
      g.drawImage(src, (c.width - w) / 2, (c.height - h) / 2, w, h);
    };
    draw();
    const off = cache.onChange((layer) => {
      if ((layer === null || layer === id) && !raf) raf = requestAnimationFrame(draw);
    });
    return () => {
      off();
      cancelAnimationFrame(raf);
    };
  }, [id]);
  return (
    <canvas
      ref={ref}
      width={44}
      height={44}
      className="checker pixelated h-[22px] w-[22px] shrink-0 rounded-[3px] border border-line"
    />
  );
}

function OpacityControl() {
  const layer = useEditor(activeLayer);
  const preview = useEditor((s) => s.opacityPreview);
  if (!layer) return null;
  const value = preview?.layer === layer.id ? preview.value : layer.opacity;
  const commit = () => {
    const p = useEditor.getState().opacityPreview;
    if (!p) return;
    void controller
      .run({ type: "update_layer", layer: p.layer, opacity: p.value })
      .finally(() => setEditor({ opacityPreview: null }));
  };
  return (
    <label className="flex h-8 shrink-0 items-center gap-2 border-t border-line px-2.5 text-dim">
      Opacity
      <input
        type="range"
        className="slider flex-1"
        min={0}
        max={255}
        value={value}
        onChange={(e) =>
          setEditor({ opacityPreview: { layer: layer.id, value: Number(e.target.value) } })
        }
        onPointerUp={commit}
        onKeyUp={commit}
      />
      <span className="w-8 text-right font-mono text-[11px] text-fg tabular-nums">
        {Math.round((value / 255) * 100)}%
      </span>
    </label>
  );
}
