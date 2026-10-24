// Tool behaviours. Plain TypeScript, driven by CanvasView pointer events;
// nothing here touches React.

import type { Command, FrameId, Hex, LayerId } from "../engine/types";
import type { CelCache } from "./CelCache";
import { rgba, TRANSPARENT } from "./color";
import { boxFrom, brushOffsets, ellipse, line, rectangle } from "./raster";

export interface Target {
  layer: LayerId;
  frame: FrameId;
}

export interface ToolHost {
  readonly cache: CelCache;
  docWidth(): number;
  docHeight(): number;
  /** The editable cel, or null (with user feedback) if drawing is not allowed. */
  target(): Target | null;
  /** Execute in the engine; the affected region is re-read afterwards. */
  run(command: Command): Promise<unknown>;
  invalidate(): void;
  pick(x: number, y: number): Hex;
  setColor(slot: "primary" | "secondary", color: Hex): void;
  brushSize(): number;
  filledShapes(): boolean;
  fillContiguous(): boolean;
  symmetry(): { x: boolean; y: boolean };
}

export interface ToolInput {
  x: number;
  y: number;
  shift: boolean;
  /** Primary (left) or secondary (right) color. */
  slot: "primary" | "secondary";
  color: Hex;
}

export interface Tool {
  begin(h: ToolHost, p: ToolInput): void;
  drag(h: ToolHost, p: ToolInput): void;
  end(h: ToolHost, p: ToolInput): void;
  /** Abort an in-progress gesture (e.g. Escape). */
  cancel?(h: ToolHost): void;
  /** Whether the hover outline should show the brush footprint. */
  readonly brushCursor?: boolean;
}

/** All mirror images of a point under the active symmetry axes. */
function mirrors(h: ToolHost, x: number, y: number): Array<[number, number]> {
  const { x: sx, y: sy } = h.symmetry();
  const w = h.docWidth(),
    ht = h.docHeight();
  const out: Array<[number, number]> = [[x, y]];
  if (sx) out.push([w - 1 - x, y]);
  if (sy) out.push([x, ht - 1 - y]);
  if (sx && sy) out.push([w - 1 - x, ht - 1 - y]);
  return out;
}

type Box = { x: number; y: number; width: number; height: number };

function mirrorBoxes(h: ToolHost, b: Box): Box[] {
  const { x: sx, y: sy } = h.symmetry();
  const w = h.docWidth(),
    ht = h.docHeight();
  const fx = (r: Box) => ({ ...r, x: w - r.x - r.width });
  const fy = (r: Box) => ({ ...r, y: ht - r.y - r.height });
  const out = [b];
  if (sx) out.push(fx(b));
  if (sy) out.push(fy(b));
  if (sx && sy) out.push(fy(fx(b)));
  return out;
}

function batch(label: string, commands: Command[]): Command {
  return commands.length === 1 ? commands[0]! : { type: "batch", label, commands };
}

/**
 * Pencil and eraser: paint into the local cache for instant feedback, then
 * commit one `set_pixels` per stroke. The engine result replaces the preview.
 */
export class BrushTool implements Tool {
  readonly brushCursor = true;
  private writes = new Map<number, Hex>();
  private target: Target | null = null;
  private last: { x: number; y: number } | null = null;
  private lastEnd: { x: number; y: number; layer: LayerId } | null = null;
  private color: Hex = TRANSPARENT;

  constructor(private readonly erase: boolean) {}

  begin(h: ToolHost, p: ToolInput) {
    this.target = h.target();
    if (!this.target) return;
    this.writes.clear();
    this.color = this.erase ? TRANSPARENT : p.color;
    if (p.shift && this.lastEnd && this.lastEnd.layer === this.target.layer) {
      this.segment(h, this.lastEnd, p);
    } else {
      this.stamp(h, p.x, p.y);
    }
    this.last = { x: p.x, y: p.y };
    this.flush(h);
  }

  drag(h: ToolHost, p: ToolInput) {
    if (!this.target || !this.last) return;
    if (p.x === this.last.x && p.y === this.last.y) return;
    this.segment(h, this.last, p);
    this.last = { x: p.x, y: p.y };
    this.flush(h);
  }

  end(h: ToolHost) {
    const target = this.target;
    this.target = null;
    if (!target || this.writes.size === 0) return;
    const w = h.docWidth();
    const pixels = [...this.writes].map(([k, color]) => ({
      x: k % w,
      y: Math.floor(k / w),
      color,
    }));
    if (this.last) this.lastEnd = { ...this.last, layer: target.layer };
    this.writes.clear();
    void h.run({ type: "set_pixels", ...target, pixels });
  }

  cancel(h: ToolHost) {
    this.end(h);
  }

  private segment(h: ToolHost, a: { x: number; y: number }, b: { x: number; y: number }) {
    line(a.x, a.y, b.x, b.y, (x, y) => this.stamp(h, x, y));
  }

  private stamp(h: ToolHost, cx: number, cy: number) {
    const surface = h.cache.surface(this.target!.layer);
    const c = rgba(this.color);
    const w = h.docWidth(),
      ht = h.docHeight();
    for (const [dx, dy] of brushOffsets(h.brushSize())) {
      for (const [x, y] of mirrors(h, cx + dx, cy + dy)) {
        if (x < 0 || y < 0 || x >= w || y >= ht) continue;
        surface.set(x, y, c);
        this.writes.set(y * w + x, this.color);
      }
    }
  }

  private flush(h: ToolHost) {
    h.cache.surface(this.target!.layer).flush();
    h.invalidate();
  }
}

type ShapeKind = "line" | "rectangle" | "ellipse";

/** Line, rectangle and ellipse: preview layer while dragging, one command on release. */
export class ShapeTool implements Tool {
  private start: { x: number; y: number } | null = null;
  private target: Target | null = null;
  private color: Hex = TRANSPARENT;

  constructor(private readonly kind: ShapeKind) {}

  begin(h: ToolHost, p: ToolInput) {
    this.target = h.target();
    if (!this.target) return;
    this.start = { x: p.x, y: p.y };
    this.color = p.color;
    this.preview(h, p);
  }

  drag(h: ToolHost, p: ToolInput) {
    if (this.target) this.preview(h, p);
  }

  end(h: ToolHost, p: ToolInput) {
    const target = this.target;
    if (!target || !this.start) return;
    const commands = this.commands(h, target, p);
    this.start = null;
    this.target = null;
    void h.run(batch(this.kind, commands)).finally(() => {
      h.cache.preview.clear();
      h.invalidate();
    });
  }

  cancel(h: ToolHost) {
    this.start = null;
    this.target = null;
    h.cache.preview.clear();
    h.invalidate();
  }

  private commands(h: ToolHost, t: Target, p: ToolInput): Command[] {
    const s = this.start!;
    const color = this.color;
    if (this.kind === "line") {
      const a = mirrors(h, s.x, s.y);
      const b = mirrors(h, p.x, p.y);
      return a.map(([x, y], i) => ({
        type: "draw_line" as const,
        ...t,
        from: { x, y },
        to: { x: b[i]![0], y: b[i]![1] },
        color,
      }));
    }
    const type =
      this.kind === "rectangle" ? ("draw_rectangle" as const) : ("draw_ellipse" as const);
    const filled = h.filledShapes();
    return mirrorBoxes(h, boxFrom(s.x, s.y, p.x, p.y, p.shift)).map((b) => ({
      type,
      ...t,
      ...b,
      color,
      filled,
    }));
  }

  private preview(h: ToolHost, p: ToolInput) {
    const pv = h.cache.preview;
    pv.clear();
    const c = rgba(this.color);
    // Show erasing shapes (transparent color) as a faint outline.
    const shown = c[3] === 0 ? ([128, 128, 128, 160] as typeof c) : c;
    const plot = (x: number, y: number) => pv.set(x, y, shown);
    for (const cmd of this.commands(h, this.target!, p)) {
      if (cmd.type === "draw_line") line(cmd.from.x, cmd.from.y, cmd.to.x, cmd.to.y, plot);
      else if (cmd.type === "draw_rectangle")
        rectangle(cmd.x, cmd.y, cmd.width, cmd.height, !!cmd.filled, plot);
      else if (cmd.type === "draw_ellipse")
        ellipse(cmd.x, cmd.y, cmd.width, cmd.height, !!cmd.filled, plot);
    }
    pv.flush();
    h.invalidate();
  }
}

export class FillTool implements Tool {
  begin(h: ToolHost, p: ToolInput) {
    const t = h.target();
    if (!t) return;
    if (p.x < 0 || p.y < 0 || p.x >= h.docWidth() || p.y >= h.docHeight()) return;
    const contiguous = h.fillContiguous();
    const cmds = mirrors(h, p.x, p.y).map(([x, y]) => ({
      type: "fill_region" as const,
      ...t,
      x,
      y,
      color: p.color,
      contiguous,
    }));
    void h.run(batch("Fill", cmds));
  }
  drag() {}
  end() {}
}

export class EyedropperTool implements Tool {
  begin(h: ToolHost, p: ToolInput) {
    this.sample(h, p);
  }
  drag(h: ToolHost, p: ToolInput) {
    this.sample(h, p);
  }
  end() {}
  private sample(h: ToolHost, p: ToolInput) {
    if (p.x < 0 || p.y < 0 || p.x >= h.docWidth() || p.y >= h.docHeight()) return;
    h.setColor(p.slot, h.pick(p.x, p.y));
  }
}

export function createTools(): Record<string, Tool> {
  return {
    pencil: new BrushTool(false),
    eraser: new BrushTool(true),
    line: new ShapeTool("line"),
    rectangle: new ShapeTool("rectangle"),
    ellipse: new ShapeTool("ellipse"),
    fill: new FillTool(),
    eyedropper: new EyedropperTool(),
  };
}
