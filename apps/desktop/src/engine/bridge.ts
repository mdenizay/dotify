import { invoke } from "@tauri-apps/api/core";
import type {
  Command,
  EngineError,
  FrameId,
  LayerId,
  Outcome,
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
  execute: (command: Command, expectedRevision?: number) =>
    invoke<Outcome>("execute", { command, expectedRevision: expectedRevision ?? null }),
  undo: () => invoke<Outcome | null>("undo"),
  redo: () => invoke<Outcome | null>("redo"),
  async readCel(layer: LayerId, frame: FrameId, rect?: Rect): Promise<Uint8ClampedArray> {
    const buf = await invoke<ArrayBuffer>("read_cel", { layer, frame, rect: rect ?? null });
    return new Uint8ClampedArray(buf);
  },
  exportPng: (frame: FrameId, scale: number) =>
    invoke<string | null>("export_png", { frame, scale }),
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
