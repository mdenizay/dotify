import { getCurrentWindow } from "@tauri-apps/api/window";
import { create } from "zustand";
import type { ReferenceSettings } from "../canvas/reference";
import type { OnionSkin } from "../canvas/onion";
import type { PlayMode } from "../canvas/playback";
import type {
  ArtboardSummary,
  Folder,
  AnimationTag,
  Frame,
  FrameId,
  Hex,
  Layer,
  LayerId,
  RecoveryInfo,
  SessionView,
  SheetOptions,
} from "../engine/types";

export type ToolId =
  "pencil" | "eraser" | "fill" | "line" | "rectangle" | "ellipse" | "eyedropper" | "hand";

export type Theme = "dark" | "light";

/** `editor` edits the active artboard; `overview` shows all artboards. */
export type Mode = "editor" | "overview";
export type ExportKind = "png" | "sheet" | "gif" | "godot";

/** The tag dialog: creating over a frame range, or editing a tag by name. */
export type TagDialog =
  { mode: "create"; from: FrameId; to: FrameId } | { mode: "edit"; tag: string } | null;

/** "paint" mirrors strokes across the axes; "guide" only shows where the mirror would land. */
export type SymmetryMode = "paint" | "guide";

interface Prefs {
  theme: Theme;
  showGrid: boolean;
  showRulers: boolean;
  primary: Hex;
  secondary: Hex;
  brushSize: number;
  filledShapes: boolean;
  fillContiguous: boolean;
  rightPanelWidth: number;
  timelineOpen: boolean;
  previewOpen: boolean;
  exportScale: number;
  libraryOpen: boolean;
  libraryWidth: number;
  exportKind: ExportKind;
  /** Spritesheet layout shared by the sheet and Godot exports (scale is `exportScale`). */
  exportSheet: Omit<SheetOptions, "scale">;
  godotResDir: string;
  onion: OnionSkin;
  playMode: PlayMode;
}

export interface EditorState extends Prefs {
  session: SessionView | null;
  mode: Mode;
  librarySearch: string;
  activeLayerId: LayerId | null;
  activeFrameId: FrameId | null;
  tool: ToolId;
  symmetryX: boolean;
  symmetryY: boolean;
  symmetryMode: SymmetryMode;
  /** Reference image for the open project (see canvas/reference.ts). */
  reference: ReferenceSettings | null;
  /** Dragging on the canvas moves the reference instead of drawing. */
  referenceMoving: boolean;
  /** Mirrors the canvas view; written by CanvasView only. */
  zoom: number;
  cursor: { x: number; y: number; color: Hex | null } | null;
  /** Live layer opacity while a slider is being dragged. */
  opacityPreview: { layer: LayerId; value: number } | null;
  toast: { message: string; kind: "info" | "error" } | null;
  newDocOpen: boolean;
  newArtboardOpen: boolean;
  exportOpen: boolean;
  exportAllOpen: boolean;
  recovery: RecoveryInfo | null;
  /** Timeline playback is running (the canvas shows composites, editing pauses it). */
  playing: boolean;
  previewPlaying: boolean;
  /** Tag to play and preview; null = every frame using `playMode`. */
  playTag: string | null;
  /** Other end of a shift-click frame range; the active frame is the first end. */
  frameAnchor: FrameId | null;
  tagDialog: TagDialog;
}

const PREFS_KEY = "dotify.prefs.v1";

const defaults: Prefs = {
  theme: "dark",
  showGrid: false,
  showRulers: true,
  primary: "#1d2b53ff",
  secondary: "#00000000",
  brushSize: 1,
  filledShapes: false,
  fillContiguous: true,
  rightPanelWidth: 264,
  timelineOpen: true,
  previewOpen: true,
  exportScale: 1,
  libraryOpen: true,
  libraryWidth: 232,
  exportKind: "png",
  exportSheet: { layout: "grid", columns: null, padding: 0, extrude: 0 },
  godotResDir: "",
  onion: { enabled: false, prev: 1, next: 1, opacity: 40, tint: true },
  playMode: "loop",
};

function loadPrefs(): Prefs {
  try {
    const raw = localStorage.getItem(PREFS_KEY);
    return raw ? { ...defaults, ...(JSON.parse(raw) as Partial<Prefs>) } : defaults;
  } catch {
    return defaults;
  }
}

export const useEditor = create<EditorState>()(() => ({
  ...loadPrefs(),
  session: null,
  mode: "editor",
  librarySearch: "",
  activeLayerId: null,
  activeFrameId: null,
  tool: "pencil",
  symmetryX: false,
  symmetryY: false,
  symmetryMode: "paint",
  reference: null,
  referenceMoving: false,
  zoom: 1,
  cursor: null,
  opacityPreview: null,
  toast: null,
  newDocOpen: false,
  newArtboardOpen: false,
  exportOpen: false,
  exportAllOpen: false,
  recovery: null,
  playing: false,
  previewPlaying: true,
  playTag: null,
  frameAnchor: null,
  tagDialog: null,
}));

export const setEditor = useEditor.setState;
export const getEditor = useEditor.getState;

/** Native window colors; keep in sync with `--bg` in index.css. */
const WINDOW_BACKGROUND: Record<Theme, string> = { dark: "#17181b", light: "#e9e7e3" };

/**
 * Apply the theme synchronously, before any other store subscriber (such as
 * the canvas) reads CSS variables, and match the native window chrome.
 */
function applyTheme(theme: Theme) {
  document.documentElement.dataset.theme = theme;
  const win = getCurrentWindow();
  void win.setTheme(theme).catch(() => undefined);
  void win.setBackgroundColor(WINDOW_BACKGROUND[theme]).catch(() => undefined);
}

applyTheme(useEditor.getState().theme);
useEditor.subscribe((s, prev) => {
  if (s.theme !== prev.theme) applyTheme(s.theme);
});

// Persist preferences (not document state) across launches.
useEditor.subscribe((s, prev) => {
  const keys = Object.keys(defaults) as Array<keyof Prefs>;
  if (keys.every((k) => s[k] === prev[k])) return;
  const prefs = Object.fromEntries(keys.map((k) => [k, s[k]]));
  try {
    localStorage.setItem(PREFS_KEY, JSON.stringify(prefs));
  } catch {
    // Storage may be unavailable; preferences are a convenience only.
  }
});

let toastTimer: ReturnType<typeof setTimeout> | undefined;
export function toast(message: string, kind: "info" | "error" = "info") {
  clearTimeout(toastTimer);
  setEditor({ toast: { message, kind } });
  toastTimer = setTimeout(() => setEditor({ toast: null }), kind === "error" ? 5000 : 2500);
}

const NO_LAYERS: Layer[] = [];
const NO_FRAMES: Frame[] = [];
const NO_ARTBOARDS: ArtboardSummary[] = [];
const NO_FOLDERS: Folder[] = [];
export const selectArtboards = (s: EditorState) => s.session?.project.artboards ?? NO_ARTBOARDS;
export const selectFolders = (s: EditorState) => s.session?.project.folders ?? NO_FOLDERS;
export const selectActiveArtboard = (s: EditorState) => s.session?.active_artboard ?? null;

/** True while a modal dialog is open (single-key shortcuts are disabled). */
export const dialogOpen = (s: EditorState) =>
  s.newDocOpen ||
  s.newArtboardOpen ||
  s.exportOpen ||
  s.exportAllOpen ||
  s.recovery !== null ||
  s.tagDialog !== null;
const NO_TAGS: AnimationTag[] = [];
export const selectLayers = (s: EditorState) => s.session?.summary.layers ?? NO_LAYERS;
export const selectFrames = (s: EditorState) => s.session?.summary.frames ?? NO_FRAMES;
export const selectTags = (s: EditorState) => s.session?.summary.tags ?? NO_TAGS;

/** Timeline index range selected with shift-click (or just the active frame). */
export function selectedRange(s: EditorState): [number, number] | null {
  const frames = selectFrames(s);
  const a = frames.findIndex((f) => f.id === s.activeFrameId);
  if (a < 0) return null;
  const b = frames.findIndex((f) => f.id === s.frameAnchor);
  return b < 0 ? [a, a] : [Math.min(a, b), Math.max(a, b)];
}

export const selectSummary = (s: EditorState) => s.session?.summary ?? null;

export function activeLayer(s: EditorState) {
  return s.session?.summary.layers.find((l) => l.id === s.activeLayerId) ?? null;
}
