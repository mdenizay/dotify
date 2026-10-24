// Pixel rulers along the top and left edges of the canvas view. Labels are
// 1-based cell numbers ("column 1 of 48"), matching the status bar.

import { scaleOf, type Viewport } from "./viewport";

/** Ruler thickness in CSS pixels. */
export const RULER_SIZE = 20;

const STEPS = [1, 2, 4, 5, 8, 10, 16, 20, 25, 32, 50, 64, 100, 128, 200, 256, 500, 1000];

/**
 * Cells between labelled ticks so that labels are at least `minGap` device
 * pixels apart at `scale` device pixels per cell.
 */
export function rulerStep(scale: number, minGap: number): number {
  return STEPS.find((s) => s * scale >= minGap) ?? STEPS[STEPS.length - 1]!;
}

/** Cell numbers (1-based) that get a label: 1, every `step`, and the last. */
export function labelledCells(count: number, step: number): number[] {
  const out = [1];
  for (let c = step; c < count; c += step) if (c > 1) out.push(c);
  if (count > 1) out.push(count);
  return out;
}

export interface RulerColors {
  background: string;
  canvasRange: string;
  border: string;
  tick: string;
  text: string;
  accent: string;
  accentText: string;
}

interface Axis {
  /** Canvas origin along this axis, device px. */
  origin: number;
  /** Number of cells on this axis. */
  count: number;
  /** Visible length of the view along this axis, device px. */
  length: number;
  /** Hovered cell (0-based) or null. */
  hover: number | null;
}

export function drawRulers(
  ctx: CanvasRenderingContext2D,
  vp: Viewport,
  viewW: number,
  viewH: number,
  docW: number,
  docH: number,
  hover: { x: number; y: number } | null,
  colors: RulerColors,
) {
  const size = Math.round(RULER_SIZE * vp.dpr);
  const scale = scaleOf(vp);
  const inX = hover && hover.x >= 0 && hover.x < docW ? hover.x : null;
  const inY = hover && hover.y >= 0 && hover.y < docH ? hover.y : null;
  ctx.save();
  ctx.font = `${Math.round(10 * vp.dpr)}px ui-monospace, "SF Mono", Menlo, monospace`;
  ctx.textBaseline = "middle";
  drawAxis(
    ctx,
    "x",
    { origin: vp.ox, count: docW, length: viewW, hover: inX },
    size,
    scale,
    vp.dpr,
    colors,
  );
  drawAxis(
    ctx,
    "y",
    { origin: vp.oy, count: docH, length: viewH, hover: inY },
    size,
    scale,
    vp.dpr,
    colors,
  );
  // Corner: total canvas size.
  ctx.fillStyle = colors.background;
  ctx.fillRect(0, 0, size, size);
  ctx.fillStyle = colors.border;
  ctx.fillRect(size - 1, 0, 1, size);
  ctx.fillRect(0, size - 1, size, 1);
  ctx.restore();
}

function drawAxis(
  ctx: CanvasRenderingContext2D,
  axis: "x" | "y",
  a: Axis,
  size: number,
  scale: number,
  dpr: number,
  colors: RulerColors,
) {
  const horizontal = axis === "x";
  // Map (along, across) to canvas x/y so one code path draws both rulers.
  const rect = (along: number, across: number, len: number, thick: number) =>
    horizontal ? ctx.fillRect(along, across, len, thick) : ctx.fillRect(across, along, thick, len);
  const text = (label: string, along: number, align: "start" | "center") => {
    ctx.save();
    if (horizontal) {
      ctx.textAlign = align === "center" ? "center" : "left";
      ctx.fillText(label, along, size / 2);
    } else {
      ctx.translate(size / 2, along);
      ctx.rotate(-Math.PI / 2);
      ctx.textAlign = align === "center" ? "center" : "right";
      ctx.fillText(label, 0, 0);
    }
    ctx.restore();
  };

  ctx.fillStyle = colors.background;
  rect(0, 0, a.length, size);
  // The extent of the canvas is highlighted so its total size reads at a glance.
  const start = a.origin;
  const end = a.origin + a.count * scale;
  ctx.fillStyle = colors.canvasRange;
  rect(
    Math.max(size, start),
    0,
    Math.max(0, Math.min(a.length, end) - Math.max(size, start)),
    size,
  );
  ctx.fillStyle = colors.border;
  rect(0, size - 1, a.length, 1);

  const step = rulerStep(scale, 34 * dpr);
  const minor = scale >= 5 * dpr ? 1 : Math.max(1, step / 4);
  const first = Math.max(0, Math.floor((size - start) / scale));
  const last = Math.min(a.count, Math.ceil((a.length - start) / scale));

  ctx.fillStyle = colors.tick;
  for (let i = first; i <= last; i++) {
    if (i % minor !== 0 && i !== a.count) continue;
    const pos = start + i * scale;
    const major = i % step === 0 || i === 0 || i === a.count;
    const len = major ? Math.round(size * 0.45) : Math.round(size * 0.2);
    rect(pos, size - len, 1, len);
  }

  if (a.hover !== null) {
    ctx.fillStyle = colors.accent;
    rect(start + a.hover * scale, 0, Math.max(scale, 2), size);
  }

  const pad = Math.round(3 * dpr);
  ctx.fillStyle = colors.text;
  for (const cell of labelledCells(a.count, step)) {
    if (cell - 1 < first - step || cell - 1 > last) continue;
    if (a.hover !== null && Math.abs(cell - 1 - a.hover) * scale < 26 * dpr) continue;
    // Label the cell just right of (below) its leading edge.
    text(String(cell), start + (cell - 1) * scale + pad, "start");
  }

  if (a.hover !== null) {
    // Hovered cell number in an accent badge.
    const label = String(a.hover + 1);
    const w = ctx.measureText(label).width + pad * 2;
    const center = start + (a.hover + 0.5) * scale;
    ctx.fillStyle = colors.accent;
    rect(center - w / 2, Math.round(size * 0.12), w, Math.round(size * 0.76));
    ctx.fillStyle = colors.accentText;
    text(label, center, "center");
  }
}
