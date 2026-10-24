import type { AnimationTag, Frame } from "../engine/types";

export interface TagSpan {
  tag: AnimationTag;
  /** Inclusive timeline indices. */
  from: number;
  to: number;
  /** Row in the tag strip; overlapping tags get separate rows. */
  lane: number;
}

/** Lay tags out on the fewest rows so that no two overlap on one row. */
export function tagLanes(tags: AnimationTag[], frames: Frame[]): TagSpan[] {
  const index = (id: number) => frames.findIndex((f) => f.id === id);
  const spans = tags
    .map((tag) => {
      const a = index(tag.from);
      const b = index(tag.to);
      return { tag, from: Math.min(a, b), to: Math.max(a, b), lane: 0 };
    })
    .filter((s) => s.from >= 0)
    .sort((x, y) => x.from - y.from || y.to - x.to);
  const laneEnds: number[] = [];
  for (const s of spans) {
    let lane = laneEnds.findIndex((end) => end < s.from);
    if (lane < 0) lane = laneEnds.length;
    laneEnds[lane] = s.to;
    s.lane = lane;
  }
  return spans;
}

/** "Tag 1", "Tag 2", … — the first name not taken. */
export function uniqueTagName(tags: AnimationTag[], base = "Tag"): string {
  for (let n = 1; ; n++) {
    const name = `${base} ${n}`;
    if (!tags.some((t) => t.name === name)) return name;
  }
}

/** `#rrggbb` for `<input type="color">` from a `#rrggbbaa` engine color. */
export const toRgbHex = (hex: string) => hex.slice(0, 7);
