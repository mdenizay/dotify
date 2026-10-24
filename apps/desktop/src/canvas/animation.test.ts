import { describe, expect, it } from "vitest";
import type { AnimationTag, Frame } from "../engine/types";
import { onionFrames, type OnionSkin } from "./onion";
import { advance, resolveRange, sequence, type PlayState } from "./playback";

const frames: Frame[] = [10, 11, 12, 13].map((id) => ({ id, duration_ms: 100 }));
const tag = (t: Partial<AnimationTag>): AnimationTag => ({
  name: "Walk",
  from: 11,
  to: 13,
  direction: "forward",
  loop: true,
  color: "#ffffffff",
  ...t,
});

describe("playback sequence (mirrors dotify-export playback_order)", () => {
  it("orders by direction", () => {
    expect(sequence({ from: 2, to: 5, direction: "forward", loop: true })).toEqual([2, 3, 4, 5]);
    expect(sequence({ from: 2, to: 5, direction: "reverse", loop: true })).toEqual([5, 4, 3, 2]);
    expect(sequence({ from: 2, to: 5, direction: "ping_pong", loop: true })).toEqual([
      2, 3, 4, 5, 4, 3,
    ]);
    expect(sequence({ from: 1, to: 2, direction: "ping_pong", loop: true })).toEqual([1, 2]);
    expect(sequence({ from: 3, to: 3, direction: "ping_pong", loop: true })).toEqual([3]);
  });

  it("resolves tags and modes", () => {
    expect(resolveRange(frames, [], null, "once")).toEqual({
      from: 0,
      to: 3,
      direction: "forward",
      loop: false,
    });
    expect(resolveRange(frames, [], null, "ping_pong")?.direction).toBe("ping_pong");
    const tags = [tag({ direction: "reverse", loop: false })];
    expect(resolveRange(frames, tags, "Walk", "loop")).toEqual({
      from: 1,
      to: 3,
      direction: "reverse",
      loop: false,
    });
    // Unknown tags fall back to every frame.
    expect(resolveRange(frames, tags, "Nope", "loop")?.from).toBe(0);
    // Endpoints swapped by reordering are normalised.
    expect(resolveRange(frames, [tag({ from: 13, to: 11 })], "Walk", "loop")?.from).toBe(1);
  });
});

describe("advance", () => {
  const start: PlayState = { pos: 0, elapsed: 0, done: false };
  const d = [100, 200, 50];

  it("honours per-frame durations", () => {
    expect(advance(start, d, true, 99).pos).toBe(0);
    expect(advance(start, d, true, 100).pos).toBe(1);
    expect(advance(start, d, true, 299).pos).toBe(1);
    expect(advance(start, d, true, 300)).toEqual({ pos: 2, elapsed: 0, done: false });
  });

  it("loops, including after long stalls", () => {
    expect(advance(start, d, true, 350).pos).toBe(0);
    expect(advance(start, d, true, 350 * 1000 + 120)).toEqual({ pos: 1, elapsed: 20, done: false });
    let s = start;
    for (let t = 0; t < 35; t++) s = advance(s, d, true, 10);
    expect(s.pos).toBe(0);
  });

  it("stops on the last frame when not looping", () => {
    const s = advance(start, d, false, 10_000);
    expect(s).toEqual({ pos: 2, elapsed: 0, done: true });
    expect(advance(s, d, false, 100)).toBe(s);
  });
});

describe("onion skin", () => {
  const o: OnionSkin = { enabled: true, prev: 2, next: 1, opacity: 60, tint: true };

  it("fades with distance and clips to the timeline", () => {
    expect(onionFrames(3, 5, o)).toEqual([
      { index: 1, side: "prev", alpha: 0.3 },
      { index: 2, side: "prev", alpha: 0.6 },
      { index: 4, side: "next", alpha: 0.6 },
    ]);
    expect(onionFrames(0, 1, o)).toEqual([]);
    expect(onionFrames(3, 5, { ...o, enabled: false })).toEqual([]);
  });
});
