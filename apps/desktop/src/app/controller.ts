import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { message } from "@tauri-apps/plugin-dialog";
import { ArtboardImages } from "../canvas/ArtboardImages";
import { CelCache } from "../canvas/CelCache";
import type { CanvasView } from "../canvas/CanvasView";
import type { OverviewView } from "../canvas/OverviewView";
import { engine, errorMessage } from "../engine/bridge";
import type {
  ArtboardId,
  Command,
  FrameId,
  Outcome,
  ProjectCommand,
  SessionView,
} from "../engine/types";
import { getEditor, setEditor, toast, type Mode } from "../state/editor";

/**
 * Coordinates the engine, the render cache and the stores. All engine calls
 * run through one queue so commands and their read-backs stay ordered.
 */
class Controller {
  readonly cache = new CelCache();
  /** Composited artboards for the library and overview. */
  readonly images = new ArtboardImages((id) => this.enqueue(() => engine.composite(id)));
  view: CanvasView | null = null;
  overview: OverviewView | null = null;
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

  /**
   * Install a session from the engine. `reset` means a different project:
   * all pixels and thumbnails reload. Switching the active artboard reloads
   * the editor's pixels too.
   */
  private async adopt(view: SessionView, reset: boolean) {
    const s = getEditor();
    const { layers, frames } = view.summary;
    const prev = s.session;
    const switched = reset || !prev || prev.active_artboard !== view.active_artboard;
    const layerOk = layers.some((l) => l.id === s.activeLayerId);
    const frameOk = frames.some((f) => f.id === s.activeFrameId);
    const sizeChanged =
      !prev ||
      prev.summary.width !== view.summary.width ||
      prev.summary.height !== view.summary.height;
    setEditor({
      session: view,
      activeLayerId: !switched && layerOk ? s.activeLayerId : (layers.at(-1)?.id ?? null),
      activeFrameId: !switched && frameOk ? s.activeFrameId : (frames[0]?.id ?? null),
    });
    if (reset) this.images.reset(view.project.artboards);
    else this.images.sync(view.project.artboards);
    if (switched || sizeChanged) this.cache.reset(view.summary.width, view.summary.height);
    if (switched || sizeChanged || !frameOk) await this.loadFrame();
    if (switched) this.view?.fitToWindow();
    if (reset) this.overview?.fitToWindow();
  }

  /** Read every layer of the active frame from the engine. */
  private async loadFrame() {
    const { session, activeFrameId } = getEditor();
    if (!session || activeFrameId == null) return;
    const artboard = session.active_artboard;
    const { width, height, layers } = session.summary;
    const rect = { x: 0, y: 0, width, height };
    for (const l of layers) {
      this.cache.putRegion(l.id, rect, await engine.readCel(artboard, l.id, activeFrameId));
    }
    this.cache.retain(layers.map((l) => l.id));
    this.view?.invalidate();
  }

  private async applyOutcome(o: Outcome | null, follow = false) {
    if (!o?.changed) return;
    let view = await engine.view();
    // Undo/redo of an edit on another artboard: show that artboard.
    const touched = o.artboards.filter((id) => view.project.artboards.some((a) => a.id === id));
    if (follow && touched.length && !touched.includes(view.active_artboard)) {
      view = await engine.activate(touched[0]!);
    }
    const before = getEditor().session?.active_artboard;
    await this.adopt(view, false);
    this.images.invalidate(o.artboards);
    if (before !== view.active_artboard) return;
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
      if (d.frame !== frame || d.artboard !== view.active_artboard) continue;
      const pixels = await engine.readCel(d.artboard, d.layer, d.frame, d.rect);
      this.cache.putRegion(d.layer, d.rect, pixels);
    }
    this.view?.invalidate();
  }

  /** Execute a document command on the active artboard. Errors become toasts. */
  run = (command: Command): Promise<Outcome | null> => {
    const artboard = getEditor().session?.active_artboard;
    if (artboard == null) return Promise.resolve(null);
    return this.runProject({ type: "edit", artboard, command });
  };

  /** Execute a project command through the shared engine. Errors become toasts. */
  runProject = (command: ProjectCommand): Promise<Outcome | null> =>
    this.enqueue(async () => {
      try {
        const o = await engine.execute(command);
        await this.applyOutcome(o);
        const created = o.created.find((c) => "artboard" in c);
        if (created && "artboard" in created) await this.activateNow(created.artboard);
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
      await this.applyOutcome(o, true);
      if (o && label) toast(`Undo ${label}`);
    });

  redo = () =>
    this.enqueue(async () => {
      const label = getEditor().session?.summary.redo_label;
      const o = await engine.redo();
      await this.applyOutcome(o, true);
      if (o && label) toast(`Redo ${label}`);
    });

  // ── Library ──────────────────────────────────────────────────────────

  private async activateNow(artboard: ArtboardId) {
    if (getEditor().session?.active_artboard === artboard) return;
    await this.adopt(await engine.activate(artboard), false);
  }

  /** Make an artboard active; `edit` also switches to the editor. */
  selectArtboard = (artboard: ArtboardId, edit = false) => {
    if (edit) setEditor({ mode: "editor" });
    return this.guard(() => this.enqueue(() => this.activateNow(artboard)));
  };

  setMode = (mode: Mode) => {
    setEditor({ mode });
    if (mode === "editor") this.view?.invalidate();
  };

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
    const { activeFrameId: frame, session } = getEditor();
    if (frame == null || !session) return;
    const path = await this.guard(() => engine.exportPng(session.active_artboard, frame, scale));
    if (path) toast(`Exported ${path.split("/").pop()}`);
  }

  /** One PNG per artboard into a folder, mirroring library folders. */
  async exportAll(scale = 1) {
    const result = await this.guard(() => engine.exportAllPng(scale));
    if (result) toast(`Exported ${result.files} artboards to ${result.dir.split("/").pop()}`);
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

  /** The canvas that view commands (zoom, fit) apply to. */
  activeView(): CanvasView | OverviewView | null {
    return getEditor().mode === "overview" ? this.overview : this.view;
  }

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
      case "export_all":
        return setEditor({ exportAllOpen: true });
      case "toggle_overview":
        return this.setMode(getEditor().mode === "overview" ? "editor" : "overview");
      case "toggle_library":
        return setEditor((s) => ({ libraryOpen: !s.libraryOpen }));
      case "undo":
        // Native text fields keep their own undo stack.
        return typing ? document.execCommand("undo") : this.undo();
      case "redo":
        return typing ? document.execCommand("redo") : this.redo();
      case "zoom_in":
        return this.activeView()?.zoomStep(1);
      case "zoom_out":
        return this.activeView()?.zoomStep(-1);
      case "zoom_actual":
        return this.activeView()?.actualSize();
      case "zoom_fit":
        return this.activeView()?.fitToWindow();
      case "toggle_grid":
        return setEditor((s) => ({ showGrid: !s.showGrid }));
      case "toggle_theme":
        return setEditor((s) => ({ theme: s.theme === "dark" ? "light" : "dark" }));
      case "check_updates": {
        const { checkForUpdates } = await import("./updates");
        return checkForUpdates({ interactive: true });
      }
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
