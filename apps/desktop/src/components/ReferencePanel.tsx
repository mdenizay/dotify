import { useEffect, useState } from "react";
import { Eye, EyeOff, ImagePlus, Layers2, Maximize, Move, X } from "lucide-react";
import { controller } from "../app/controller";
import { fitReference, referenceImages, rescaleAround } from "../canvas/reference";
import { setEditor, useEditor } from "../state/editor";
import { IconButton } from "./ui/primitives";

/** Header actions for the Reference section. */
export function ReferenceActions() {
  const has = useEditor((s) => s.reference !== null);
  return (
    <IconButton
      label={has ? "Replace reference image" : "Add reference image"}
      onClick={() => void controller.addReference()}
    >
      <ImagePlus size={13} />
    </IconButton>
  );
}

/** A view-only image behind or over the canvas, to trace or compare against. */
export function ReferencePanel() {
  const reference = useEditor((s) => s.reference);
  const moving = useEditor((s) => s.referenceMoving);
  const docW = useEditor((s) => s.session?.summary.width ?? 1);
  const docH = useEditor((s) => s.session?.summary.height ?? 1);
  const [, setLoaded] = useState(0);
  useEffect(() => referenceImages.onLoad(() => setLoaded((n) => n + 1)), []);

  if (!reference) {
    return (
      <div className="px-2 pb-2.5">
        <button
          type="button"
          onClick={() => void controller.addReference()}
          className="flex h-8 w-full items-center justify-center gap-1.5 rounded-[5px] border border-dashed border-line-strong text-dim hover:border-accent hover:text-fg"
        >
          <ImagePlus size={13} /> Add reference image…
        </button>
      </div>
    );
  }

  const img = referenceImages.get(reference.path);
  const set = (patch: Partial<typeof reference>) =>
    setEditor({ reference: { ...reference, ...patch } });
  const fit = img ? fitReference(img.naturalWidth, img.naturalHeight, docW, docH) : null;
  const sizePct = fit ? Math.round((reference.scale / fit.scale) * 100) : 100;
  const name = reference.path.split("/").pop();

  return (
    <div className="flex flex-col gap-2 px-2 pb-2.5">
      <div className="flex items-center gap-2">
        <div className="checker h-8 w-8 shrink-0 overflow-hidden rounded-[4px] border border-line">
          {img && <img src={img.src} alt="" className="h-full w-full object-cover" />}
        </div>
        <span className="min-w-0 flex-1 truncate text-dim" title={reference.path}>
          {name}
        </span>
        <IconButton
          label={reference.visible ? "Hide reference" : "Show reference"}
          onClick={() => set({ visible: !reference.visible })}
        >
          {reference.visible ? <Eye size={13} /> : <EyeOff size={13} className="text-faint" />}
        </IconButton>
        <IconButton
          label="Remove reference"
          onClick={() => setEditor({ reference: null, referenceMoving: false })}
        >
          <X size={13} />
        </IconButton>
      </div>
      <label className="flex items-center gap-2 text-dim">
        <span className="w-12">Opacity</span>
        <input
          type="range"
          className="slider flex-1"
          min={5}
          max={100}
          value={reference.opacity}
          onChange={(e) => set({ opacity: Number(e.target.value) })}
        />
        <span className="w-9 text-right font-mono text-[11px] text-fg tabular-nums">
          {reference.opacity}%
        </span>
      </label>
      <label className="flex items-center gap-2 text-dim">
        <span className="w-12">Size</span>
        <input
          type="range"
          className="slider flex-1"
          min={10}
          max={400}
          disabled={!fit}
          value={sizePct}
          onChange={(e) =>
            img &&
            fit &&
            set(
              rescaleAround(
                reference,
                img.naturalWidth,
                img.naturalHeight,
                (fit.scale * Number(e.target.value)) / 100,
              ),
            )
          }
        />
        <span className="w-9 text-right font-mono text-[11px] text-fg tabular-nums">
          {sizePct}%
        </span>
      </label>
      <div className="flex items-center gap-1">
        <IconButton
          label="Move reference: drag on the canvas"
          active={moving}
          onClick={() => setEditor({ referenceMoving: !moving })}
        >
          <Move size={13} />
        </IconButton>
        <IconButton label="Fit to canvas" disabled={!fit} onClick={() => fit && set(fit)}>
          <Maximize size={13} />
        </IconButton>
        <IconButton
          label={reference.onTop ? "Show behind the layers" : "Show over the layers"}
          active={reference.onTop}
          onClick={() => set({ onTop: !reference.onTop })}
        >
          <Layers2 size={13} />
        </IconButton>
        <span className="pl-1 text-[11px] text-faint">
          {moving
            ? "Drag on the canvas to move"
            : reference.onTop
              ? "Over layers"
              : "Behind layers"}
        </span>
      </div>
    </div>
  );
}
