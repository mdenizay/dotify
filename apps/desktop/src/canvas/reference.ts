// Reference image shown behind (or over) the canvas. View-only: it is never
// part of the document, its pixels or its exports. Settings are kept per
// project in localStorage; the image file itself is read through the
// engine, which only serves files the user picked in a native dialog.

import { engine } from "../engine/bridge";

export interface ReferenceSettings {
  /** Absolute path of the picked image. */
  path: string;
  visible: boolean;
  /** Draw over the layers instead of behind them. */
  onTop: boolean;
  /** 0–100 */
  opacity: number;
  /** Top-left position, in canvas pixels. */
  x: number;
  y: number;
  /** Canvas pixels per image pixel. */
  scale: number;
}

/** Largest scale that fits the whole image inside the canvas, centred. */
export function fitReference(imgW: number, imgH: number, docW: number, docH: number) {
  const scale = Math.min(docW / imgW, docH / imgH);
  return { scale, x: (docW - imgW * scale) / 2, y: (docH - imgH * scale) / 2 };
}

/** Scale by `factor` around the centre of the image. */
export function rescaleAround(
  r: Pick<ReferenceSettings, "x" | "y" | "scale">,
  imgW: number,
  imgH: number,
  scale: number,
) {
  const cx = r.x + (imgW * r.scale) / 2;
  const cy = r.y + (imgH * r.scale) / 2;
  return { scale, x: cx - (imgW * scale) / 2, y: cy - (imgH * scale) / 2 };
}

const STORAGE_KEY = "dotify.references.v1";

function readAll(): Record<string, ReferenceSettings> {
  try {
    return JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "{}") as Record<
      string,
      ReferenceSettings
    >;
  } catch {
    return {};
  }
}

export function loadReferenceSettings(projectKey: string): ReferenceSettings | null {
  return readAll()[projectKey] ?? null;
}

export function saveReferenceSettings(projectKey: string, settings: ReferenceSettings | null) {
  try {
    const others = Object.entries(readAll()).filter(([k]) => k !== projectKey);
    const all = Object.fromEntries(settings ? [...others, [projectKey, settings]] : others);
    localStorage.setItem(STORAGE_KEY, JSON.stringify(all));
  } catch {
    // Storage may be unavailable; the reference simply won't be remembered.
  }
}

/** Decoded reference images by path, shared by the canvas and the panel. */
class ReferenceImages {
  private images = new Map<string, HTMLImageElement>();
  private pending = new Map<string, Promise<HTMLImageElement>>();
  private listeners = new Set<() => void>();

  get(path: string): HTMLImageElement | null {
    return this.images.get(path) ?? null;
  }

  load(path: string): Promise<HTMLImageElement> {
    const ready = this.images.get(path);
    if (ready) return Promise.resolve(ready);
    let p = this.pending.get(path);
    if (!p) {
      p = engine.referenceLoad(path).then(
        (bytes) =>
          new Promise<HTMLImageElement>((resolve, reject) => {
            const img = new Image();
            img.onload = () => {
              this.images.set(path, img);
              this.pending.delete(path);
              this.listeners.forEach((fn) => fn());
              resolve(img);
            };
            img.onerror = () => {
              this.pending.delete(path);
              reject(new Error("The reference image could not be decoded."));
            };
            img.src = URL.createObjectURL(new Blob([bytes]));
          }),
        (e: unknown) => {
          this.pending.delete(path);
          throw e;
        },
      );
      this.pending.set(path, p);
    }
    return p;
  }

  onLoad(fn: () => void): () => void {
    this.listeners.add(fn);
    return () => {
      this.listeners.delete(fn);
    };
  }
}

export const referenceImages = new ReferenceImages();
