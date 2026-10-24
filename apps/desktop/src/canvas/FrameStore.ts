import type { FrameId } from "../engine/types";

interface Entry {
  canvas: HTMLCanvasElement;
  /** Needs a refetch; the old pixels are still shown meanwhile. */
  stale: boolean;
  loading: boolean;
  /** Pixels have arrived at least once. */
  ready: boolean;
  /** Tinted copies for onion skins, keyed by CSS color. */
  tints: Map<string, HTMLCanvasElement>;
}

/**
 * Engine-composited frames for playback, onion skins and the preview.
 * Frames load lazily on first use and are refetched after edits; until
 * then the previous pixels stay visible so nothing flickers.
 */
export class FrameStore {
  private width = 0;
  private height = 0;
  private entries = new Map<FrameId, Entry>();
  private listeners = new Set<() => void>();

  constructor(private load: (frame: FrameId) => Promise<Uint8ClampedArray<ArrayBuffer>>) {}

  reset(width: number, height: number) {
    this.width = width;
    this.height = height;
    this.entries.clear();
    this.emit();
  }

  /** Mark frames (all when omitted) as needing a refetch. */
  invalidate(frames?: Iterable<FrameId>) {
    const ids = frames ? [...frames] : [...this.entries.keys()];
    for (const id of ids) {
      const e = this.entries.get(id);
      if (e) e.stale = true;
    }
  }

  retain(frames: FrameId[]) {
    for (const id of [...this.entries.keys()]) if (!frames.includes(id)) this.entries.delete(id);
  }

  /** The composited frame, or null while it loads for the first time. */
  get(frame: FrameId): HTMLCanvasElement | null {
    const e = this.entries.get(frame);
    if (!e || e.stale) this.fetch(frame);
    return e?.ready ? e.canvas : null;
  }

  /** A copy with every visible pixel recolored to `color`. */
  tinted(frame: FrameId, color: string): HTMLCanvasElement | null {
    const base = this.get(frame);
    const e = this.entries.get(frame);
    if (!base || !e) return null;
    let t = e.tints.get(color);
    if (!t) {
      t = document.createElement("canvas");
      t.width = base.width;
      t.height = base.height;
      const g = t.getContext("2d")!;
      g.drawImage(base, 0, 0);
      g.globalCompositeOperation = "source-atop";
      g.fillStyle = color;
      g.fillRect(0, 0, t.width, t.height);
      e.tints.set(color, t);
    }
    return t;
  }

  onChange(fn: () => void) {
    this.listeners.add(fn);
    return () => {
      this.listeners.delete(fn);
    };
  }

  private fetch(frame: FrameId) {
    let e = this.entries.get(frame);
    if (e?.loading || this.width === 0) return;
    const { width, height } = this;
    if (!e) {
      const canvas = document.createElement("canvas");
      canvas.width = width;
      canvas.height = height;
      e = { canvas, stale: true, loading: false, ready: false, tints: new Map() };
      this.entries.set(frame, e);
    }
    const entry = e;
    entry.loading = true;
    entry.stale = false;
    this.load(frame)
      .then((pixels) => {
        // Dropped by a reset or a resize while loading.
        if (this.entries.get(frame) !== entry || width !== this.width || height !== this.height)
          return;
        entry.canvas.getContext("2d")!.putImageData(new ImageData(pixels, width, height), 0, 0);
        entry.tints.clear();
        entry.loading = false;
        entry.ready = true;
        if (entry.stale) this.fetch(frame);
        this.emit();
      })
      .catch(() => {
        entry.loading = false;
        // The frame may have been deleted; forget it.
        if (this.entries.get(frame) === entry) this.entries.delete(frame);
      });
  }

  private emit() {
    for (const fn of this.listeners) fn();
  }
}
