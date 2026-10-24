import type { AnimationTag, Direction, Frame } from "../engine/types";

/** A frame range to play. Indices are timeline positions, inclusive. */
export interface PlayRange {
  from: number;
  to: number;
  direction: Direction;
  loop: boolean;
}

export type PlayMode = "loop" | "once" | "ping_pong";

/**
 * Timeline indices in display order. Ping-pong does not repeat the end
 * frames; mirrors `playback_order` in crates/dotify-export/src/sheet.rs.
 */
export function sequence(r: PlayRange): number[] {
  const out: number[] = [];
  if (r.direction === "reverse") for (let i = r.to; i >= r.from; i--) out.push(i);
  else for (let i = r.from; i <= r.to; i++) out.push(i);
  if (r.direction === "ping_pong") for (let i = r.to - 1; i > r.from; i--) out.push(i);
  return out;
}

/** The range for a tag (with its own direction and loop), or every frame in `mode`. */
export function resolveRange(
  frames: Frame[],
  tags: AnimationTag[],
  tag: string | null,
  mode: PlayMode,
): PlayRange | null {
  if (frames.length === 0) return null;
  const t = tag ? tags.find((x) => x.name === tag) : undefined;
  if (t) {
    const a = frames.findIndex((f) => f.id === t.from);
    const b = frames.findIndex((f) => f.id === t.to);
    if (a >= 0 && b >= 0)
      return { from: Math.min(a, b), to: Math.max(a, b), direction: t.direction, loop: t.loop };
  }
  return {
    from: 0,
    to: frames.length - 1,
    direction: mode === "ping_pong" ? "ping_pong" : "forward",
    loop: mode !== "once",
  };
}

export interface PlayState {
  /** Position in the sequence. */
  pos: number;
  /** Milliseconds already spent on the current position. */
  elapsed: number;
  done: boolean;
}

/**
 * Advance by `dt` ms through a sequence whose entries last `durations[pos]`
 * ms. Pure so timing can be tested without a clock.
 */
export function advance(s: PlayState, durations: number[], loop: boolean, dt: number): PlayState {
  let { pos, elapsed } = s;
  if (s.done || durations.length === 0) return s;
  elapsed += dt;
  // Bound the work after long stalls (a hidden window, a breakpoint).
  const total = durations.reduce((a, b) => a + b, 0);
  if (loop && total > 0 && elapsed > total) elapsed %= total;
  while (elapsed >= durations[pos]!) {
    if (pos === durations.length - 1 && !loop) return { pos, elapsed: 0, done: true };
    elapsed -= durations[pos]!;
    pos = (pos + 1) % durations.length;
  }
  return { pos, elapsed, done: false };
}

/**
 * Plays a range on a `requestAnimationFrame` clock and reports the current
 * timeline index. Frame durations are read live, so edits apply while
 * playing. Lives outside React.
 */
export class Player {
  private seq: number[] = [];
  private range: PlayRange | null = null;
  private state: PlayState = { pos: 0, elapsed: 0, done: true };
  private raf = 0;
  private last = 0;
  private listeners = new Set<() => void>();
  /** Timeline index being shown. */
  index = 0;

  constructor(private durations: () => number[]) {}

  get playing() {
    return this.raf !== 0;
  }

  /** Start at `startIndex` if it is part of the range, else at its start. */
  play(range: PlayRange, startIndex?: number) {
    this.stopClock();
    this.range = range;
    this.seq = sequence(range);
    const pos = startIndex === undefined ? -1 : this.seq.indexOf(startIndex);
    this.state = { pos: Math.max(0, pos), elapsed: 0, done: false };
    this.index = this.seq[this.state.pos]!;
    this.last = performance.now();
    this.raf = requestAnimationFrame(this.tick);
    this.emit();
  }

  pause() {
    if (!this.playing) return;
    this.stopClock();
    this.emit();
  }

  onChange(fn: () => void) {
    this.listeners.add(fn);
    return () => {
      this.listeners.delete(fn);
    };
  }

  private stopClock() {
    cancelAnimationFrame(this.raf);
    this.raf = 0;
  }

  private tick = (now: number) => {
    const all = this.durations();
    const range = this.range;
    if (!range || range.to >= all.length) {
      // The timeline shrank under us.
      this.stopClock();
      this.emit();
      return;
    }
    const durations = this.seq.map((i) => all[i] ?? 100);
    this.state = advance(this.state, durations, range.loop, now - this.last);
    this.last = now;
    const index = this.seq[this.state.pos]!;
    if (this.state.done) {
      this.raf = 0;
      this.index = index;
      this.emit();
      return;
    }
    this.raf = requestAnimationFrame(this.tick);
    if (index !== this.index) {
      this.index = index;
      this.emit();
    }
  };

  private emit() {
    for (const fn of this.listeners) fn();
  }
}
