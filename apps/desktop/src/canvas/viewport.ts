// Pure view-transform math, kept separate from DOM code so it can be tested.

export const ZOOM_LEVELS = [1, 2, 3, 4, 6, 8, 12, 16, 24, 32, 48, 64, 96, 128] as const;

export interface Viewport {
  /** CSS pixels per canvas pixel (always an integer). */
  zoom: number;
  /** Device pixels per CSS pixel. */
  dpr: number;
  /** Canvas origin in device pixels relative to the element (integers). */
  ox: number;
  oy: number;
}

/** Device pixels per canvas pixel. Rounded so pixel edges align on screen. */
export const scaleOf = (v: Viewport) => Math.max(1, Math.round(v.zoom * v.dpr));

/** Element CSS coordinates → canvas pixel coordinates (fractional). */
export function toCanvas(v: Viewport, cssX: number, cssY: number): { x: number; y: number } {
  const s = scaleOf(v);
  return { x: (cssX * v.dpr - v.ox) / s, y: (cssY * v.dpr - v.oy) / s };
}

export function nextZoom(zoom: number, dir: 1 | -1): number {
  if (dir > 0) return ZOOM_LEVELS.find((z) => z > zoom) ?? ZOOM_LEVELS[ZOOM_LEVELS.length - 1]!;
  return [...ZOOM_LEVELS].reverse().find((z) => z < zoom) ?? ZOOM_LEVELS[0];
}

/** Change zoom keeping the canvas point under (cssX, cssY) fixed. */
export function zoomAt(v: Viewport, zoom: number, cssX: number, cssY: number): Viewport {
  const p = toCanvas(v, cssX, cssY);
  const next = { ...v, zoom };
  const s = scaleOf(next);
  return {
    ...next,
    ox: Math.round(cssX * v.dpr - p.x * s),
    oy: Math.round(cssY * v.dpr - p.y * s),
  };
}

/** Largest zoom level that fits the document with some margin, centred. */
export function fit(
  docW: number,
  docH: number,
  cssW: number,
  cssH: number,
  dpr: number,
  margin = 48,
): Viewport {
  const avail = Math.max(1, Math.min((cssW - margin * 2) / docW, (cssH - margin * 2) / docH));
  const zoom = [...ZOOM_LEVELS].reverse().find((z) => z <= avail) ?? 1;
  return center({ zoom, dpr, ox: 0, oy: 0 }, docW, docH, cssW, cssH);
}

export function center(
  v: Viewport,
  docW: number,
  docH: number,
  cssW: number,
  cssH: number,
): Viewport {
  const s = scaleOf(v);
  return {
    ...v,
    ox: Math.round((cssW * v.dpr - docW * s) / 2),
    oy: Math.round((cssH * v.dpr - docH * s) / 2),
  };
}
