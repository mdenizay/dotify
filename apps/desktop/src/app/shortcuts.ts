import { dialogOpen, getEditor, setEditor, type ToolId } from "../state/editor";
import { controller, isTyping } from "./controller";

const TOOL_KEYS: Record<string, ToolId> = {
  b: "pencil",
  e: "eraser",
  g: "fill",
  l: "line",
  u: "rectangle",
  o: "ellipse",
  i: "eyedropper",
  h: "hand",
};

export const TOOL_SHORTCUTS: Record<ToolId, string> = Object.fromEntries(
  Object.entries(TOOL_KEYS).map(([k, t]) => [t, k.toUpperCase()]),
) as Record<ToolId, string>;

/**
 * Single-key editor shortcuts. Shortcuts with ⌘ are native menu accelerators
 * (src-tauri/src/menu.rs) so they work with macOS conventions.
 */
export function installShortcuts(): () => void {
  const onKey = (e: KeyboardEvent) => {
    if (e.metaKey || e.ctrlKey || isTyping() || e.repeat) return;
    const s = getEditor();
    if (dialogOpen(s) || s.mode !== "editor") return;
    const key = e.key.toLowerCase();
    const tool = TOOL_KEYS[key];
    if (tool && !e.altKey) {
      setEditor({ tool: key === "u" && e.shiftKey ? "ellipse" : tool });
      e.preventDefault();
      return;
    }
    switch (e.key) {
      case "x":
        setEditor({ primary: s.secondary, secondary: s.primary });
        break;
      case "[":
        setEditor({ brushSize: Math.max(1, s.brushSize - 1) });
        break;
      case "]":
        setEditor({ brushSize: Math.min(32, s.brushSize + 1) });
        break;
      case "+":
      case "=":
        controller.view?.zoomStep(1);
        break;
      case "-":
        controller.view?.zoomStep(-1);
        break;
      case ",":
      case ".": {
        controller.stopPlayback();
        const s2 = getEditor();
        const frames = s.session?.summary.frames ?? [];
        const i = frames.findIndex((f) => f.id === s2.activeFrameId);
        const next = frames[(i + (e.key === "," ? -1 : 1) + frames.length) % frames.length];
        if (next) void controller.selectFrame(next.id);
        break;
      }
      case "N":
        void controller.run({ type: "create_layer" });
        break;
      case "Enter":
        // Buttons handle Enter themselves.
        if ((e.target as HTMLElement | null)?.tagName === "BUTTON") return;
        controller.togglePlay();
        break;
      default:
        return;
    }
    e.preventDefault();
  };
  window.addEventListener("keydown", onKey);
  return () => window.removeEventListener("keydown", onKey);
}
