import { getEditor, useEditor } from "../state/editor";
import type { FrameStore } from "./FrameStore";
import type { Player } from "./playback";

/**
 * The live preview: the animation at the largest integer zoom that fits,
 * drawn from engine composites. Shows its own player's frame while that
 * loops, else follows the timeline. Lives outside React.
 */
export class PreviewView {
  private canvas: HTMLCanvasElement;
  private ctx: CanvasRenderingContext2D;
  private disposers: Array<() => void> = [];
  private resizeObserver: ResizeObserver;
  private requested = false;

  constructor(
    private container: HTMLElement,
    private frames: FrameStore,
    private players: { preview: Player; timeline: Player },
  ) {
    this.canvas = document.createElement("canvas");
    this.canvas.className = "block h-full w-full";
    container.appendChild(this.canvas);
    this.ctx = this.canvas.getContext("2d")!;
    this.resizeObserver = new ResizeObserver(() => this.resize());
    this.resizeObserver.observe(container);
    this.disposers.push(
      frames.onChange(() => this.invalidate()),
      players.preview.onChange(() => this.invalidate()),
      players.timeline.onChange(() => this.invalidate()),
      useEditor.subscribe((s, p) => {
        if (s.session !== p.session || s.activeFrameId !== p.activeFrameId || s.theme !== p.theme)
          this.invalidate();
      }),
    );
    this.resize();
  }

  dispose() {
    this.resizeObserver.disconnect();
    for (const d of this.disposers) d();
    this.canvas.remove();
  }

  private resize() {
    const r = this.container.getBoundingClientRect();
    const dpr = window.devicePixelRatio || 1;
    this.canvas.width = Math.max(1, Math.round(r.width * dpr));
    this.canvas.height = Math.max(1, Math.round(r.height * dpr));
    this.render();
  }

  private invalidate() {
    if (this.requested) return;
    this.requested = true;
    requestAnimationFrame(() => {
      this.requested = false;
      this.render();
    });
  }

  private render() {
    const { ctx, canvas } = this;
    const s = getEditor();
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    const summary = s.session?.summary;
    if (!summary) return;
    const { preview, timeline } = this.players;
    const frame = preview.playing
      ? summary.frames[preview.index]
      : timeline.playing
        ? summary.frames[timeline.index]
        : summary.frames.find((f) => f.id === s.activeFrameId);
    const img = frame && this.frames.get(frame.id);
    // Integer zoom only: pixel art must never be resampled.
    const zoom = Math.max(
      1,
      Math.floor(Math.min(canvas.width / summary.width, canvas.height / summary.height)),
    );
    const w = summary.width * zoom;
    const h = summary.height * zoom;
    const x = Math.floor((canvas.width - w) / 2);
    const y = Math.floor((canvas.height - h) / 2);
    const cs = getComputedStyle(document.documentElement);
    ctx.fillStyle = cs.getPropertyValue("--check-b").trim() || "#222327";
    ctx.fillRect(x, y, w, h);
    if (!img) return;
    ctx.imageSmoothingEnabled = false;
    ctx.drawImage(img, x, y, w, h);
  }
}
