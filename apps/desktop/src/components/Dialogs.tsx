import { useState } from "react";
import { controller } from "../app/controller";
import type { FolderId } from "../engine/types";
import { getEditor, selectFolders, setEditor, useEditor } from "../state/editor";
import { Button, Dialog, inputClass } from "./ui/primitives";

const PRESETS = [16, 24, 32, 48, 64, 128, 256];
const MAX = 4096;

export function NewDocumentDialog() {
  const open = useEditor((s) => s.newDocOpen);
  if (!open) return null;
  return (
    <SpriteForm
      title="New Project"
      initialName="Untitled"
      submitLabel="Create"
      onClose={() => setEditor({ newDocOpen: false })}
      onSubmit={(name, width, height) => void controller.newDocument(name, width, height)}
    />
  );
}

/** Adds an artboard to the project, sized like the active one by default. */
export function NewArtboardDialog() {
  const open = useEditor((s) => s.newArtboardOpen);
  if (!open) return null;
  const s = getEditor();
  const active = s.session?.project.artboards.find((a) => a.id === s.session?.active_artboard);
  return (
    <SpriteForm
      title="New Artboard"
      initialName=""
      namePlaceholder="Sprite name"
      initialSize={active ? { width: active.width, height: active.height } : undefined}
      initialFolder={active?.folder ?? null}
      withFolder
      submitLabel="Add"
      onClose={() => setEditor({ newArtboardOpen: false })}
      onSubmit={(name, width, height, folder) =>
        void controller.runProject({
          type: "create_artboard",
          name: name.trim() || null,
          width,
          height,
          folder,
        })
      }
    />
  );
}

function SpriteForm({
  title,
  initialName,
  namePlaceholder,
  initialSize = { width: 32, height: 32 },
  initialFolder = null,
  withFolder,
  submitLabel,
  onClose,
  onSubmit,
}: {
  title: string;
  initialName: string;
  namePlaceholder?: string;
  initialSize?: { width: number; height: number };
  initialFolder?: FolderId | null;
  withFolder?: boolean;
  submitLabel: string;
  onClose: () => void;
  onSubmit: (name: string, width: number, height: number, folder: FolderId | null) => void;
}) {
  const folders = useEditor(selectFolders);
  const [name, setName] = useState(initialName);
  const [width, setWidth] = useState(initialSize.width);
  const [height, setHeight] = useState(initialSize.height);
  const [folder, setFolder] = useState<FolderId | null>(initialFolder);
  const valid = width >= 1 && height >= 1 && width <= MAX && height <= MAX;
  const close = onClose;
  const submit = () => {
    if (!valid) return;
    close();
    onSubmit(name, width, height, folder);
  };
  const num = (v: string) => Math.max(0, Math.round(Number(v) || 0));

  return (
    <Dialog title={title} onClose={close}>
      <form
        className="flex flex-col gap-3"
        onSubmit={(e) => {
          e.preventDefault();
          submit();
        }}
      >
        <label className="flex flex-col gap-1 text-dim">
          Name
          <input
            className={inputClass}
            value={name}
            placeholder={namePlaceholder}
            onChange={(e) => setName(e.target.value)}
          />
        </label>
        {withFolder && folders.length > 0 && (
          <label className="flex flex-col gap-1 text-dim">
            Folder
            <select
              className={inputClass}
              value={folder ?? ""}
              onChange={(e) => setFolder(e.target.value === "" ? null : Number(e.target.value))}
            >
              <option value="">No folder</option>
              {folders.map((f) => (
                <option key={f.id} value={f.id}>
                  {f.name}
                </option>
              ))}
            </select>
          </label>
        )}
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
            {submitLabel}
          </Button>
        </div>
      </form>
    </Dialog>
  );
}

const SCALES = [1, 2, 4, 8, 10, 16];

export function ExportDialog() {
  const single = useEditor((s) => s.exportOpen);
  const all = useEditor((s) => s.exportAllOpen);
  const scale = useEditor((s) => s.exportScale);
  const width = useEditor((s) => s.session?.summary.width ?? 0);
  const height = useEditor((s) => s.session?.summary.height ?? 0);
  const count = useEditor((s) => s.session?.project.artboards.length ?? 0);
  if (!single && !all) return null;
  const close = () => setEditor({ exportOpen: false, exportAllOpen: false });
  const submit = () => {
    close();
    void (all ? controller.exportAll(scale) : controller.exportPng(scale));
  };
  return (
    <Dialog title={all ? "Export All Artboards" : "Export PNG"} onClose={close} width={320}>
      <div className="flex flex-col gap-3">
        <div className="text-dim">
          {all
            ? `First frame of each of the ${count} artboards as PNG, one sub-folder per library folder.`
            : "Current frame, visible layers, transparent background."}
        </div>
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
          {all ? `${scale}× each` : `${width * scale} × ${height * scale} px`} · nearest neighbour
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
