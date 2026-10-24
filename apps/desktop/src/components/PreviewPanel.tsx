import { useEffect, useRef } from "react";
import { Pause, Play } from "lucide-react";
import { controller } from "../app/controller";
import { PreviewView } from "../canvas/PreviewView";
import { setEditor, useEditor } from "../state/editor";
import { IconButton } from "./ui/primitives";

export function PreviewActions() {
  const playing = useEditor((s) => s.previewPlaying);
  return (
    <IconButton
      label={playing ? "Pause preview" : "Play preview"}
      active={playing}
      onClick={() => setEditor({ previewPlaying: !playing })}
    >
      {playing ? <Pause size={12} /> : <Play size={12} />}
    </IconButton>
  );
}

/** Live animation preview; loops the playback range while you draw. */
export function PreviewPanel() {
  const ref = useRef<HTMLDivElement>(null);
  const playTag = useEditor((s) => s.playTag);
  useEffect(() => {
    const view = new PreviewView(ref.current!, controller.frames, {
      preview: controller.previewPlayer,
      timeline: controller.player,
    });
    return () => view.dispose();
  }, []);
  return (
    <div className="flex flex-col gap-1 px-2 pb-2">
      <div ref={ref} className="h-36 overflow-hidden rounded-[5px] bg-workspace" />
      <div className="truncate text-[11px] text-faint">{playTag ?? "All frames"}</div>
    </div>
  );
}
