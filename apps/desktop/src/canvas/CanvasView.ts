import type { Command, Frame, Hex, Layer } from "../engine/types";
import {
  activeLayer,
  getEditor,
  setEditor,
  toast,
  useEditor,
  type EditorState,
  type ToolId,
} from "../state/editor";
import type { CelCache } from "./CelCache";
import { rgba, toHex } from "./color";
import type { FrameStore } from "./FrameStore";
import { ONION_TINTS, onionFrames } from "./onion";
import type { Player } from "./playback";
import { brushOffsets } from "./raster";
import { drawRulers, RULER_SIZE } from "./rulers";
import { createTools, type Tool, type ToolHost, type ToolInput } from "./tools";
import { center, fit, nextZoom, scaleOf, toCanvas, zoomAt, type Viewport } from "./viewport";

export interface CanvasViewDeps {
  cache: CelCache;
  /** Composites for onion skins and playback. */
  frames: FrameStore;
  player: Player;
  run(command: Command): Promise<unknown>;
  /** Called before a tool starts, so edits never land on a playing canvas. */
  stopPlayback(): void;
}

const DRAW_TOOLS: ToolId[] = ["pencil", "eraser", "fill", "line", "rectangle", "ellipse"];

/**
 * The pixel canvas: view transform, rendering and pointer input.
 * Lives entirely outside React; React only mounts it into a container.
 */
export class CanvasView implements ToolHost {
  readonly cache: CelCache;
  private canvas: HTMLCanvasElement;
  private ctx: CanvasRenderingContext2D;
  private vp: Viewport = { zoom: 8, dpr: window.devicePixelRatio || 1, ox: 0, oy: 0 };
  private cssW = 0;
  private cssH = 0;
  private frameRequested = false;
  private tools = createTools();
  private active: { tool: Tool; pointer: number } | null = null;
  private pan: { pointer: number; x: number; y: number; ox: number; oy: number } | null = null;
  private spaceDown = false;
  private altDown = false;
  private hover: { x: number; y: number } | null = null;
  private pinchAcc = 0;
  private colors = {
    workspace: "#111214",
    checkA: "#2a2b30",
    checkB: "#232428",
    grid: "#ffffff1f",
    panel: "#1d1e22",
    hover: "#2c2d33",
    border: "#2e2f35",
    faint: "#6b6c74",
    dim: "#9d9ea6",
    accent: "#ff5a5f",
    accentText: "#ffffff",
  };
  private disposers: Array<() => void> = [];
  private resizeObserver: ResizeObserver;
  private fitPending = true;

  constructor(
    private container: HTMLElement,
    private deps: CanvasViewDeps,
  ) {
    this.cache = deps.cache;
    this.canvas = document.createElement("canvas");
    // Absolutely positioned so its pixel size never feeds back into the layout.
    this.canvas.className = "absolute inset-0 block h-full w-full touch-none outline-none";
    this.canvas.tabIndex = 0;
    container.appendChild(this.canvas);
    this.ctx = this.canvas.getContext("2d", { alpha: false })!;

    this.resizeObserver = new ResizeObserver(() => this.resize());
    this.resizeObserver.observe(container);
    this.resize();
    this.readTheme();

    const on = <K extends keyof HTMLElementEventMap>(
      el: HTMLElement | Window,
      type: K,
      fn: (e: HTMLElementEventMap[K]) => void,
      opts?: AddEventListenerOptions,
    ) => {
      el.addEventListener(type, fn as EventListener, opts);
      this.disposers.push(() => el.removeEventListener(type, fn as EventListener, opts));
    };
    on(this.canvas, "pointerdown", (e) => this.onPointerDown(e));
    on(this.canvas, "pointermove", (e) => this.onPointerMove(e));
    on(this.canvas, "pointerup", (e) => this.onPointerUp(e));
    on(this.canvas, "pointercancel", (e) => this.onPointerUp(e));
    on(this.canvas, "pointerleave", () => this.setHover(null));
    on(this.canvas, "wheel", (e) => this.onWheel(e), { passive: false });
    on(this.canvas, "contextmenu", (e) => e.preventDefault());
    on(window, "keydown", (e) => this.onKey(e, true));
    on(window, "keyup", (e) => this.onKey(e, false));
    on(window, "blur", () => {
      this.spaceDown = this.altDown = false;
      this.updateCursor();
    });

    // Redraw on the store changes that affect rendering.
    this.disposers.push(
      useEditor.subscribe((s, p) => {
        if (s.theme !== p.theme) this.readTheme();
        if (
          s.session !== p.session ||
          s.showGrid !== p.showGrid ||
          s.showRulers !== p.showRulers ||
          s.symmetryX !== p.symmetryX ||
          s.symmetryY !== p.symmetryY ||
          s.symmetryMode !== p.symmetryMode ||
          s.primary !== p.primary ||
          s.activeLayerId !== p.activeLayerId ||
          s.opacityPreview !== p.opacityPreview ||
          s.tool !== p.tool ||
          s.brushSize !== p.brushSize ||
          s.theme !== p.theme ||
          s.onion !== p.onion ||
          s.activeFrameId !== p.activeFrameId
        ) {
          this.updateCursor();
          this.invalidate();
        }
      }),
    );
    // Pixels under the cursor can change without the pointer moving.
    this.disposers.push(this.cache.onChange(() => this.updateStatus()));
    this.disposers.push(deps.frames.onChange(() => this.invalidate()));
    this.disposers.push(deps.player.onChange(() => this.invalidate()));
    this.updateCursor();
  }

  dispose() {
    this.resizeObserver.disconnect();
    for (const d of this.disposers) d();
    this.canvas.remove();
  }

  // ── ToolHost ─────────────────────────────────────────────────────────

  docWidth() {
    return this.cache.width;
  }
  docHeight() {
    return this.cache.height;
  }
  target() {
    const s = getEditor();
    const layer = activeLayer(s);
    if (!layer || s.activeFrameId == null) return null;
    if (layer.locked) {
      toast(`“${layer.name}” is locked`, "error");
      return null;
    }
    if (!layer.visible) {
      toast(`“${layer.name}” is hidden`, "error");
      return null;
    }
    return { layer: layer.id, frame: s.activeFrameId };
  }
  run(command: Command) {
    return this.deps.run(command);
  }
  pick(x: number, y: number): Hex {
    const layers = getEditor().session?.summary.layers ?? [];
    return toHex(this.cache.composite(layers, x, y));
  }
  setColor(slot: "primary" | "secondary", color: Hex) {
    setEditor({ [slot]: color });
  }
  brushSize() {
    return getEditor().brushSize;
  }
  filledShapes() {
    return getEditor().filledShapes;
  }
  fillContiguous() {
    return getEditor().fillContiguous;
  }
  /** Axes that strokes are mirrored across. In guide mode nothing is mirrored. */
  symmetry() {
    const s = getEditor();
    if (s.symmetryMode === "guide") return { x: false, y: false };
    return { x: s.symmetryX, y: s.symmetryY };
  }

  // ── View transform ───────────────────────────────────────────────────

  /** Fit the document on the next layout (or now if already laid out). */
  fitToWindow() {
    if (this.cssW === 0 || this.cache.width === 0) {
      this.fitPending = true;
      return;
    }
    this.fitPending = false;
    this.setViewport(fit(this.cache.width, this.cache.height, this.cssW, this.cssH, this.vp.dpr));
  }

  zoomStep(dir: 1 | -1, cssX = this.cssW / 2, cssY = this.cssH / 2) {
    this.setViewport(zoomAt(this.vp, nextZoom(this.vp.zoom, dir), cssX, cssY));
  }

  actualSize() {
    this.setViewport(
      center({ ...this.vp, zoom: 1 }, this.cache.width, this.cache.height, this.cssW, this.cssH),
    );
  }

  private setViewport(v: Viewport) {
    this.vp = v;
    if (getEditor().zoom !== v.zoom) setEditor({ zoom: v.zoom });
    this.invalidate();
  }

  private resize() {
    const rect = this.container.getBoundingClientRect();
    const dpr = window.devicePixelRatio || 1;
    const prev = { w: this.cssW, h: this.cssH };
    this.cssW = rect.width;
    this.cssH = rect.height;
    this.canvas.width = Math.max(1, Math.round(rect.width * dpr));
    this.canvas.height = Math.max(1, Math.round(rect.height * dpr));
    if (dpr !== this.vp.dpr) this.vp = { ...this.vp, dpr };
    if (this.fitPending) this.fitToWindow();
    else if (prev.w > 0) {
      // Keep the canvas centred relative to the resized area.
      this.vp = {
        ...this.vp,
        ox: this.vp.ox + Math.round(((this.cssW - prev.w) * dpr) / 2),
        oy: this.vp.oy + Math.round(((this.cssH - prev.h) * dpr) / 2),
      };
    }
    this.render();
  }

  private readTheme() {
    const cs = getComputedStyle(document.documentElement);
    const v = (name: string, fallback: string) => cs.getPropertyValue(name).trim() || fallback;
    this.colors = {
      workspace: v("--workspace", this.colors.workspace),
      checkA: v("--check-a", this.colors.checkA),
      checkB: v("--check-b", this.colors.checkB),
      grid: v("--grid", this.colors.grid),
      panel: v("--panel", this.colors.panel),
      hover: v("--hover", this.colors.hover),
      border: v("--border", this.colors.border),
      faint: v("--text-faint", this.colors.faint),
      dim: v("--text-dim", this.colors.dim),
      accent: v("--accent", this.colors.accent),
      accentText: v("--accent-text", this.colors.accentText),
    };
    this.checker = null;
  }

  // ── Rendering ────────────────────────────────────────────────────────

  invalidate() {
    if (this.frameRequested) return;
    this.frameRequested = true;
    requestAnimationFrame(() => {
      this.frameRequested = false;
      this.render();
    });
  }

  private checker: CanvasPattern | null = null;

  private checkerPattern(): CanvasPattern {
    if (this.checker) return this.checker;
    const size = Math.round(6 * this.vp.dpr);
    const c = document.createElement("canvas");
    c.width = c.height = size * 2;
    const g = c.getContext("2d")!;
    g.fillStyle = this.colors.checkA;
    g.fillRect(0, 0, size * 2, size * 2);
    g.fillStyle = this.colors.checkB;
    g.fillRect(0, 0, size, size);
    g.fillRect(size, size, size, size);
    this.checker = this.ctx.createPattern(c, "repeat")!;
    return this.checker;
  }

  private render() {
    const { ctx, canvas, vp } = this;
    const s = getEditor();
    const summary = s.session?.summary;
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.fillStyle = this.colors.workspace;
    ctx.fillRect(0, 0, canvas.width, canvas.height);
    if (!summary || this.cache.width === 0) return;

    const scale = scaleOf(vp);
    const w = summary.width * scale;
    const h = summary.height * scale;

    // Transparency checkerboard, anchored to the canvas origin.
    const pattern = this.checkerPattern();
    pattern.setTransform(new DOMMatrix().translateSelf(vp.ox, vp.oy));
    ctx.fillStyle = pattern;
    ctx.fillRect(vp.ox, vp.oy, w, h);

    ctx.imageSmoothingEnabled = false;
    const { frames, player } = this.deps;
    const playing = player.playing ? summary.frames[player.index] : undefined;
    const composite = playing && frames.get(playing.id);
    if (composite) {
      ctx.drawImage(composite, vp.ox, vp.oy, w, h);
    } else {
      this.drawOnion(summary.frames, s, w, h);
      this.drawLayers(summary.layers, s, w, h);
    }
    ctx.globalAlpha = 1;

    if (s.showGrid && scale >= 6) this.drawGrid(summary.width, summary.height, scale);

    // Canvas edge.
    ctx.strokeStyle = "rgba(0,0,0,0.5)";
    ctx.lineWidth = 1;
    ctx.strokeRect(vp.ox - 0.5, vp.oy - 0.5, w + 1, h + 1);

    this.drawMirrorAxes(summary.width, summary.height, scale);
    if (!composite) this.drawHover(scale);
    if (s.showRulers) {
      drawRulers(ctx, vp, canvas.width, canvas.height, summary.width, summary.height, this.hover, {
        background: this.colors.panel,
        canvasRange: this.colors.hover,
        border: this.colors.border,
        tick: this.colors.faint,
        text: this.colors.dim,
        accent: this.colors.accent,
        accentText: this.colors.accentText,
      });
    }
  }

  /** Dashed lines through the canvas centre for each active symmetry axis. */
  private drawMirrorAxes(cols: number, rows: number, scale: number) {
    const s = getEditor();
    if (!s.symmetryX && !s.symmetryY) return;
    const { ctx, vp } = this;
    const dpr = vp.dpr;
    ctx.save();
    ctx.strokeStyle = this.colors.accent;
    ctx.globalAlpha = s.symmetryMode === "guide" ? 0.45 : 0.8;
    ctx.lineWidth = Math.max(1, Math.round(dpr));
    ctx.setLineDash([4 * dpr, 4 * dpr]);
    ctx.beginPath();
    if (s.symmetryX) {
      const x = vp.ox + Math.round((cols * scale) / 2) + 0.5;
      ctx.moveTo(x, vp.oy);
      ctx.lineTo(x, vp.oy + rows * scale);
    }
    if (s.symmetryY) {
      const y = vp.oy + Math.round((rows * scale) / 2) + 0.5;
      ctx.moveTo(vp.ox, y);
      ctx.lineTo(vp.ox + cols * scale, y);
    }
    ctx.stroke();
    ctx.restore();
  }

  /** Is (cssX, cssY) over a ruler strip rather than the drawing area? */
  private overRuler(e: PointerEvent): boolean {
    if (!getEditor().showRulers) return false;
    const r = this.canvas.getBoundingClientRect();
    return e.clientX - r.left < RULER_SIZE || e.clientY - r.top < RULER_SIZE;
  }

  /** Neighbouring frames, faded (and optionally tinted), under the active one. */
  private drawOnion(frames: Frame[], s: EditorState, w: number, h: number) {
    const { ctx, vp } = this;
    const active = frames.findIndex((f) => f.id === s.activeFrameId);
    for (const o of onionFrames(active, frames.length, s.onion)) {
      const id = frames[o.index]!.id;
      const img = s.onion.tint
        ? this.deps.frames.tinted(id, ONION_TINTS[o.side])
        : this.deps.frames.get(id);
      if (!img) continue;
      ctx.globalAlpha = o.alpha;
      ctx.drawImage(img, vp.ox, vp.oy, w, h);
    }
    ctx.globalAlpha = 1;
  }

  private drawLayers(layers: Layer[], s: EditorState, w: number, h: number) {
    const { ctx, vp } = this;
    for (const layer of layers) {
      if (!layer.visible) continue;
      const opacity = s.opacityPreview?.layer === layer.id ? s.opacityPreview.value : layer.opacity;
      ctx.globalAlpha = opacity / 255;
      if (this.cache.has(layer.id)) {
        ctx.drawImage(this.cache.surface(layer.id).canvas, vp.ox, vp.oy, w, h);
      }
      if (layer.id === s.activeLayerId && !this.cache.preview.empty) {
        ctx.drawImage(this.cache.preview.canvas, vp.ox, vp.oy, w, h);
      }
    }
  }

  private drawGrid(cols: number, rows: number, scale: number) {
    const { ctx, vp } = this;
    ctx.fillStyle = this.colors.grid;
    const x0 = Math.max(0, Math.floor(-vp.ox / scale));
    const x1 = Math.min(cols, Math.ceil((this.canvas.width - vp.ox) / scale));
    const y0 = Math.max(0, Math.floor(-vp.oy / scale));
    const y1 = Math.min(rows, Math.ceil((this.canvas.height - vp.oy) / scale));
    for (let x = x0 + 1; x < x1; x++)
      ctx.fillRect(vp.ox + x * scale, vp.oy + y0 * scale, 1, (y1 - y0) * scale);
    for (let y = y0 + 1; y < y1; y++)
      ctx.fillRect(vp.ox + x0 * scale, vp.oy + y * scale, (x1 - x0) * scale, 1);
  }

  private drawHover(scale: number) {
    const s = getEditor();
    if (!this.hover || this.pan || this.spaceDown) return;
    const tool = this.currentToolId();
    if (tool === "hand") return;
    const size = tool === "pencil" || tool === "eraser" ? s.brushSize : 1;
    const off = brushOffsets(size)[0]!;
    const x = this.vp.ox + (this.hover.x + off[0]) * scale;
    const y = this.vp.oy + (this.hover.y + off[1]) * scale;
    const side = size * scale;
    const { ctx } = this;
    this.drawMirrorGhosts(this.hover.x + off[0], this.hover.y + off[1], size, scale);
    ctx.lineWidth = 1;
    ctx.strokeStyle = "rgba(0,0,0,0.75)";
    ctx.strokeRect(x - 0.5, y - 0.5, side + 1, side + 1);
    ctx.strokeStyle = "rgba(255,255,255,0.9)";
    ctx.strokeRect(x + 0.5, y + 0.5, side - 1, side - 1);
  }

  /**
   * Where the brush footprint at cell (cx, cy) lands under each active
   * mirror: a see-through swatch of the current color with a dashed outline.
   * In guide mode this is the only effect; nothing is painted there.
   */
  private drawMirrorGhosts(cx: number, cy: number, size: number, scale: number) {
    const s = getEditor();
    if (!s.symmetryX && !s.symmetryY) return;
    const w = this.cache.width,
      h = this.cache.height;
    const mx = w - cx - size,
      my = h - cy - size;
    const spots: Array<[number, number]> = [];
    if (s.symmetryX) spots.push([mx, cy]);
    if (s.symmetryY) spots.push([cx, my]);
    if (s.symmetryX && s.symmetryY) spots.push([mx, my]);
    const { ctx, vp } = this;
    const side = size * scale;
    const [r, g, b, a] = rgba(s.primary);
    ctx.save();
    for (const [gx, gy] of spots) {
      if (gx === cx && gy === cy) continue;
      const x = vp.ox + gx * scale,
        y = vp.oy + gy * scale;
      if (a > 0 && s.tool !== "eraser") {
        ctx.fillStyle = `rgba(${r},${g},${b},0.45)`;
        ctx.fillRect(x, y, side, side);
      }
      ctx.lineWidth = Math.max(1, Math.round(vp.dpr));
      ctx.setLineDash([3 * vp.dpr, 2 * vp.dpr]);
      ctx.strokeStyle = this.colors.accent;
      ctx.strokeRect(x + 0.5, y + 0.5, side - 1, side - 1);
    }
    ctx.restore();
  }

  // ── Input ────────────────────────────────────────────────────────────

  private currentToolId(): ToolId {
    const tool = getEditor().tool;
    if (this.altDown && DRAW_TOOLS.includes(tool)) return "eyedropper";
    return tool;
  }

  private updateCursor() {
    const tool = this.currentToolId();
    this.canvas.style.cursor = this.pan
      ? "grabbing"
      : this.spaceDown || tool === "hand"
        ? "grab"
        : "crosshair";
  }

  private pixelAt(e: PointerEvent | WheelEvent) {
    const r = this.canvas.getBoundingClientRect();
    const p = toCanvas(this.vp, e.clientX - r.left, e.clientY - r.top);
    return { x: Math.floor(p.x), y: Math.floor(p.y) };
  }

  private input(e: PointerEvent, slot: "primary" | "secondary"): ToolInput {
    const s = getEditor();
    return { ...this.pixelAt(e), shift: e.shiftKey, slot, color: s[slot] };
  }

  private slot: "primary" | "secondary" = "primary";

  private onPointerDown(e: PointerEvent) {
    this.canvas.focus({ preventScroll: true });
    if (this.active || this.pan) return;
    const tool = this.currentToolId();
    if (e.button === 1 || (e.button === 0 && (this.spaceDown || tool === "hand"))) {
      this.pan = {
        pointer: e.pointerId,
        x: e.clientX,
        y: e.clientY,
        ox: this.vp.ox,
        oy: this.vp.oy,
      };
      this.canvas.setPointerCapture(e.pointerId);
      this.updateCursor();
      return;
    }
    if (e.button !== 0 && e.button !== 2) return;
    if (this.overRuler(e)) return;
    const impl = this.tools[tool];
    if (!impl) return;
    this.deps.stopPlayback();
    this.slot = e.button === 2 ? "secondary" : "primary";
    this.canvas.setPointerCapture(e.pointerId);
    this.active = { tool: impl, pointer: e.pointerId };
    impl.begin(this, this.input(e, this.slot));
  }

  private onPointerMove(e: PointerEvent) {
    if (this.pan && e.pointerId === this.pan.pointer) {
      const dpr = this.vp.dpr;
      this.setViewport({
        ...this.vp,
        ox: this.pan.ox + Math.round((e.clientX - this.pan.x) * dpr),
        oy: this.pan.oy + Math.round((e.clientY - this.pan.y) * dpr),
      });
      return;
    }
    if (this.active && e.pointerId === this.active.pointer) {
      const events = e.getCoalescedEvents?.() ?? [e];
      for (const ce of events.length ? events : [e])
        this.active.tool.drag(this, this.input(ce, this.slot));
    }
    this.setHover(this.pixelAt(e));
  }

  private onPointerUp(e: PointerEvent) {
    if (this.pan && e.pointerId === this.pan.pointer) {
      this.pan = null;
      this.updateCursor();
      return;
    }
    if (this.active && e.pointerId === this.active.pointer) {
      const { tool } = this.active;
      this.active = null;
      tool.end(this, this.input(e, this.slot));
    }
  }

  private hoverQueued = false;

  private setHover(p: { x: number; y: number } | null) {
    const same = p && this.hover && p.x === this.hover.x && p.y === this.hover.y;
    if (same || (!p && !this.hover)) return;
    this.hover = p;
    this.invalidate();
    this.updateStatus();
  }

  private updateStatus() {
    if (this.hoverQueued) return;
    this.hoverQueued = true;
    // Status bar updates are throttled to one per frame.
    requestAnimationFrame(() => {
      this.hoverQueued = false;
      const h = this.hover;
      const inside = h && h.x >= 0 && h.y >= 0 && h.x < this.cache.width && h.y < this.cache.height;
      setEditor({ cursor: h ? { ...h, color: inside ? this.pick(h.x, h.y) : null } : null });
    });
  }

  private onWheel(e: WheelEvent) {
    e.preventDefault();
    const r = this.canvas.getBoundingClientRect();
    const cx = e.clientX - r.left,
      cy = e.clientY - r.top;
    if (e.ctrlKey || e.metaKey || e.altKey) {
      // Trackpad pinch arrives as ctrl+wheel with small deltas; accumulate.
      this.pinchAcc += e.deltaY;
      const threshold = e.ctrlKey && Math.abs(e.deltaY) < 10 ? 12 : 1;
      if (Math.abs(this.pinchAcc) >= threshold) {
        this.zoomStep(this.pinchAcc < 0 ? 1 : -1, cx, cy);
        this.pinchAcc = 0;
      }
      return;
    }
    const k = e.deltaMode === 1 ? 16 : 1;
    this.setViewport({
      ...this.vp,
      ox: this.vp.ox - Math.round(e.deltaX * k * this.vp.dpr),
      oy: this.vp.oy - Math.round(e.deltaY * k * this.vp.dpr),
    });
  }

  private onKey(e: KeyboardEvent, down: boolean) {
    const t = e.target as HTMLElement | null;
    const typing = t && (t.tagName === "INPUT" || t.tagName === "TEXTAREA" || t.isContentEditable);
    if (e.key === "Alt") {
      this.altDown = down;
      this.updateCursor();
      this.invalidate();
    }
    if (typing) return;
    if (e.code === "Space") {
      e.preventDefault();
      if (this.spaceDown !== down) {
        this.spaceDown = down;
        this.updateCursor();
        this.invalidate();
      }
    }
    if (down && e.key === "Escape" && this.active) {
      this.active.tool.cancel?.(this);
      this.active = null;
    }
  }
}
