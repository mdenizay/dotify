import type { ArtboardId } from "../engine/types";
import { getEditor, useEditor } from "../state/editor";
import type { ArtboardImages } from "./ArtboardImages";
import {
  DEFAULT_LAYOUT,
  hitTest,
  layoutOverview,
  matchesSearch,
  type OverviewLayout,
} from "./overviewLayout";

export interface OverviewDeps {
  images: ArtboardImages;
  select(artboard: ArtboardId): void;
  open(artboard: ArtboardId): void;
}

/** Overview zoom levels; fractional levels let large projects fit. */
export const OVERVIEW_ZOOM_LEVELS = [
  0.125, 0.25, 0.5, 1, 1.5, 2, 3, 4, 6, 8, 12, 16, 24, 32,
] as const;

interface View {
  /** CSS pixels per artboard pixel. */
  zoom: number;
  /** World origin in CSS pixels relative to the element. */
  ox: number;
  oy: number;
}

const LABEL_FONT = '11px -apple-system, BlinkMacSystemFont, "SF Pro Text", system-ui, sans-serif';
const HEADER_FONT =
  '600 11px -apple-system, BlinkMacSystemFont, "SF Pro Text", system-ui, sans-serif';

/**
 * Every artboard of the project side by side, grouped by folder, with pan
 * and zoom. Click selects, double-click opens in the editor. Like
 * `CanvasView`, it lives outside React.
 */
export class OverviewView {
  private canvas: HTMLCanvasElement;
  private ctx: CanvasRenderingContext2D;
  private view: View = { zoom: 4, ox: 0, oy: 0 };
  private cssW = 0;
  private cssH = 0;
  private dpr = window.devicePixelRatio || 1;
  private layout: OverviewLayout = { items: [], sections: [], width: 0, height: 0 };
  private hover: ArtboardId | null = null;
  private pan: {
    pointer: number;
    x: number;
    y: number;
    ox: number;
    oy: number;
    moved: boolean;
  } | null = null;
  private pinchAcc = 0;
  private frameRequested = false;
  private fitPending = true;
  private colors = {
    workspace: "#111214",
    checkA: "#2b2c31",
    checkB: "#222327",
    text: "#e7e7ea",
    dim: "#9d9ea6",
    accent: "#ff5a5f",
  };
  private disposers: Array<() => void> = [];
  private resizeObserver: ResizeObserver;

  constructor(
    private container: HTMLElement,
    private deps: OverviewDeps,
  ) {
    this.canvas = document.createElement("canvas");
    this.canvas.className = "block h-full w-full touch-none outline-none";
    this.canvas.tabIndex = 0;
    this.canvas.setAttribute("aria-label", "Artboard overview");
    container.appendChild(this.canvas);
    this.ctx = this.canvas.getContext("2d", { alpha: false })!;
    this.readTheme();
    this.relayout();

    this.resizeObserver = new ResizeObserver(() => this.resize());
    this.resizeObserver.observe(container);
    this.resize();

    const on = <K extends keyof HTMLElementEventMap>(
      type: K,
      fn: (e: HTMLElementEventMap[K]) => void,
      opts?: AddEventListenerOptions,
    ) => {
      this.canvas.addEventListener(type, fn as EventListener, opts);
      this.disposers.push(() => this.canvas.removeEventListener(type, fn as EventListener, opts));
    };
    on("pointerdown", (e) => this.onPointerDown(e));
    on("pointermove", (e) => this.onPointerMove(e));
    on("pointerup", (e) => this.onPointerUp(e));
    on("pointercancel", () => (this.pan = null));
    on("pointerleave", () => this.setHover(null));
    on("dblclick", (e) => {
      const id = this.itemAt(e);
      if (id != null) this.deps.open(id);
    });
    on("wheel", (e) => this.onWheel(e), { passive: false });
    on("keydown", (e) => {
      const active = getEditor().session?.active_artboard;
      if (e.key === "Enter" && active != null) this.deps.open(active);
    });

    this.disposers.push(
      useEditor.subscribe((s, p) => {
        if (s.theme !== p.theme) this.readTheme();
        if (s.session?.project !== p.session?.project || s.librarySearch !== p.librarySearch) {
          this.relayout();
        }
        if (s.session !== p.session || s.librarySearch !== p.librarySearch || s.theme !== p.theme) {
          this.invalidate();
        }
      }),
      deps.images.onChange(() => this.invalidate()),
    );
  }

  dispose() {
    this.resizeObserver.disconnect();
    for (const d of this.disposers) d();
    this.canvas.remove();
  }

  // ── View ─────────────────────────────────────────────────────────────

  fitToWindow() {
    const { width, height } = this.layout;
    if (this.cssW === 0 || width === 0) {
      this.fitPending = true;
      return;
    }
    this.fitPending = false;
    const margin = 40;
    const avail = Math.min((this.cssW - margin * 2) / width, (this.cssH - margin * 2) / height);
    const zoom =
      [...OVERVIEW_ZOOM_LEVELS].reverse().find((z) => z <= Math.min(avail, 8)) ??
      OVERVIEW_ZOOM_LEVELS[0];
    this.setView({
      zoom,
      ox: Math.round((this.cssW - width * zoom) / 2),
      oy: Math.round(Math.max(margin, (this.cssH - height * zoom) / 2)),
    });
  }

  zoomStep(dir: 1 | -1, cssX = this.cssW / 2, cssY = this.cssH / 2) {
    const z = this.view.zoom;
    const zoom =
      dir > 0
        ? (OVERVIEW_ZOOM_LEVELS.find((l) => l > z) ?? OVERVIEW_ZOOM_LEVELS.at(-1)!)
        : ([...OVERVIEW_ZOOM_LEVELS].reverse().find((l) => l < z) ?? OVERVIEW_ZOOM_LEVELS[0]);
    this.zoomTo(zoom, cssX, cssY);
  }

  actualSize() {
    this.zoomTo(1, this.cssW / 2, this.cssH / 2);
  }

  private zoomTo(zoom: number, cssX: number, cssY: number) {
    const wx = (cssX - this.view.ox) / this.view.zoom;
    const wy = (cssY - this.view.oy) / this.view.zoom;
    this.setView({ zoom, ox: Math.round(cssX - wx * zoom), oy: Math.round(cssY - wy * zoom) });
  }

  private setView(v: View) {
    this.view = v;
    this.invalidate();
  }

  private relayout() {
    const s = getEditor();
    const project = s.session?.project;
    if (!project) return;
    const visible = project.artboards.filter((a) =>
      matchesSearch(a, project.folders, s.librarySearch),
    );
    const hadContent = this.layout.items.length > 0;
    this.layout = layoutOverview(visible, project.folders);
    if (!hadContent || this.fitPending) this.fitToWindow();
  }

  private resize() {
    const rect = this.container.getBoundingClientRect();
    this.dpr = window.devicePixelRatio || 1;
    this.cssW = rect.width;
    this.cssH = rect.height;
    this.canvas.width = Math.max(1, Math.round(rect.width * this.dpr));
    this.canvas.height = Math.max(1, Math.round(rect.height * this.dpr));
    if (this.fitPending) this.fitToWindow();
    this.render();
  }

  private readTheme() {
    const cs = getComputedStyle(document.documentElement);
    const v = (name: string, fallback: string) => cs.getPropertyValue(name).trim() || fallback;
    this.colors = {
      workspace: v("--workspace", this.colors.workspace),
      checkA: v("--check-a", this.colors.checkA),
      checkB: v("--check-b", this.colors.checkB),
      text: v("--text", this.colors.text),
      dim: v("--text-dim", this.colors.dim),
      accent: v("--accent", this.colors.accent),
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
    const size = 4;
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
    const { ctx, canvas, view, layout, colors } = this;
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.fillStyle = colors.workspace;
    ctx.fillRect(0, 0, canvas.width, canvas.height);
    const s = getEditor();
    const project = s.session?.project;
    if (!project) return;
    // Draw in CSS pixels; the canvas backing store is device pixels.
    ctx.setTransform(this.dpr, 0, 0, this.dpr, 0, 0);
    const z = view.zoom;
    const screen = (x: number, y: number) => ({ x: view.ox + x * z, y: view.oy + y * z });

    if (layout.items.length === 0) {
      ctx.fillStyle = colors.dim;
      ctx.font = LABEL_FONT;
      ctx.textAlign = "center";
      ctx.fillText("No artboards match the search.", this.cssW / 2, this.cssH / 2);
      ctx.textAlign = "start";
      return;
    }

    const headerRoom = DEFAULT_LAYOUT.header * z;
    if (headerRoom >= 12) {
      ctx.textBaseline = "bottom";
      ctx.font = HEADER_FONT;
      ctx.fillStyle = colors.dim;
      for (const sec of layout.sections) {
        const p = screen(sec.x, sec.y);
        ctx.fillText(sec.title.toUpperCase(), Math.round(p.x), Math.round(p.y + headerRoom - 4));
      }
    }

    const names = new Map(project.artboards.map((a) => [a.id, a.name]));
    const active = s.session?.active_artboard;
    ctx.font = LABEL_FONT;
    for (const it of layout.items) {
      const p = screen(it.x, it.y);
      const w = it.width * z;
      const h = it.height * z;
      if (p.x > this.cssW || p.y > this.cssH || p.x + w < 0 || p.y + h < -16) continue;
      ctx.fillStyle = this.checkerPattern();
      ctx.fillRect(p.x, p.y, w, h);
      const img = this.deps.images.get(it.id);
      if (img) {
        ctx.imageSmoothingEnabled = false;
        ctx.drawImage(img, p.x, p.y, w, h);
      }
      const selected = it.id === active;
      if (selected || it.id === this.hover) {
        ctx.strokeStyle = selected ? colors.accent : colors.dim;
        ctx.lineWidth = selected ? 2 : 1;
        const o = selected ? 1 : 0.5;
        ctx.strokeRect(p.x - o, p.y - o, w + o * 2, h + o * 2);
      } else {
        ctx.strokeStyle = "rgba(0,0,0,0.35)";
        ctx.lineWidth = 1;
        ctx.strokeRect(p.x - 0.5, p.y - 0.5, w + 1, h + 1);
      }
      // Names sit in the gap under each artboard once there is room.
      if (w >= 36 && DEFAULT_LAYOUT.gap * z >= 14) {
        ctx.fillStyle = selected ? colors.text : colors.dim;
        ctx.textBaseline = "top";
        ctx.fillText(this.fit(names.get(it.id) ?? "", w), p.x, p.y + h + 3);
      }
    }
  }

  /** Truncate `text` with an ellipsis to fit `width` CSS pixels. */
  private fit(text: string, width: number): string {
    if (this.ctx.measureText(text).width <= width) return text;
    let lo = 0;
    let hi = text.length;
    while (lo < hi) {
      const mid = (lo + hi + 1) >> 1;
      if (this.ctx.measureText(`${text.slice(0, mid)}…`).width <= width) lo = mid;
      else hi = mid - 1;
    }
    return `${text.slice(0, lo)}…`;
  }

  // ── Input ────────────────────────────────────────────────────────────

  private itemAt(e: MouseEvent): ArtboardId | null {
    const r = this.canvas.getBoundingClientRect();
    const x = (e.clientX - r.left - this.view.ox) / this.view.zoom;
    const y = (e.clientY - r.top - this.view.oy) / this.view.zoom;
    return hitTest(this.layout, x, y);
  }

  private onPointerDown(e: PointerEvent) {
    this.canvas.focus({ preventScroll: true });
    if (e.button !== 0 && e.button !== 1) return;
    this.pan = {
      pointer: e.pointerId,
      x: e.clientX,
      y: e.clientY,
      ox: this.view.ox,
      oy: this.view.oy,
      moved: false,
    };
    this.canvas.setPointerCapture(e.pointerId);
  }

  private onPointerMove(e: PointerEvent) {
    const pan = this.pan;
    if (pan && e.pointerId === pan.pointer) {
      const dx = e.clientX - pan.x;
      const dy = e.clientY - pan.y;
      if (!pan.moved && Math.hypot(dx, dy) < 3) return;
      pan.moved = true;
      this.canvas.style.cursor = "grabbing";
      this.setView({ ...this.view, ox: pan.ox + dx, oy: pan.oy + dy });
      return;
    }
    this.setHover(this.itemAt(e));
  }

  private onPointerUp(e: PointerEvent) {
    const pan = this.pan;
    if (!pan || e.pointerId !== pan.pointer) return;
    this.pan = null;
    this.canvas.style.cursor = this.hover != null ? "pointer" : "default";
    if (!pan.moved && e.button === 0) {
      const id = this.itemAt(e);
      if (id != null) this.deps.select(id);
    }
  }

  private setHover(id: ArtboardId | null) {
    if (id === this.hover) return;
    this.hover = id;
    this.canvas.style.cursor = id != null ? "pointer" : "default";
    this.invalidate();
  }

  private onWheel(e: WheelEvent) {
    e.preventDefault();
    const r = this.canvas.getBoundingClientRect();
    const cx = e.clientX - r.left;
    const cy = e.clientY - r.top;
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
    this.setView({
      ...this.view,
      ox: this.view.ox - Math.round(e.deltaX * k),
      oy: this.view.oy - Math.round(e.deltaY * k),
    });
  }
}
