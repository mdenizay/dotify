import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { message } from "@tauri-apps/plugin-dialog";
import { ArtboardImages } from "../canvas/ArtboardImages";
import { CelCache } from "../canvas/CelCache";
import type { CanvasView } from "../canvas/CanvasView";
import { FrameStore } from "../canvas/FrameStore";
import type { OverviewView } from "../canvas/OverviewView";
import { Player, resolveRange } from "../canvas/playback";
import { engine, errorMessage } from "../engine/bridge";
import type {
  ArtboardId,
  Command,
  FrameId,
  GodotOptions,
  Outcome,
  ProjectCommand,
  SessionView,
} from "../engine/types";
import { getEditor, selectFrames, setEditor, toast, useEditor, type Mode } from "../state/editor";

/** Neutral mid-gray that reads on both themes and on most palettes. */
const SILHOUETTE_COLOR = "#8a8f98ff";
/** About 45%: visible enough to trace, faint enough to draw over. */
const SILHOUETTE_OPACITY = 115;

/**
 * Coordinates the engine, the render cache and the stores. All engine calls
 * run through one queue so commands and their read-backs stay ordered.
 */
class Controller {
  readonly cache = new CelCache();
  /** Composited artboards for the library and overview. */
  readonly images = new ArtboardImages((id) => this.enqueue(() => engine.composite(id)));
  /** Engine composites of the active artboard, read through the queue so they follow edits. */
  readonly frames = new FrameStore((f) =>
    this.enqueue(() => engine.readFrame(activeArtboard(), f)),
  );
  /** Timeline playback, shown on the canvas. */
  readonly player = new Player(durations);
  /** Independent loop in the preview panel; keeps running while editing. */
  readonly previewPlayer = new Player(durations);
  view: CanvasView | null = null;
  overview: OverviewView | null = null;
  private queue: Promise<unknown> = Promise.resolve();
  private initialized: Promise<void> | null = null;

  constructor() {
    this.player.onChange(() => {
      const playing = this.player.playing;
      if (playing === getEditor().playing) return;
      setEditor({ playing });
      // Land on the frame that was showing so editing continues there.
      const frame = selectFrames(getEditor())[this.player.index];
      if (!playing && frame) void this.selectFrame(frame.id);
    });
    // Restart the preview when what it should play changes.
    useEditor.subscribe((s, p) => {
      if (s.playTag !== p.playTag || s.playMode !== p.playMode) {
        if (this.player.playing) this.togglePlay();
        if (s.previewPlaying) this.startPreview();
      }
      if (s.previewPlaying !== p.previewPlaying || s.previewOpen !== p.previewOpen) {
        if (s.previewPlaying && s.previewOpen) this.startPreview();
        else this.previewPlayer.pause();
      }
    });
  }

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
    if (switched || sizeChanged) {
      this.cache.reset(view.summary.width, view.summary.height);
      this.frames.reset(view.summary.width, view.summary.height);
    } else this.frames.retain(frames.map((f) => f.id));
    if (switched) {
      this.player.pause();
      setEditor({ playTag: null, frameAnchor: null, tagDialog: null });
    } else {
      // Drop references to tags and frames that no longer exist.
      const s2 = getEditor();
      if (s2.playTag && !view.summary.tags.some((t) => t.name === s2.playTag))
        setEditor({ playTag: null });
      if (s2.frameAnchor != null && !frames.some((f) => f.id === s2.frameAnchor))
        setEditor({ frameAnchor: null });
    }
    if (switched || sizeChanged || !frameOk) await this.loadFrame();
    if (switched) this.view?.fitToWindow();
    if (reset) this.overview?.fitToWindow();
    if (switched && getEditor().previewPlaying && getEditor().previewOpen) this.startPreview();
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
    // Visibility, opacity and frame changes can alter any composite.
    if (before === view.active_artboard) {
      if (o.structure_changed) this.frames.invalidate();
      else this.frames.invalidate(o.dirty.filter((d) => d.artboard === before).map((d) => d.frame));
    }
    await this.adopt(view, false);
    this.images.invalidate(o.artboards);
    if (before !== view.active_artboard) return;
    // Keep the preview's range valid after frames or tags changed.
    if (o.structure_changed && this.previewPlayer.playing) this.startPreview();
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
  runProject = (command: ProjectCommand): Promise<Outcome | null> => {
    this.stopPlayback();
    return this.enqueue(async () => {
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
  };

  undo = () => {
    this.stopPlayback();
    return this.enqueue(async () => {
      const label = getEditor().session?.summary.undo_label;
      const o = await engine.undo();
      await this.applyOutcome(o, true);
      if (o && label) toast(`Undo ${label}`);
    });
  };

  redo = () => {
    this.stopPlayback();
    return this.enqueue(async () => {
      const label = getEditor().session?.summary.redo_label;
      const o = await engine.redo();
      await this.applyOutcome(o, true);
      if (o && label) toast(`Redo ${label}`);
    });
  };

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

  selectFrame = (frame: FrameId, extend = false) => {
    const s = getEditor();
    // Shift-click extends a range from the active frame; a plain click clears it.
    if (extend) return setEditor({ frameAnchor: frame === s.activeFrameId ? null : frame });
    if (s.frameAnchor != null) setEditor({ frameAnchor: null });
    if (s.activeFrameId === frame) return;
    setEditor({ activeFrameId: frame });
    return this.enqueue(() => this.loadFrame());
  };

  /**
   * Carry the active frame over to the next one (created if this is the last
   * frame), then move there. "pixels" copies every unlocked layer;
   * "silhouette" adds a faint, locked one-color layer to trace the next pose
   * over, and keeps drawing on the layer you were using.
   */
  copyToNextFrame = async (mode: "pixels" | "silhouette") => {
    const s = getEditor();
    const summary = s.session?.summary;
    if (!summary || s.activeFrameId == null) return;
    const from = s.activeFrameId;
    const index = summary.frames.findIndex((f) => f.id === from);
    let to = summary.frames[index + 1]?.id;
    if (to == null) {
      if (mode === "pixels") return void (await this.run({ type: "duplicate_frame", frame: from }));
      const added = await this.run({ type: "add_frame", index: index + 1 });
      const created = added?.created.find((c) => "frame" in c);
      if (!created || !("frame" in created)) return;
      to = created.frame;
    }
    if (mode === "pixels") {
      const o = await this.run({ type: "copy_frame", from, to });
      if (o) await this.selectFrame(to);
      return;
    }
    const drawingLayer = s.activeLayerId;
    const below = summary.layers.findIndex((l) => l.id === drawingLayer);
    const o = await this.run({
      type: "copy_silhouette",
      from,
      to,
      color: SILHOUETTE_COLOR,
      name: `Silhouette of frame ${index + 1}`,
      index: below < 0 ? 0 : below,
      opacity: SILHOUETTE_OPACITY,
      locked: true,
    });
    if (!o) return;
    if (drawingLayer != null) setEditor({ activeLayerId: drawingLayer });
    await this.selectFrame(to);
  };

  // ── Playback ─────────────────────────────────────────────────────────

  private range() {
    const s = getEditor();
    const summary = s.session?.summary;
    if (!summary) return null;
    return resolveRange(summary.frames, summary.tags, s.playTag, s.playMode);
  }

  togglePlay = () => {
    if (this.player.playing) return this.player.pause();
    const range = this.range();
    if (!range || range.from === range.to) return;
    const s = getEditor();
    const index = selectFrames(s).findIndex((f) => f.id === s.activeFrameId);
    setEditor({ frameAnchor: null });
    this.prefetch(range);
    this.player.play(range, index);
  };

  /** Start loading every composite of a range so playback doesn't stall. */
  private prefetch(range: { from: number; to: number }) {
    const frames = selectFrames(getEditor());
    for (const f of frames.slice(range.from, range.to + 1)) this.frames.get(f.id);
  }

  /** Pause timeline playback (before an edit) and select the shown frame. */
  stopPlayback = () => {
    if (this.player.playing) this.player.pause();
  };

  private startPreview() {
    const range = this.range();
    if (range) this.prefetch(range);
    if (range) this.previewPlayer.play(range);
    else this.previewPlayer.pause();
  }

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
    if (path) toast(`Exported ${baseName(path)}`);
  }

  async exportSheet() {
    const s = getEditor();
    const path = await this.guard(() =>
      engine.exportSheet({ ...s.exportSheet, scale: s.exportScale }),
    );
    if (path) toast(`Exported ${baseName(path)} and its .json`);
  }

  async exportGif(tag: string | null) {
    const out = await this.guard(() => engine.exportGif({ scale: getEditor().exportScale, tag }));
    if (!out) return;
    const rounded = out.report.alpha_rounded_pixels;
    if (rounded > 0)
      toast(
        `Exported ${baseName(out.path)}. GIF has no partial transparency: ${rounded} pixels were rounded.`,
      );
    else toast(`Exported ${baseName(out.path)}`);
  }

  async exportGodot() {
    const s = getEditor();
    const options: GodotOptions = {
      sheet: { ...s.exportSheet, scale: s.exportScale },
      res_dir: s.godotResDir.trim() || null,
    };
    const files = await this.guard(() => engine.exportGodot(options));
    if (files) toast(`Exported ${baseName(files.tres)} for Godot`);
  }

  /** Every artboard into a folder, mirroring library folders. */
  async exportAll(godot: boolean) {
    const s = getEditor();
    const options: GodotOptions | null = godot
      ? { sheet: { ...s.exportSheet, scale: s.exportScale }, res_dir: null }
      : null;
    const result = await this.guard(() => engine.exportAll(s.exportScale, options));
    if (result) toast(`Exported ${result.files} artboards to ${baseName(result.dir)}`);
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
      case "export_godot":
        return setEditor({ exportOpen: true, exportKind: "godot" });
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
      case "toggle_rulers":
        return setEditor((s) => ({ showRulers: !s.showRulers }));
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

function durations() {
  return selectFrames(getEditor()).map((f) => f.duration_ms);
}

function baseName(path: string) {
  return path.split(/[\\/]/).pop() ?? path;
}

function activeArtboard(): ArtboardId {
  return getEditor().session?.active_artboard ?? 0;
}

export function isTyping(): boolean {
  const el = document.activeElement as HTMLElement | null;
  return !!el && (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.isContentEditable);
}

export const controller = new Controller();
