import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { message } from "@tauri-apps/plugin-dialog";
import { CelCache } from "../canvas/CelCache";
import type { CanvasView } from "../canvas/CanvasView";
import { engine, errorMessage } from "../engine/bridge";
import type { Command, FrameId, Outcome, SessionView } from "../engine/types";
import { getEditor, setEditor, toast } from "../state/editor";

/**
 * Coordinates the engine, the render cache and the stores. All engine calls
 * run through one queue so commands and their read-backs stay ordered.
 */
class Controller {
  readonly cache = new CelCache();
  view: CanvasView | null = null;
  private queue: Promise<unknown> = Promise.resolve();
  private initialized: Promise<void> | null = null;

  private enqueue<T>(fn: () => Promise<T>): Promise<T> {
    const p = this.queue.then(fn);
    this.queue = p.catch(() => undefined);
    return p;
  }

  /** Idempotent: React StrictMode runs mount effects twice in development. */
  init(): Promise<void> {
    this.initialized ??= this.start();
    return this.initialized;
  }

  private async start() {
    await this.enqueue(async () => this.adopt(await engine.view(), true));
    const recovery = await engine.recoveryStatus();
    if (recovery) setEditor({ recovery });
    await listen<string>("menu", (e) => void this.menu(e.payload));
    await getCurrentWindow().onCloseRequested(async (e) => {
      if (!(await this.confirmDiscard())) {
        e.preventDefault();
        return;
      }
      await engine.recoveryDiscard();
    });
  }

  /** Install a session from the engine; `reset` reloads all pixels. */
  private async adopt(view: SessionView, reset: boolean) {
    const s = getEditor();
    const { layers, frames } = view.summary;
    const prev = s.session?.summary;
    const layerOk = layers.some((l) => l.id === s.activeLayerId);
    const frameOk = frames.some((f) => f.id === s.activeFrameId);
    const sizeChanged =
      !prev || prev.width !== view.summary.width || prev.height !== view.summary.height;
    setEditor({
      session: view,
      activeLayerId: !reset && layerOk ? s.activeLayerId : (layers.at(-1)?.id ?? null),
      activeFrameId: !reset && frameOk ? s.activeFrameId : (frames[0]?.id ?? null),
    });
    if (reset || sizeChanged) this.cache.reset(view.summary.width, view.summary.height);
    if (reset || sizeChanged || !frameOk) await this.loadFrame();
    if (reset) this.view?.fitToWindow();
  }

  /** Read every layer of the active frame from the engine. */
  private async loadFrame() {
    const { session, activeFrameId } = getEditor();
    if (!session || activeFrameId == null) return;
    const { width, height, layers } = session.summary;
    const rect = { x: 0, y: 0, width, height };
    for (const l of layers) {
      this.cache.putRegion(l.id, rect, await engine.readCel(l.id, activeFrameId));
    }
    this.cache.retain(layers.map((l) => l.id));
    this.view?.invalidate();
  }

  private async applyOutcome(o: Outcome | null) {
    if (!o?.changed) return;
    await this.adopt(await engine.view(), false);
    const createdLayer = o.created.find((c) => "layer" in c);
    const createdFrame = o.created.find((c) => "frame" in c);
    if (createdLayer && "layer" in createdLayer) setEditor({ activeLayerId: createdLayer.layer });
    if (createdFrame && "frame" in createdFrame) setEditor({ activeFrameId: createdFrame.frame });
    if (o.structure_changed || createdFrame) {
      await this.loadFrame();
      return;
    }
    const frame = getEditor().activeFrameId;
    for (const d of o.dirty) {
      if (d.frame !== frame) continue;
      this.cache.putRegion(d.layer, d.rect, await engine.readCel(d.layer, d.frame, d.rect));
    }
    this.view?.invalidate();
  }

  /** Execute a command through the shared engine. Errors become toasts. */
  run = (command: Command): Promise<Outcome | null> =>
    this.enqueue(async () => {
      try {
        const o = await engine.execute(command);
        await this.applyOutcome(o);
        return o;
      } catch (e) {
        toast(errorMessage(e), "error");
        // The local preview may have diverged; resynchronise.
        await this.loadFrame();
        return null;
      }
    });

  undo = () =>
    this.enqueue(async () => {
      const label = getEditor().session?.summary.undo_label;
      const o = await engine.undo();
      await this.applyOutcome(o);
      if (o && label) toast(`Undo ${label}`);
    });

  redo = () =>
    this.enqueue(async () => {
      const label = getEditor().session?.summary.redo_label;
      const o = await engine.redo();
      await this.applyOutcome(o);
      if (o && label) toast(`Redo ${label}`);
    });

  selectFrame = (frame: FrameId) => {
    if (getEditor().activeFrameId === frame) return;
    setEditor({ activeFrameId: frame });
    return this.enqueue(() => this.loadFrame());
  };

  // ── Files ────────────────────────────────────────────────────────────

  /** Ask to save unsaved changes. Resolves false if the user cancels. */
  async confirmDiscard(): Promise<boolean> {
    const session = getEditor().session;
    if (!session?.summary.dirty) return true;
    const answer = await message(
      `Do you want to save the changes made to “${session.file_name}”?`,
      {
        title: "Unsaved Changes",
        kind: "warning",
        buttons: { yes: "Save", no: "Don’t Save", cancel: "Cancel" },
      },
    );
    if (answer === "Save" || answer === "Yes") return (await this.save()) !== null;
    return answer === "Don’t Save" || answer === "No";
  }

  async newDocument(name: string, width: number, height: number) {
    if (!(await this.confirmDiscard())) return;
    await this.guard(() =>
      this.enqueue(async () => this.adopt(await engine.newDocument(name, width, height), true)),
    );
  }

  async open() {
    if (!(await this.confirmDiscard())) return;
    await this.guard(() =>
      this.enqueue(async () => {
        const v = await engine.open();
        if (v) await this.adopt(v, true);
      }),
    );
  }

  save = () => this.saveWith(engine.save);
  saveAs = () => this.saveWith(engine.saveAs);

  private async saveWith(fn: () => Promise<SessionView | null>) {
    return this.guard(() =>
      this.enqueue(async () => {
        const v = await fn();
        if (v) {
          setEditor({ session: v });
          toast(`Saved ${v.file_name}`);
        }
        return v;
      }),
    );
  }

  async exportPng(scale = 1) {
    const frame = getEditor().activeFrameId;
    if (frame == null) return;
    const path = await this.guard(() => engine.exportPng(frame, scale));
    if (path) toast(`Exported ${path.split("/").pop()}`);
  }

  async restoreRecovery() {
    setEditor({ recovery: null });
    await this.guard(() =>
      this.enqueue(async () => this.adopt(await engine.recoveryRestore(), true)),
    );
  }

  async discardRecovery() {
    setEditor({ recovery: null });
    await engine.recoveryDiscard();
  }

  private async guard<T>(fn: () => Promise<T>): Promise<T | null> {
    try {
      return await fn();
    } catch (e) {
      toast(errorMessage(e), "error");
      return null;
    }
  }

  // ── Menu ─────────────────────────────────────────────────────────────

  async menu(id: string) {
    const typing = isTyping();
    switch (id) {
      case "new":
        return setEditor({ newDocOpen: true });
      case "open":
        return this.open();
      case "save":
        return this.save();
      case "save_as":
        return this.saveAs();
      case "export_png":
        return setEditor({ exportOpen: true });
      case "undo":
        // Native text fields keep their own undo stack.
        return typing ? document.execCommand("undo") : this.undo();
      case "redo":
        return typing ? document.execCommand("redo") : this.redo();
      case "zoom_in":
        return this.view?.zoomStep(1);
      case "zoom_out":
        return this.view?.zoomStep(-1);
      case "zoom_actual":
        return this.view?.actualSize();
      case "zoom_fit":
        return this.view?.fitToWindow();
      case "toggle_grid":
        return setEditor((s) => ({ showGrid: !s.showGrid }));
      case "toggle_theme":
        return setEditor((s) => ({ theme: s.theme === "dark" ? "light" : "dark" }));
      case "close":
      case "quit":
        return getCurrentWindow().close();
    }
  }
}

export function isTyping(): boolean {
  const el = document.activeElement as HTMLElement | null;
  return !!el && (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.isContentEditable);
}

export const controller = new Controller();
