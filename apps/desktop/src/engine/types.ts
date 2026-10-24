// TypeScript mirrors of the Rust engine types. Keep in sync with
// crates/dotify-core/src/{command,editor,model}.rs,
// crates/dotify-core/src/project/{command,editor,mod}.rs and
// apps/desktop/src-tauri/src/{commands,session}.rs.

/** `#rrggbbaa`, straight alpha. */
export type Hex = string;
export type LayerId = number;
export type FrameId = number;
export type ArtboardId = number;
export type FolderId = number;

export interface Point {
  x: number;
  y: number;
}

export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface Layer {
  id: LayerId;
  name: string;
  kind: "pixel";
  visible: boolean;
  locked: boolean;
  /** 0–255 */
  opacity: number;
  blend_mode: "normal";
}

export interface Frame {
  id: FrameId;
  duration_ms: number;
}

export interface Palette {
  name: string;
  colors: Hex[];
}

export interface AnimationTag {
  name: string;
  from: FrameId;
  to: FrameId;
  direction: "forward" | "reverse" | "ping_pong";
  color: Hex;
}

export interface DocumentSummary {
  name: string;
  width: number;
  height: number;
  layers: Layer[];
  frames: Frame[];
  palette: Palette;
  tags: AnimationTag[];
  revision: number;
  dirty: boolean;
  can_undo: boolean;
  can_redo: boolean;
  undo_label: string | null;
  redo_label: string | null;
}

export interface Folder {
  id: FolderId;
  name: string;
}

/** Library entry of one artboard (no pixel data). */
export interface ArtboardSummary {
  id: ArtboardId;
  name: string;
  /** `null` = not in any folder. */
  folder: FolderId | null;
  width: number;
  height: number;
  layers: Layer[];
  frames: Frame[];
  tags: AnimationTag[];
}

export interface ProjectSummary {
  name: string;
  palette: Palette;
  folders: Folder[];
  /** Library order. */
  artboards: ArtboardSummary[];
  revision: number;
  dirty: boolean;
  can_undo: boolean;
  can_redo: boolean;
  undo_label: string | null;
  redo_label: string | null;
}

export interface SessionView {
  project: ProjectSummary;
  active_artboard: ArtboardId;
  /** The active artboard, with the project's history state. */
  summary: DocumentSummary;
  path: string | null;
  file_name: string;
}

export interface PixelWrite {
  x: number;
  y: number;
  color: Hex;
}

type Cel = { layer: LayerId; frame: FrameId };
type Box = { x: number; y: number; width: number; height: number };

export type Command =
  | ({ type: "set_pixels"; pixels: PixelWrite[] } & Cel)
  | ({ type: "draw_line"; from: Point; to: Point; color: Hex } & Cel)
  | ({ type: "draw_rectangle"; color: Hex; filled?: boolean } & Cel & Box)
  | ({ type: "draw_ellipse"; color: Hex; filled?: boolean } & Cel & Box)
  | ({ type: "fill_region"; x: number; y: number; color: Hex; contiguous?: boolean } & Cel)
  | ({ type: "replace_color"; from: Hex; to: Hex } & Cel)
  | ({ type: "clear_cel" } & Cel)
  | { type: "create_layer"; name?: string | null; index?: number | null }
  | { type: "duplicate_layer"; layer: LayerId }
  | { type: "delete_layer"; layer: LayerId }
  | {
      type: "update_layer";
      layer: LayerId;
      name?: string | null;
      visible?: boolean | null;
      locked?: boolean | null;
      opacity?: number | null;
    }
  | { type: "move_layer"; layer: LayerId; index: number }
  | { type: "add_frame"; index?: number | null; duration_ms?: number | null }
  | { type: "duplicate_frame"; frame: FrameId }
  | { type: "delete_frame"; frame: FrameId }
  | { type: "move_frame"; frame: FrameId; index: number }
  | { type: "set_frame_duration"; frame: FrameId; duration_ms: number }
  | { type: "set_palette"; palette: Palette }
  | { type: "rename_document"; name: string }
  | { type: "batch"; label?: string | null; commands: Command[] };

export type ProjectCommand =
  | {
      type: "create_artboard";
      name?: string | null;
      width: number;
      height: number;
      folder?: FolderId | null;
      index?: number | null;
    }
  | { type: "rename_artboard"; artboard: ArtboardId; name: string }
  | { type: "duplicate_artboard"; artboard: ArtboardId }
  | { type: "delete_artboard"; artboard: ArtboardId }
  | { type: "move_artboard"; artboard: ArtboardId; index: number; folder: FolderId | null }
  | { type: "create_folder"; name?: string | null; index?: number | null }
  | { type: "rename_folder"; folder: FolderId; name: string }
  | { type: "delete_folder"; folder: FolderId }
  | { type: "set_palette"; palette: Palette }
  | { type: "rename_project"; name: string }
  | { type: "edit"; artboard: ArtboardId; command: Command }
  | { type: "batch"; label?: string | null; commands: ProjectCommand[] };

export type Created =
  { layer: LayerId } | { frame: FrameId } | { artboard: ArtboardId } | { folder: FolderId };

export interface DirtyRegion {
  artboard: ArtboardId;
  layer: LayerId;
  frame: FrameId;
  rect: Rect;
}

export interface Outcome {
  revision: number;
  changed: boolean;
  structure_changed: boolean;
  dirty: DirtyRegion[];
  /** Artboards whose content changed. */
  artboards: ArtboardId[];
  created: Created[];
  label: string | null;
}

export interface ExportAllResult {
  dir: string;
  files: number;
}

export interface RecoveryInfo {
  name: string;
  original_path: string | null;
  saved_at: number;
}

export interface EngineError {
  code: string;
  message: string;
}
