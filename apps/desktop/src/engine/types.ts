// TypeScript mirrors of the Rust engine types. Keep in sync with
// crates/dotify-core/src/{command,editor,model}.rs and
// apps/desktop/src-tauri/src/{commands,session}.rs.

/** `#rrggbbaa`, straight alpha. */
export type Hex = string;
export type LayerId = number;
export type FrameId = number;

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

export type Direction = "forward" | "reverse" | "ping_pong";

export interface AnimationTag {
  name: string;
  from: FrameId;
  to: FrameId;
  direction: Direction;
  loop: boolean;
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

export interface SessionView {
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
  | { type: "copy_frame"; from: FrameId; to: FrameId; layer?: LayerId | null }
  | {
      type: "copy_silhouette";
      from: FrameId;
      to: FrameId;
      color: Hex;
      name?: string | null;
      index?: number | null;
      opacity?: number | null;
      locked?: boolean;
    }
  | {
      type: "create_tag";
      name: string;
      from: FrameId;
      to: FrameId;
      direction?: Direction | null;
      loop?: boolean | null;
      color?: Hex | null;
    }
  | {
      type: "update_tag";
      tag: string;
      name?: string | null;
      from?: FrameId | null;
      to?: FrameId | null;
      direction?: Direction | null;
      loop?: boolean | null;
      color?: Hex | null;
    }
  | { type: "delete_tag"; tag: string }
  | { type: "set_palette"; palette: Palette }
  | { type: "rename_document"; name: string }
  | { type: "batch"; label?: string | null; commands: Command[] };

export type Created = { layer: LayerId } | { frame: FrameId };

export interface DirtyRegion {
  layer: LayerId;
  frame: FrameId;
  rect: Rect;
}

export interface Outcome {
  revision: number;
  changed: boolean;
  structure_changed: boolean;
  dirty: DirtyRegion[];
  created: Created[];
  label: string | null;
}

// Exporters (crates/dotify-export).

export type SheetLayout = "grid" | "rows_per_tag";

export interface SheetOptions {
  layout: SheetLayout;
  /** null = near-square grid */
  columns: number | null;
  scale: number;
  padding: number;
  extrude: number;
}

export interface GifOptions {
  scale: number;
  /** null = every frame, forward, looping */
  tag: string | null;
}

export interface GifExport {
  path: string;
  report: { frames: number; colors: number; alpha_rounded_pixels: number };
}

export interface GodotOptions {
  name?: string | null;
  sheet: SheetOptions;
  res_dir?: string | null;
}

export interface GodotFiles {
  png: string;
  tres: string;
  json: string;
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
