import { getCurrentWindow } from "@tauri-apps/api/window";
import { create } from "zustand";
import type {
  Frame,
  FrameId,
  Hex,
  Layer,
  LayerId,
  RecoveryInfo,
  SessionView,
} from "../engine/types";

export type ToolId =
  "pencil" | "eraser" | "fill" | "line" | "rectangle" | "ellipse" | "eyedropper" | "hand";

export type Theme = "dark" | "light";

interface Prefs {
  theme: Theme;
  showGrid: boolean;
  primary: Hex;
  secondary: Hex;
  brushSize: number;
  filledShapes: boolean;
  fillContiguous: boolean;
  rightPanelWidth: number;
  timelineOpen: boolean;
  exportScale: number;
}

export interface EditorState extends Prefs {
  session: SessionView | null;
  activeLayerId: LayerId | null;
  activeFrameId: FrameId | null;
  tool: ToolId;
  symmetryX: boolean;
  symmetryY: boolean;
  /** Mirrors the canvas view; written by CanvasView only. */
  zoom: number;
  cursor: { x: number; y: number; color: Hex | null } | null;
  /** Live layer opacity while a slider is being dragged. */
  opacityPreview: { layer: LayerId; value: number } | null;
  toast: { message: string; kind: "info" | "error" } | null;
  newDocOpen: boolean;
  exportOpen: boolean;
  recovery: RecoveryInfo | null;
}

const PREFS_KEY = "dotify.prefs.v1";

const defaults: Prefs = {
  theme: "dark",
  showGrid: false,
  primary: "#1d2b53ff",
  secondary: "#00000000",
  brushSize: 1,
  filledShapes: false,
  fillContiguous: true,
  rightPanelWidth: 264,
  timelineOpen: true,
  exportScale: 1,
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
  activeLayerId: null,
  activeFrameId: null,
  tool: "pencil",
  symmetryX: false,
  symmetryY: false,
  zoom: 1,
  cursor: null,
  opacityPreview: null,
  toast: null,
  newDocOpen: false,
  exportOpen: false,
  recovery: null,
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
export const selectLayers = (s: EditorState) => s.session?.summary.layers ?? NO_LAYERS;
export const selectFrames = (s: EditorState) => s.session?.summary.frames ?? NO_FRAMES;

export const selectSummary = (s: EditorState) => s.session?.summary ?? null;

export function activeLayer(s: EditorState) {
  return s.session?.summary.layers.find((l) => l.id === s.activeLayerId) ?? null;
}
