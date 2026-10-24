import type { LayerId, Rect } from "../engine/types";
import { blendOver, type RGBA } from "./color";

/**
 * Render cache of the active frame: one offscreen canvas per layer plus a
 * preview layer for in-progress shapes. Pixels arrive from the engine; the
 * cache is never the source of truth.
 */
export class CelCache {
  width = 0;
  height = 0;
  private layers = new Map<LayerId, LayerSurface>();
  readonly preview = new LayerSurface(0, 0);
  private listeners = new Set<(layer: LayerId | null) => void>();
  version = 0;

  reset(width: number, height: number) {
    this.width = width;
    this.height = height;
    this.layers.clear();
    this.preview.resize(width, height);
    this.emit(null);
  }

  surface(layer: LayerId): LayerSurface {
    let s = this.layers.get(layer);
    if (!s) {
      s = new LayerSurface(this.width, this.height);
      this.layers.set(layer, s);
    }
    return s;
  }

  has(layer: LayerId) {
    return this.layers.has(layer);
  }

  retain(layers: LayerId[]) {
    for (const id of [...this.layers.keys()]) if (!layers.includes(id)) this.layers.delete(id);
  }

  /** Replace a region with engine data (`pixels` is rect-sized RGBA). */
  putRegion(layer: LayerId, rect: Rect, pixels: Uint8ClampedArray) {
    this.surface(layer).putRegion(rect, pixels);
    this.emit(layer);
  }

  /** Color of the flattened visible layers at (x, y). */
  composite(
    order: Array<{ id: LayerId; opacity: number; visible: boolean }>,
    x: number,
    y: number,
  ): RGBA {
    let out: RGBA = [0, 0, 0, 0];
    for (const l of order) {
      if (!l.visible) continue;
      const s = this.layers.get(l.id);
      if (s) out = blendOver(out, s.get(x, y), l.opacity);
    }
    return out;
  }

  onChange(fn: (layer: LayerId | null) => void) {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  }

  emit(layer: LayerId | null) {
    this.version++;
    for (const fn of this.listeners) fn(layer);
  }
}

export class LayerSurface {
  canvas: HTMLCanvasElement;
  private ctx: CanvasRenderingContext2D;
  private image: ImageData;
  /** Bounding box of pixels written since the last flush. */
  private pending: { x0: number; y0: number; x1: number; y1: number } | null = null;
  empty = true;

  constructor(width: number, height: number) {
    this.canvas = document.createElement("canvas");
    this.ctx = this.canvas.getContext("2d", { willReadFrequently: true })!;
    this.image = new ImageData(Math.max(1, width), Math.max(1, height));
    this.resize(width, height);
  }

  get data() {
    return this.image.data;
  }

  resize(width: number, height: number) {
    this.canvas.width = Math.max(1, width);
    this.canvas.height = Math.max(1, height);
    this.image = new ImageData(this.canvas.width, this.canvas.height);
    this.pending = null;
    this.empty = true;
  }

  get(x: number, y: number): RGBA {
    const i = (y * this.image.width + x) * 4;
    const d = this.image.data;
    return [d[i]!, d[i + 1]!, d[i + 2]!, d[i + 3]!];
  }

  /** Write one pixel locally; call `flush` to upload. */
  set(x: number, y: number, c: RGBA) {
    const w = this.image.width;
    if (x < 0 || y < 0 || x >= w || y >= this.image.height) return;
    this.image.data.set(c, (y * w + x) * 4);
    this.empty = false;
    const p = this.pending;
    if (!p) this.pending = { x0: x, y0: y, x1: x, y1: y };
    else {
      p.x0 = Math.min(p.x0, x);
      p.y0 = Math.min(p.y0, y);
      p.x1 = Math.max(p.x1, x);
      p.y1 = Math.max(p.y1, y);
    }
  }

  flush() {
    const p = this.pending;
    if (!p) return;
    this.ctx.putImageData(this.image, 0, 0, p.x0, p.y0, p.x1 - p.x0 + 1, p.y1 - p.y0 + 1);
    this.pending = null;
  }

  putRegion(rect: Rect, pixels: Uint8ClampedArray) {
    const w = this.image.width;
    const row = rect.width * 4;
    for (let j = 0; j < rect.height; j++) {
      this.image.data.set(pixels.subarray(j * row, (j + 1) * row), ((rect.y + j) * w + rect.x) * 4);
    }
    this.ctx.putImageData(this.image, 0, 0, rect.x, rect.y, rect.width, rect.height);
    this.empty = false;
  }

  clear() {
    if (this.empty) return;
    this.image.data.fill(0);
    this.ctx.clearRect(0, 0, this.canvas.width, this.canvas.height);
    this.pending = null;
    this.empty = true;
  }
}
