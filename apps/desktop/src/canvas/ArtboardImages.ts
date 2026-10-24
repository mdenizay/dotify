import type { ArtboardId, ArtboardSummary } from "../engine/types";

type Loader = (id: ArtboardId) => Promise<Uint8ClampedArray>;

/**
 * Composited first-frame images of every artboard, shared by the library
 * thumbnails and the overview. Reloads are debounced so a stroke on the
 * active artboard does not composite the whole canvas on every commit.
 */
export class ArtboardImages {
  private images = new Map<ArtboardId, HTMLCanvasElement>();
  private sizes = new Map<ArtboardId, { width: number; height: number }>();
  private stale = new Set<ArtboardId>();
  private timer: ReturnType<typeof setTimeout> | undefined;
  private listeners = new Set<(id: ArtboardId | null) => void>();

  constructor(private load: Loader) {}

  get(id: ArtboardId): HTMLCanvasElement | undefined {
    return this.images.get(id);
  }

  /** Track the current artboards: drop removed ones, load new or resized ones. */
  sync(artboards: ArtboardSummary[]) {
    const ids = new Set(artboards.map((a) => a.id));
    for (const id of [...this.images.keys()]) {
      if (!ids.has(id)) {
        this.images.delete(id);
        this.sizes.delete(id);
      }
    }
    const fresh: ArtboardId[] = [];
    for (const a of artboards) {
      const size = this.sizes.get(a.id);
      if (!size || size.width !== a.width || size.height !== a.height) {
        this.sizes.set(a.id, { width: a.width, height: a.height });
        fresh.push(a.id);
      }
    }
    if (fresh.length) this.invalidate(fresh, 0);
  }

  /** Forget everything (a different project was opened). */
  reset(artboards: ArtboardSummary[]) {
    this.images.clear();
    this.sizes.clear();
    this.stale.clear();
    this.sync(artboards);
    this.emit(null);
  }

  invalidate(ids: ArtboardId[], delay = 350) {
    for (const id of ids) if (this.sizes.has(id)) this.stale.add(id);
    if (!this.stale.size) return;
    clearTimeout(this.timer);
    this.timer = setTimeout(() => void this.flush(), delay);
  }

  private async flush() {
    const ids = [...this.stale];
    this.stale.clear();
    for (const id of ids) {
      const size = this.sizes.get(id);
      if (!size) continue;
      let data: Uint8ClampedArray;
      try {
        data = await this.load(id);
      } catch {
        continue; // Deleted meanwhile; the next sync drops it.
      }
      if (data.length !== size.width * size.height * 4) continue;
      const canvas = this.images.get(id) ?? document.createElement("canvas");
      canvas.width = size.width;
      canvas.height = size.height;
      const image = new ImageData(size.width, size.height);
      image.data.set(data);
      canvas.getContext("2d")!.putImageData(image, 0, 0);
      this.images.set(id, canvas);
      this.emit(id);
    }
  }

  onChange(fn: (id: ArtboardId | null) => void): () => void {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  }

  private emit(id: ArtboardId | null) {
    for (const fn of this.listeners) fn(id);
  }
}
