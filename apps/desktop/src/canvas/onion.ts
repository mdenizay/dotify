export interface OnionSkin {
  enabled: boolean;
  /** Frames shown before the active one. */
  prev: number;
  /** Frames shown after the active one. */
  next: number;
  /** Opacity of the nearest frame, 0–100. */
  opacity: number;
  /** Recolor previous frames red and next frames blue. */
  tint: boolean;
}

export const ONION_TINTS = { prev: "#ff4f5e", next: "#3f8cff" } as const;
export const MAX_ONION_FRAMES = 8;

export interface OnionFrame {
  index: number;
  side: "prev" | "next";
  /** 0–1 */
  alpha: number;
}

/**
 * Neighbouring frames to draw under frame `active` of `count`, farthest
 * first. Opacity fades linearly with distance; the timeline does not wrap.
 */
export function onionFrames(active: number, count: number, o: OnionSkin): OnionFrame[] {
  if (!o.enabled) return [];
  const out: OnionFrame[] = [];
  const fade = (d: number, n: number) => ((o.opacity / 100) * (n - d + 1)) / n;
  for (let d = o.prev; d >= 1; d--) {
    if (active - d >= 0) out.push({ index: active - d, side: "prev", alpha: fade(d, o.prev) });
  }
  for (let d = o.next; d >= 1; d--) {
    if (active + d < count) out.push({ index: active + d, side: "next", alpha: fade(d, o.next) });
  }
  return out;
}
