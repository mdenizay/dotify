import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { message } from "@tauri-apps/plugin-dialog";
import { CelCache } from "../canvas/CelCache";
import type { CanvasView } from "../canvas/CanvasView";
import { FrameStore } from "../canvas/FrameStore";
import { Player, resolveRange } from "../canvas/playback";
import { engine, errorMessage } from "../engine/bridge";
import type { Command, FrameId, GodotOptions, Outcome, SessionView } from "../engine/types";
import { getEditor, selectFrames, setEditor, toast, useEditor } from "../state/editor";

/**
 * Coordinates the engine, the render cache and the stores. All engine calls
 * run through one queue so commands and their read-backs stay ordered.
 */
class Controller {
  readonly cache = new CelCache();
  /** Engine composites, read through the queue so they follow edits. */
  readonly frames = new FrameStore((f) => this.enqueue(() => engine.readFrame(f)));
  /** Timeline playback, shown on the canvas. */
  readonly player = new Player(durations);
  /** Independent loop in the preview panel; keeps running while editing. */
  readonly previewPlayer = new Player(durations);
  view: CanvasView | null = null;
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
    if (reset || sizeChanged) {
      this.cache.reset(view.summary.width, view.summary.height);
      this.frames.reset(view.summary.width, view.summary.height);
    } else this.frames.retain(frames.map((f) => f.id));
    if (reset) {
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
    if (reset || sizeChanged || !frameOk) await this.loadFrame();
    if (reset) this.view?.fitToWindow();
    if (reset && getEditor().previewPlaying && getEditor().previewOpen) this.startPreview();
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
    // Visibility, opacity and frame changes can alter any composite.
    if (o.structure_changed) this.frames.invalidate();
    else this.frames.invalidate(o.dirty.map((d) => d.frame));
    await this.adopt(await engine.view(), false);
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
      if (d.frame !== frame) continue;
      this.cache.putRegion(d.layer, d.rect, await engine.readCel(d.layer, d.frame, d.rect));
    }
    this.view?.invalidate();
  }

  /** Execute a command through the shared engine. Errors become toasts. */
  run = (command: Command): Promise<Outcome | null> => {
    this.stopPlayback();
    return this.enqueue(async () => {
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
  };

  undo = () => {
    this.stopPlayback();
    return this.enqueue(async () => {
      const label = getEditor().session?.summary.undo_label;
      const o = await engine.undo();
      await this.applyOutcome(o);
      if (o && label) toast(`Undo ${label}`);
    });
  };

  redo = () => {
    this.stopPlayback();
    return this.enqueue(async () => {
      const label = getEditor().session?.summary.redo_label;
      const o = await engine.redo();
      await this.applyOutcome(o);
      if (o && label) toast(`Redo ${label}`);
    });
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
    const frame = getEditor().activeFrameId;
    if (frame == null) return;
    const path = await this.guard(() => engine.exportPng(frame, scale));
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
      case "export_godot":
        return setEditor({ exportOpen: true, exportKind: "godot" });
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

export function isTyping(): boolean {
  const el = document.activeElement as HTMLElement | null;
  return !!el && (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.isContentEditable);
}

export const controller = new Controller();
