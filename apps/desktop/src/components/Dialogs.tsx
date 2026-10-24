import { useState } from "react";
import { controller } from "../app/controller";
import { sheetSize } from "../app/sheetSize";
import { tagLanes } from "../app/tags";
import { selectFrames, selectTags, setEditor, useEditor, type ExportKind } from "../state/editor";
import { Button, Dialog, inputClass } from "./ui/primitives";

const PRESETS = [16, 24, 32, 48, 64, 128, 256];
const MAX = 4096;

export function NewDocumentDialog() {
  const open = useEditor((s) => s.newDocOpen);
  if (!open) return null;
  return <NewDocumentForm />;
}

function NewDocumentForm() {
  const [name, setName] = useState("Untitled");
  const [width, setWidth] = useState(32);
  const [height, setHeight] = useState(32);
  const valid = width >= 1 && height >= 1 && width <= MAX && height <= MAX;
  const close = () => setEditor({ newDocOpen: false });
  const submit = () => {
    if (!valid) return;
    close();
    void controller.newDocument(name, width, height);
  };
  const num = (v: string) => Math.max(0, Math.round(Number(v) || 0));

  return (
    <Dialog title="New Sprite" onClose={close}>
      <form
        className="flex flex-col gap-3"
        onSubmit={(e) => {
          e.preventDefault();
          submit();
        }}
      >
        <label className="flex flex-col gap-1 text-dim">
          Name
          <input className={inputClass} value={name} onChange={(e) => setName(e.target.value)} />
        </label>
        <div className="flex gap-2">
          <label className="flex flex-1 flex-col gap-1 text-dim">
            Width
            <input
              type="number"
              min={1}
              max={MAX}
              className={`${inputClass} font-mono`}
              value={width}
              onChange={(e) => setWidth(num(e.target.value))}
            />
          </label>
          <label className="flex flex-1 flex-col gap-1 text-dim">
            Height
            <input
              type="number"
              min={1}
              max={MAX}
              className={`${inputClass} font-mono`}
              value={height}
              onChange={(e) => setHeight(num(e.target.value))}
            />
          </label>
        </div>
        <div className="flex flex-wrap gap-1">
          {PRESETS.map((p) => (
            <button
              key={p}
              type="button"
              onClick={() => {
                setWidth(p);
                setHeight(p);
              }}
              className={`h-6 rounded-[5px] border px-2 font-mono text-[11px] ${
                width === p && height === p
                  ? "border-accent bg-accent-soft text-fg"
                  : "border-line-strong text-dim hover:text-fg"
              }`}
            >
              {p}²
            </button>
          ))}
        </div>
        {!valid && <p className="text-accent">Sizes must be between 1 and {MAX} pixels.</p>}
        <div className="flex justify-end gap-2 pt-1">
          <Button data-dismiss onClick={close}>
            Cancel
          </Button>
          <Button variant="primary" type="submit" disabled={!valid}>
            Create
          </Button>
        </div>
      </form>
    </Dialog>
  );
}

const SCALES = [1, 2, 4, 8, 10, 16];

const KINDS: Array<{ id: ExportKind; label: string }> = [
  { id: "png", label: "PNG" },
  { id: "sheet", label: "Spritesheet" },
  { id: "gif", label: "GIF" },
  { id: "godot", label: "Godot 4" },
];

const chipClass = (on: boolean) =>
  `h-6 rounded-[5px] border px-2 font-mono text-[11px] ${
    on ? "border-accent bg-accent-soft text-fg" : "border-line-strong text-dim hover:text-fg"
  }`;

export function ExportDialog() {
  const open = useEditor((s) => s.exportOpen);
  if (!open) return null;
  return <ExportForm />;
}

function ExportForm() {
  const kind = useEditor((s) => s.exportKind);
  const scale = useEditor((s) => s.exportScale);
  const sheet = useEditor((s) => s.exportSheet);
  const resDir = useEditor((s) => s.godotResDir);
  const width = useEditor((s) => s.session?.summary.width ?? 0);
  const height = useEditor((s) => s.session?.summary.height ?? 0);
  const frames = useEditor(selectFrames);
  const tags = useEditor(selectTags);
  const playTag = useEditor((s) => s.playTag);
  const [gifTag, setGifTag] = useState<string | null>(playTag);
  const close = () => setEditor({ exportOpen: false });
  const setSheet = (patch: Partial<typeof sheet>) =>
    setEditor({ exportSheet: { ...sheet, ...patch } });
  const num = (v: string, max: number) => Math.max(0, Math.min(max, Math.round(Number(v) || 0)));

  const submit = () => {
    close();
    if (kind === "png") void controller.exportPng(scale);
    else if (kind === "sheet") void controller.exportSheet();
    else if (kind === "gif") void controller.exportGif(gifTag);
    else void controller.exportGodot();
  };

  // Rows-per-tag row lengths, for the size readout.
  const spans = tagLanes(tags, frames);
  const tagged = new Set(spans.flatMap((s) => range(s.from, s.to)));
  const untagged = frames.length - tagged.size;
  const rows =
    sheet.layout === "rows_per_tag"
      ? [...spans.map((s) => s.to - s.from + 1), ...(untagged > 0 ? [untagged] : [])]
      : null;
  const size =
    kind === "sheet" || kind === "godot"
      ? sheetSize(width, height, frames.length, rows?.length ? rows : null, { ...sheet, scale })
      : { width: width * scale, height: height * scale };

  const description = {
    png: "Current frame, visible layers, transparent background.",
    sheet: "All frames packed into one PNG, plus a .json with frame rects, durations and tags.",
    gif: "Animated GIF with exact colors (up to 255 per frame). Partial transparency is rounded.",
    godot:
      "A spritesheet PNG and a SpriteFrames .tres with one animation per tag, ready for AnimatedSprite2D. Pick the destination folder inside your Godot project.",
  }[kind];

  const field = "flex items-center justify-between gap-2 text-dim";
  const numClass = `${inputClass.replace("w-full", "w-16")} text-right font-mono`;

  return (
    <Dialog title="Export" onClose={close} width={380}>
      <div className="flex flex-col gap-3">
        <div role="tablist" className="flex gap-1 rounded-md border border-line p-0.5">
          {KINDS.map((k) => (
            <button
              key={k.id}
              type="button"
              role="tab"
              aria-selected={kind === k.id}
              onClick={() => setEditor({ exportKind: k.id })}
              className={`h-6 flex-1 rounded-[4px] text-[11px] font-medium ${
                kind === k.id ? "bg-accent-soft text-fg" : "text-dim hover:text-fg"
              }`}
            >
              {k.label}
            </button>
          ))}
        </div>
        <div className="text-dim">{description}</div>
        <div className="flex flex-wrap gap-1">
          {SCALES.map((s) => (
            <button
              key={s}
              type="button"
              onClick={() => setEditor({ exportScale: s })}
              className={chipClass(scale === s)}
            >
              {s}×
            </button>
          ))}
        </div>
        {kind === "gif" && (
          <label className={field}>
            Frames
            <select
              className={inputClass.replace("w-full", "w-44")}
              value={gifTag ?? ""}
              onChange={(e) => setGifTag(e.target.value || null)}
            >
              <option value="">All frames (loop)</option>
              {tags.map((t) => (
                <option key={t.name} value={t.name}>
                  Tag: {t.name}
                </option>
              ))}
            </select>
          </label>
        )}
        {(kind === "sheet" || kind === "godot") && (
          <div className="flex flex-col gap-2">
            <div className="flex gap-1">
              <button
                type="button"
                className={chipClass(sheet.layout === "grid")}
                onClick={() => setSheet({ layout: "grid" })}
              >
                Grid
              </button>
              <button
                type="button"
                className={chipClass(sheet.layout === "rows_per_tag")}
                onClick={() => setSheet({ layout: "rows_per_tag" })}
              >
                Row per tag
              </button>
            </div>
            {sheet.layout === "grid" && (
              <label className={field}>
                Columns (0 = auto)
                <input
                  type="number"
                  min={0}
                  max={4096}
                  className={numClass}
                  value={sheet.columns ?? 0}
                  onChange={(e) => setSheet({ columns: num(e.target.value, 4096) || null })}
                />
              </label>
            )}
            <label className={field}>
              Padding
              <input
                type="number"
                min={0}
                max={64}
                className={numClass}
                value={sheet.padding}
                onChange={(e) => setSheet({ padding: num(e.target.value, 64) })}
              />
            </label>
            <label className={field}>
              Extrude edges
              <input
                type="number"
                min={0}
                max={64}
                className={numClass}
                value={sheet.extrude}
                onChange={(e) => setSheet({ extrude: num(e.target.value, 64) })}
              />
            </label>
          </div>
        )}
        {kind === "godot" && (
          <label className="flex flex-col gap-1 text-dim">
            Texture path in project (optional)
            <input
              className={`${inputClass} font-mono`}
              placeholder="res://sprites/hero"
              value={resDir}
              onChange={(e) => setEditor({ godotResDir: e.target.value })}
            />
            <span className="text-[11px] text-faint">
              Leave empty to reference the PNG next to the .tres.
            </span>
          </label>
        )}
        <div className="font-mono text-[11px] text-faint">
          {size.width} × {size.height} px · nearest neighbour
        </div>
        <div className="flex justify-end gap-2 pt-1">
          <Button data-dismiss onClick={close}>
            Cancel
          </Button>
          <Button variant="primary" onClick={submit}>
            Export…
          </Button>
        </div>
      </div>
    </Dialog>
  );
}

function range(a: number, b: number) {
  return Array.from({ length: b - a + 1 }, (_, i) => a + i);
}

export function RecoveryDialog() {
  const info = useEditor((s) => s.recovery);
  if (!info) return null;
  const when = new Date(info.saved_at).toLocaleString();
  return (
    <Dialog
      title="Recover Unsaved Work"
      onClose={() => void controller.discardRecovery()}
      width={380}
    >
      <div className="flex flex-col gap-3">
        <p className="text-dim">
          Dotify didn’t close cleanly. An autosaved copy of{" "}
          <span className="text-fg">{info.name}</span> from {when} is available.
        </p>
        <div className="flex justify-end gap-2">
          <Button data-dismiss onClick={() => void controller.discardRecovery()}>
            Discard
          </Button>
          <Button variant="primary" onClick={() => void controller.restoreRecovery()}>
            Restore
          </Button>
        </div>
      </div>
    </Dialog>
  );
}
