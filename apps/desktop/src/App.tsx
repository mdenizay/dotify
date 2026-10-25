import { useEffect, useRef, useState } from "react";
import { controller } from "./app/controller";
import { installShortcuts } from "./app/shortcuts";
import { scheduleUpdateChecks } from "./app/updates";
import { CanvasView } from "./canvas/CanvasView";
import { OverviewView } from "./canvas/OverviewView";
import { ColorPicker } from "./components/ColorPicker";
import {
  ExportAllDialog,
  ExportDialog,
  NewArtboardDialog,
  NewDocumentDialog,
  RecoveryDialog,
} from "./components/Dialogs";
import { LayerActions, LayersPanel } from "./components/LayersPanel";
import { LibraryPanel } from "./components/LibraryPanel";
import { PaletteActions, PalettePanel } from "./components/PalettePanel";
import { PreviewActions, PreviewPanel } from "./components/PreviewPanel";
import { StatusBar } from "./components/StatusBar";
import { TagDialog } from "./components/TagDialog";
import { Timeline } from "./components/Timeline";
import { Toolbar } from "./components/Toolbar";
import { TopBar } from "./components/TopBar";
import { Section } from "./components/ui/primitives";
import { errorMessage } from "./engine/bridge";
import { setEditor, useEditor } from "./state/editor";

function CanvasHost() {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const view = new CanvasView(ref.current!, {
      cache: controller.cache,
      frames: controller.frames,
      player: controller.player,
      run: controller.run,
      stopPlayback: controller.stopPlayback,
    });
    controller.view = view;
    return () => {
      controller.view = null;
      view.dispose();
    };
  }, []);
  // Clipped, so the canvas can never paint over the timeline below it.
  return <div ref={ref} className="relative min-h-0 min-w-0 flex-1 overflow-hidden bg-workspace" />;
}

function OverviewHost() {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const view = new OverviewView(ref.current!, {
      images: controller.images,
      select: (id) => void controller.selectArtboard(id),
      open: (id) => void controller.selectArtboard(id, true),
    });
    controller.overview = view;
    return () => {
      controller.overview = null;
      view.dispose();
    };
  }, []);
  return <div ref={ref} className="relative min-h-0 min-w-0 flex-1 bg-workspace" />;
}

function RightPanel() {
  const width = useEditor((s) => s.rightPanelWidth);
  const [open, setOpen] = useState({ color: true, palette: true, layers: true });
  const toggle = (k: keyof typeof open) => () => setOpen((o) => ({ ...o, [k]: !o[k] }));
  // Persisted, because closing the preview also stops its playback.
  const previewOpen = useEditor((s) => s.previewOpen);

  const startResize = (e: React.PointerEvent) => {
    const startX = e.clientX;
    const start = width;
    const move = (ev: PointerEvent) =>
      setEditor({ rightPanelWidth: Math.min(420, Math.max(220, start - (ev.clientX - startX))) });
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };

  return (
    <aside
      style={{ width }}
      className="relative flex shrink-0 flex-col border-l border-line bg-panel"
    >
      <div
        role="separator"
        aria-orientation="vertical"
        aria-label="Resize panel"
        onPointerDown={startResize}
        className="absolute top-0 bottom-0 -left-1 z-10 w-2 cursor-col-resize"
      />
      <Section
        title="Preview"
        open={previewOpen}
        onToggle={() => setEditor({ previewOpen: !previewOpen })}
        actions={<PreviewActions />}
      >
        <PreviewPanel />
      </Section>
      <Section title="Color" open={open.color} onToggle={toggle("color")}>
        <ColorPicker />
      </Section>
      <Section
        title="Palette"
        open={open.palette}
        onToggle={toggle("palette")}
        actions={<PaletteActions />}
      >
        <PalettePanel />
      </Section>
      <Section
        title="Layers"
        grow
        open={open.layers}
        onToggle={toggle("layers")}
        actions={<LayerActions />}
      >
        <LayersPanel />
      </Section>
    </aside>
  );
}

export function App() {
  const ready = useEditor((s) => s.session !== null);
  const overview = useEditor((s) => s.mode === "overview");
  const libraryOpen = useEditor((s) => s.libraryOpen);
  const [fatal, setFatal] = useState<string | null>(null);

  useEffect(() => {
    controller.init().catch((e) => setFatal(errorMessage(e)));
    const stopUpdates = scheduleUpdateChecks();
    const removeShortcuts = installShortcuts();
    return () => {
      stopUpdates();
      removeShortcuts();
    };
  }, []);

  if (fatal) {
    return (
      <div className="flex h-full items-center justify-center p-8 text-center text-dim">
        Dotify could not connect to its engine: {fatal}
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col">
      <TopBar />
      <div className="flex min-h-0 flex-1">
        {libraryOpen && <LibraryPanel />}
        <div className={overview ? "flex min-w-0 flex-1" : "hidden"}>
          <OverviewHost />
        </div>
        <div className={overview ? "hidden" : "flex min-w-0 flex-1"}>
          <Toolbar />
          <div className="flex min-w-0 flex-1 flex-col">
            <CanvasHost />
            <Timeline />
          </div>
          <RightPanel />
        </div>
      </div>
      <StatusBar />
      {ready && (
        <>
          <NewDocumentDialog />
          <NewArtboardDialog />
          <ExportDialog />
          <ExportAllDialog />
          <TagDialog />
          <RecoveryDialog />
        </>
      )}
    </div>
  );
}
