import { invoke } from "@tauri-apps/api/core";
import type {
  ArtboardId,
  EngineError,
  ExportAllResult,
  FrameId,
  LayerId,
  Outcome,
  ProjectCommand,
  Rect,
  RecoveryInfo,
  SessionView,
} from "./types";

/** Typed wrappers over the Tauri IPC commands in src-tauri/src/commands.rs. */
export const engine = {
  view: () => invoke<SessionView>("doc_view"),
  newDocument: (name: string, width: number, height: number) =>
    invoke<SessionView>("doc_new", { name, width, height }),
  open: () => invoke<SessionView | null>("doc_open"),
  save: () => invoke<SessionView | null>("doc_save"),
  saveAs: () => invoke<SessionView | null>("doc_save_as"),
  execute: (command: ProjectCommand, expectedRevision?: number) =>
    invoke<Outcome>("execute", { command, expectedRevision: expectedRevision ?? null }),
  undo: () => invoke<Outcome | null>("undo"),
  redo: () => invoke<Outcome | null>("redo"),
  activate: (artboard: ArtboardId) => invoke<SessionView>("artboard_activate", { artboard }),
  async readCel(
    artboard: ArtboardId,
    layer: LayerId,
    frame: FrameId,
    rect?: Rect,
  ): Promise<Uint8ClampedArray> {
    const buf = await invoke<ArrayBuffer>("read_cel", {
      artboard,
      layer,
      frame,
      rect: rect ?? null,
    });
    return new Uint8ClampedArray(buf);
  },
  /** Composited RGBA of an artboard frame (first frame by default). */
  async composite(artboard: ArtboardId, frame?: FrameId): Promise<Uint8ClampedArray> {
    const buf = await invoke<ArrayBuffer>("artboard_composite", { artboard, frame: frame ?? null });
    return new Uint8ClampedArray(buf);
  },
  exportPng: (artboard: ArtboardId, frame: FrameId, scale: number) =>
    invoke<string | null>("export_png", { artboard, frame, scale }),
  exportAllPng: (scale: number) => invoke<ExportAllResult | null>("export_all_png", { scale }),
  recoveryStatus: () => invoke<RecoveryInfo | null>("recovery_status"),
  recoveryRestore: () => invoke<SessionView>("recovery_restore"),
  recoveryDiscard: () => invoke<null>("recovery_discard"),
};

export function isEngineError(e: unknown): e is EngineError {
  return typeof e === "object" && e !== null && "code" in e && "message" in e;
}

export function errorMessage(e: unknown): string {
  if (isEngineError(e)) return e.message;
  if (e instanceof Error) return e.message;
  return String(e);
}
