import { useEffect, useRef, useState } from "react";
import {
  ArrowRightFromLine,
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  Copy,
  Ghost,
  Pause,
  PersonStanding,
  Play,
  Plus,
  Settings2,
  Tag,
  Trash2,
} from "lucide-react";
import { controller } from "../app/controller";
import { tagLanes, toRgbHex } from "../app/tags";
import { MAX_ONION_FRAMES } from "../canvas/onion";
import type { PlayMode } from "../canvas/playback";
import { selectFrames, selectTags, setEditor, useEditor } from "../state/editor";
import { Divider, IconButton } from "./ui/primitives";

/** Frame cell width + gap, in CSS pixels. Tag bars and the playhead align to it. */
const CELL = 36;
const GAP = 4;
const PITCH = CELL + GAP;
const LANE = 16;

const selectClass =
  "h-6 rounded-[5px] border border-line-strong bg-panel px-1 text-[11px] text-fg outline-none focus:border-accent";

const DIRECTION_GLYPH = { forward: "→", reverse: "←", ping_pong: "↔" } as const;

/** Frame strip with playback, onion skinning and animation tags. */
export function Timeline() {
  const open = useEditor((s) => s.timelineOpen);
  const frames = useEditor(selectFrames);
  const tags = useEditor(selectTags);
  const activeId = useEditor((s) => s.activeFrameId);
  const anchorId = useEditor((s) => s.frameAnchor);
  const playing = useEditor((s) => s.playing);
  const playTag = useEditor((s) => s.playTag);
  const playMode = useEditor((s) => s.playMode);
  const onion = useEditor((s) => s.onion);
  const index = frames.findIndex((f) => f.id === activeId);
  const anchor = frames.findIndex((f) => f.id === anchorId);
  const [selFrom, selTo] =
    anchor < 0 ? [index, index] : [Math.min(index, anchor), Math.max(index, anchor)];
  const active = frames[index];
  const run = controller.run;
  const spans = tagLanes(tags, frames);
  const lanes = spans.reduce((n, s) => Math.max(n, s.lane + 1), 0);

  const newTag = () => {
    const from = frames[selFrom];
    const to = frames[selTo];
    if (from && to) setEditor({ tagDialog: { mode: "create", from: from.id, to: to.id } });
  };

  return (
    <section aria-label="Timeline" className="shrink-0 border-t border-line bg-panel">
      <header className="flex h-8 items-center gap-1 px-2">
        <button
          type="button"
          aria-expanded={open}
          onClick={() => setEditor({ timelineOpen: !open })}
          className="flex items-center gap-1 text-[11px] font-semibold tracking-wide text-dim uppercase hover:text-fg"
        >
          <ChevronDown size={12} className={`transition-transform ${open ? "" : "-rotate-90"}`} />
          Frames
        </button>
        <span className="pl-1 font-mono text-[11px] text-faint tabular-nums">
          {index + 1}/{frames.length}
        </span>
        <Divider />
        <IconButton
          label={playing ? "Pause" : "Play"}
          shortcut="Enter"
          active={playing}
          disabled={frames.length < 2}
          onClick={controller.togglePlay}
        >
          {playing ? <Pause size={13} /> : <Play size={13} />}
        </IconButton>
        <select
          aria-label="Playback range"
          className={`${selectClass} max-w-28`}
          value={playTag ?? ""}
          onChange={(e) => setEditor({ playTag: e.target.value || null })}
        >
          <option value="">All frames</option>
          {tags.map((t) => (
            <option key={t.name} value={t.name}>
              {t.name}
            </option>
          ))}
        </select>
        {playTag === null && (
          <select
            aria-label="Playback mode"
            className={selectClass}
            value={playMode}
            onChange={(e) => setEditor({ playMode: e.target.value as PlayMode })}
          >
            <option value="loop">Loop</option>
            <option value="once">Once</option>
            <option value="ping_pong">Ping-pong</option>
          </select>
        )}
        <Divider />
        <IconButton
          label="Onion skin"
          active={onion.enabled}
          onClick={() => setEditor({ onion: { ...onion, enabled: !onion.enabled } })}
        >
          <Ghost size={13} />
        </IconButton>
        <OnionSettings />
        <IconButton
          label={
            selFrom === selTo
              ? "New tag on this frame"
              : `New tag on frames ${selFrom + 1}–${selTo + 1}`
          }
          disabled={!active}
          onClick={newTag}
        >
          <Tag size={12} />
        </IconButton>
        <div className="flex-1" />
        {active && (
          <label className="flex items-center gap-1.5 pr-1 text-dim">
            Duration
            <input
              key={`${active.id}-${active.duration_ms}`}
              type="number"
              min={1}
              max={60000}
              defaultValue={active.duration_ms}
              aria-label="Frame duration in milliseconds"
              className="h-6 w-16 rounded-[5px] border border-line-strong bg-panel px-1.5 text-right font-mono text-[11px] text-fg outline-none focus:border-accent"
              onKeyDown={(e) => e.key === "Enter" && (e.target as HTMLInputElement).blur()}
              onBlur={(e) => {
                const v = Math.round(Number(e.target.value));
                if (v >= 1 && v <= 60000 && v !== active.duration_ms) {
                  void run({ type: "set_frame_duration", frame: active.id, duration_ms: v });
                } else e.target.value = String(active.duration_ms);
              }}
            />
            <span className="text-faint">ms</span>
          </label>
        )}
        <Divider />
        <IconButton
          label="Move frame left"
          disabled={index <= 0}
          onClick={() =>
            active && void run({ type: "move_frame", frame: active.id, index: index - 1 })
          }
        >
          <ChevronLeft size={14} />
        </IconButton>
        <IconButton
          label="Move frame right"
          disabled={index < 0 || index >= frames.length - 1}
          onClick={() =>
            active && void run({ type: "move_frame", frame: active.id, index: index + 1 })
          }
        >
          <ChevronRight size={14} />
        </IconButton>
        <IconButton
          label="New frame"
          onClick={() => void run({ type: "add_frame", index: index + 1 })}
        >
          <Plus size={13} />
        </IconButton>
        <IconButton
          label="Duplicate frame"
          disabled={!active}
          onClick={() => active && void run({ type: "duplicate_frame", frame: active.id })}
        >
          <Copy size={12} />
        </IconButton>
        <IconButton
          label={
            index < frames.length - 1
              ? `Copy frame ${index + 1} onto frame ${index + 2}`
              : "Copy this frame into a new next frame"
          }
          disabled={!active}
          onClick={() => void controller.copyToNextFrame("pixels")}
        >
          <ArrowRightFromLine size={13} />
        </IconButton>
        <IconButton
          label={`Copy frame ${index + 1} to the next frame as a silhouette to trace over`}
          disabled={!active}
          onClick={() => void controller.copyToNextFrame("silhouette")}
        >
          <PersonStanding size={13} />
        </IconButton>
        <IconButton
          label="Delete frame"
          disabled={!active || frames.length <= 1}
          onClick={() => active && void run({ type: "delete_frame", frame: active.id })}
        >
          <Trash2 size={12} />
        </IconButton>
      </header>
      {open && (
        <div className="overflow-x-auto px-2 pb-2">
          <div className="relative" style={{ width: frames.length * PITCH - GAP }}>
            {lanes > 0 && (
              <div className="relative mb-1" style={{ height: lanes * LANE }}>
                {spans.map((s) => (
                  <button
                    key={s.tag.name}
                    type="button"
                    title={`${s.tag.name} · frames ${s.from + 1}–${s.to + 1} · ${s.tag.direction.replace("_", "-")}${s.tag.loop ? "" : " · once"} (click to edit)`}
                    onClick={() => setEditor({ tagDialog: { mode: "edit", tag: s.tag.name } })}
                    onDoubleClick={() => setEditor({ playTag: s.tag.name })}
                    style={{
                      left: s.from * PITCH,
                      top: s.lane * LANE,
                      width: (s.to - s.from + 1) * PITCH - GAP,
                      background: `${toRgbHex(s.tag.color)}40`,
                      borderColor: toRgbHex(s.tag.color),
                    }}
                    className={`absolute flex h-[14px] items-center gap-1 overflow-hidden rounded-[3px] border-l-2 px-1 text-left text-[10px] leading-none whitespace-nowrap text-fg hover:brightness-125 ${
                      playTag === s.tag.name ? "ring-1 ring-fg/60" : ""
                    }`}
                  >
                    <span className="truncate">{s.tag.name}</span>
                    <span className="text-dim">
                      {DIRECTION_GLYPH[s.tag.direction]}
                      {s.tag.loop ? "" : "1"}
                    </span>
                  </button>
                ))}
              </div>
            )}
            <div
              role="listbox"
              aria-label="Frames"
              aria-orientation="horizontal"
              aria-multiselectable="true"
              className="relative flex h-8 items-center"
              style={{ gap: GAP }}
            >
              {frames.map((f, i) => {
                const current = f.id === activeId;
                const inRange = i >= selFrom && i <= selTo && selFrom !== selTo;
                return (
                  <button
                    key={f.id}
                    type="button"
                    role="option"
                    aria-selected={current || inRange}
                    title={`Frame ${i + 1} · ${f.duration_ms} ms (, and . to step, ⇧-click to select a range)`}
                    onClick={(e) => void controller.selectFrame(f.id, e.shiftKey)}
                    style={{ width: CELL }}
                    className={`flex h-8 shrink-0 items-center justify-center rounded-[5px] border font-mono text-[11px] tabular-nums ${
                      current
                        ? "border-accent bg-accent-soft text-fg"
                        : inRange
                          ? "border-accent/50 bg-accent-soft/50 text-fg"
                          : "border-line text-dim hover:border-line-strong hover:text-fg"
                    }`}
                  >
                    {i + 1}
                  </button>
                );
              })}
              <Playhead />
            </div>
          </div>
        </div>
      )}
    </section>
  );
}

/** Marks the frame being played. Positioned directly from the player, not React state. */
function Playhead() {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const p = controller.player;
    const update = () => {
      const el = ref.current;
      if (!el) return;
      el.style.display = p.playing ? "block" : "none";
      el.style.transform = `translateX(${p.index * PITCH}px)`;
    };
    update();
    return p.onChange(update);
  }, []);
  return (
    <div
      ref={ref}
      aria-hidden
      className="pointer-events-none absolute top-0 left-0 h-8 rounded-[5px] border-2 border-fg"
      style={{ width: CELL, display: "none" }}
    />
  );
}

function OnionSettings() {
  const onion = useEditor((s) => s.onion);
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const close = (e: PointerEvent) => {
      if (!ref.current?.contains(e.target as Node)) setOpen(false);
    };
    window.addEventListener("pointerdown", close);
    return () => window.removeEventListener("pointerdown", close);
  }, [open]);
  const set = (patch: Partial<typeof onion>) => setEditor({ onion: { ...onion, ...patch } });
  const count = (v: string) => Math.max(0, Math.min(MAX_ONION_FRAMES, Math.round(Number(v) || 0)));
  const numClass =
    "h-6 w-12 rounded-[5px] border border-line-strong bg-panel px-1.5 text-right font-mono text-[11px] text-fg outline-none focus:border-accent";

  return (
    <div ref={ref} className="relative">
      <IconButton label="Onion skin settings" active={open} onClick={() => setOpen(!open)}>
        <Settings2 size={12} />
      </IconButton>
      {open && (
        <div className="absolute bottom-7 left-0 z-40 flex w-52 flex-col gap-2 rounded-md border border-line-strong bg-raised p-3 shadow-xl shadow-black/30">
          <div className="text-[11px] font-semibold tracking-wide text-dim uppercase">
            Onion skin
          </div>
          <label className="flex items-center justify-between text-dim">
            Previous frames
            <input
              type="number"
              min={0}
              max={MAX_ONION_FRAMES}
              className={numClass}
              value={onion.prev}
              onChange={(e) => set({ prev: count(e.target.value), enabled: true })}
            />
          </label>
          <label className="flex items-center justify-between text-dim">
            Next frames
            <input
              type="number"
              min={0}
              max={MAX_ONION_FRAMES}
              className={numClass}
              value={onion.next}
              onChange={(e) => set({ next: count(e.target.value), enabled: true })}
            />
          </label>
          <label className="flex flex-col gap-1 text-dim">
            <span className="flex justify-between">
              Opacity <span className="font-mono text-faint">{onion.opacity}%</span>
            </span>
            <input
              type="range"
              min={5}
              max={100}
              value={onion.opacity}
              onChange={(e) => set({ opacity: Number(e.target.value), enabled: true })}
              className="accent-[var(--accent)]"
            />
          </label>
          <label className="flex items-center gap-2 text-dim">
            <input
              type="checkbox"
              checked={onion.tint}
              onChange={(e) => set({ tint: e.target.checked })}
            />
            Tint previous red, next blue
          </label>
        </div>
      )}
    </div>
  );
}
