import { useEffect, useRef, useState, type PointerEvent as RPointerEvent } from "react";
import { hsvaToRgba, normalizeHex, rgba, rgbaToHsva, toHex, type HSVA } from "../canvas/color";
import { setEditor, useEditor } from "../state/editor";
import { inputClass, plainText } from "./ui/primitives";

/** HSV picker bound to the primary color. Keeps hue stable for greys. */
export function ColorPicker() {
  const primary = useEditor((s) => s.primary);
  const [hsva, setHsva] = useState<HSVA>(() => rgbaToHsva(rgba(primary)));
  const [hexText, setHexText] = useState(primary);
  const lastEmitted = useRef(primary);

  // Follow external changes (palette clicks, eyedropper) without losing hue.
  useEffect(() => {
    if (primary === lastEmitted.current) return;
    lastEmitted.current = primary;
    const next = rgbaToHsva(rgba(primary));
    setHsva((prev) => (next.s === 0 || next.v === 0 ? { ...next, h: prev.h } : next));
    setHexText(primary);
  }, [primary]);

  const commit = (next: HSVA) => {
    setHsva(next);
    const hex = toHex(hsvaToRgba(next));
    lastEmitted.current = hex;
    setHexText(hex);
    setEditor({ primary: hex });
  };

  const hueColor = toHex(hsvaToRgba({ h: hsva.h, s: 1, v: 1, a: 255 }));
  const opaque = toHex(hsvaToRgba({ ...hsva, a: 255 }));

  return (
    <div className="flex flex-col gap-2 px-2 pb-2.5">
      <div
        role="slider"
        aria-label="Saturation and value"
        aria-valuetext={`saturation ${Math.round(hsva.s * 100)}%, value ${Math.round(hsva.v * 100)}%`}
        tabIndex={0}
        className="relative h-[120px] cursor-crosshair rounded-[5px]"
        style={{
          background: `linear-gradient(to top, #000, transparent), linear-gradient(to right, #fff, ${hueColor})`,
        }}
        onPointerDown={(e) => trackDrag(e, (fx, fy) => commit({ ...hsva, s: fx, v: 1 - fy }))}
      >
        <div
          className="pointer-events-none absolute h-2.5 w-2.5 -translate-x-1/2 -translate-y-1/2 rounded-full border-2 border-white shadow-[0_0_0_1px_rgba(0,0,0,0.5)]"
          style={{ left: `${hsva.s * 100}%`, top: `${(1 - hsva.v) * 100}%` }}
        />
      </div>
      <Bar
        label="Hue"
        value={hsva.h / 360}
        background="linear-gradient(to right,#f00,#ff0,#0f0,#0ff,#00f,#f0f,#f00)"
        onDrag={(fx) => commit({ ...hsva, h: Math.min(359.9, fx * 360) })}
      />
      <Bar
        label="Alpha"
        checker
        value={hsva.a / 255}
        background={`linear-gradient(to right, transparent, ${opaque})`}
        onDrag={(fx) => commit({ ...hsva, a: Math.round(fx * 255) })}
      />
      <div className="flex items-center gap-1.5">
        <div className="checker h-7 w-7 shrink-0 rounded-[5px] border border-line-strong">
          <div className="h-full w-full rounded-[4px]" style={{ background: primary }} />
        </div>
        <input
          {...plainText}
          aria-label="Hex color"
          className={`${inputClass} font-mono`}
          value={hexText}
          onChange={(e) => setHexText(e.target.value)}
          onBlur={() => setHexText(primary)}
          onKeyDown={(e) => {
            if (e.key !== "Enter") return;
            const hex = normalizeHex(hexText);
            if (hex) {
              lastEmitted.current = "";
              setEditor({ primary: hex });
            }
            (e.target as HTMLInputElement).blur();
          }}
        />
        <span className="w-9 shrink-0 text-right font-mono text-[11px] text-dim tabular-nums">
          {Math.round((hsva.a / 255) * 100)}%
        </span>
      </div>
    </div>
  );
}

function Bar({
  label,
  value,
  background,
  checker,
  onDrag,
}: {
  label: string;
  value: number;
  background: string;
  checker?: boolean;
  onDrag: (fx: number) => void;
}) {
  return (
    <div
      role="slider"
      aria-label={label}
      aria-valuenow={Math.round(value * 100)}
      tabIndex={0}
      className={`relative h-3 cursor-pointer rounded-full ${checker ? "checker" : ""}`}
      onPointerDown={(e) => trackDrag(e, onDrag)}
    >
      <div className="absolute inset-0 rounded-full" style={{ background }} />
      <div
        className="pointer-events-none absolute top-1/2 h-3.5 w-1.5 -translate-x-1/2 -translate-y-1/2 rounded-[2px] border border-black/40 bg-white"
        style={{ left: `${value * 100}%` }}
      />
    </div>
  );
}

/** Report normalised pointer positions while dragging inside an element. */
function trackDrag(e: RPointerEvent<HTMLDivElement>, fn: (fx: number, fy: number) => void) {
  const el = e.currentTarget;
  el.setPointerCapture(e.pointerId);
  const update = (ev: { clientX: number; clientY: number }) => {
    const r = el.getBoundingClientRect();
    fn(
      Math.min(1, Math.max(0, (ev.clientX - r.left) / r.width)),
      Math.min(1, Math.max(0, (ev.clientY - r.top) / r.height)),
    );
  };
  update(e);
  const move = (ev: PointerEvent) => update(ev);
  const up = () => {
    el.removeEventListener("pointermove", move);
    el.removeEventListener("pointerup", up);
  };
  el.addEventListener("pointermove", move);
  el.addEventListener("pointerup", up);
}
