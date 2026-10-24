import { useState } from "react";
import { controller } from "../app/controller";
import { setEditor, useEditor } from "../state/editor";
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

export function ExportDialog() {
  const open = useEditor((s) => s.exportOpen);
  const scale = useEditor((s) => s.exportScale);
  const width = useEditor((s) => s.session?.summary.width ?? 0);
  const height = useEditor((s) => s.session?.summary.height ?? 0);
  if (!open) return null;
  const close = () => setEditor({ exportOpen: false });
  const submit = () => {
    close();
    void controller.exportPng(scale);
  };
  return (
    <Dialog title="Export PNG" onClose={close} width={320}>
      <div className="flex flex-col gap-3">
        <div className="text-dim">Current frame, visible layers, transparent background.</div>
        <div className="flex flex-wrap gap-1">
          {SCALES.map((s) => (
            <button
              key={s}
              type="button"
              onClick={() => setEditor({ exportScale: s })}
              className={`h-6 rounded-[5px] border px-2 font-mono text-[11px] ${
                scale === s
                  ? "border-accent bg-accent-soft text-fg"
                  : "border-line-strong text-dim hover:text-fg"
              }`}
            >
              {s}×
            </button>
          ))}
        </div>
        <div className="font-mono text-[11px] text-faint">
          {width * scale} × {height * scale} px · nearest neighbour
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
