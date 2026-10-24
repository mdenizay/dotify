// Preview rasterisers. Exact mirrors of crates/dotify-core/src/raster.rs so
// live previews match what the engine commits. Keep both in sync.

export type Plot = (x: number, y: number) => void;

export function line(x0: number, y0: number, x1: number, y1: number, plot: Plot): void {
  let x = x0,
    y = y0;
  const dx = Math.abs(x1 - x0);
  const dy = -Math.abs(y1 - y0);
  const sx = x0 < x1 ? 1 : -1;
  const sy = y0 < y1 ? 1 : -1;
  let err = dx + dy;
  for (;;) {
    plot(x, y);
    if (x === x1 && y === y1) break;
    const e2 = 2 * err;
    if (e2 >= dy) {
      err += dy;
      x += sx;
    }
    if (e2 <= dx) {
      err += dx;
      y += sy;
    }
  }
}

export function rectangle(
  x: number,
  y: number,
  w: number,
  h: number,
  filled: boolean,
  plot: Plot,
): void {
  for (let j = 0; j < h; j++) {
    for (let i = 0; i < w; i++) {
      if (filled || i === 0 || j === 0 || i === w - 1 || j === h - 1) plot(x + i, y + j);
    }
  }
}

function ellipseSpan(major: number, minor: number, j: number): [number, number] | null {
  const halfMajor = major / 2;
  const t = (2 * j + 1 - minor) / minor;
  const half = halfMajor * Math.sqrt(Math.max(0, 1 - t * t));
  // Values are non-negative, where Math.round matches Rust's f64::round.
  const left = Math.round(halfMajor - half);
  const right = Math.round(halfMajor + half) - 1;
  return right >= left ? [left, right] : null;
}

export function ellipse(
  x: number,
  y: number,
  w: number,
  h: number,
  filled: boolean,
  plot: Plot,
): void {
  if (w <= 0 || h <= 0) return;
  const mask = new Uint8Array(w * h);
  for (let j = 0; j < h; j++) {
    const s = ellipseSpan(w, h, j);
    if (!s) continue;
    if (filled) for (let i = s[0]; i <= s[1]; i++) mask[j * w + i] = 1;
    else mask[j * w + s[0]] = mask[j * w + s[1]] = 1;
  }
  for (let i = 0; i < w; i++) {
    const s = ellipseSpan(h, w, i);
    if (!s) continue;
    if (filled) for (let j = s[0]; j <= s[1]; j++) mask[j * w + i] = 1;
    else mask[s[0] * w + i] = mask[s[1] * w + i] = 1;
  }
  for (let j = 0; j < h; j++) {
    for (let i = 0; i < w; i++) if (mask[j * w + i]) plot(x + i, y + j);
  }
}

/** Normalised box from two corner points (inclusive). */
export function boxFrom(
  ax: number,
  ay: number,
  bx: number,
  by: number,
  square: boolean,
): { x: number; y: number; width: number; height: number } {
  let dx = bx - ax,
    dy = by - ay;
  if (square) {
    const m = Math.max(Math.abs(dx), Math.abs(dy));
    dx = Math.sign(dx || 1) * m;
    dy = Math.sign(dy || 1) * m;
  }
  const x = Math.min(ax, ax + dx);
  const y = Math.min(ay, ay + dy);
  return { x, y, width: Math.abs(dx) + 1, height: Math.abs(dy) + 1 };
}

/** Offsets of a square brush of `size` pixels, centred on the cursor. */
export function brushOffsets(size: number): Array<[number, number]> {
  const start = -Math.floor((size - 1) / 2);
  const out: Array<[number, number]> = [];
  for (let j = 0; j < size; j++) for (let i = 0; i < size; i++) out.push([start + i, start + j]);
  return out;
}
