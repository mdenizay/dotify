import { describe, expect, it } from "vitest";
import { sheetSize } from "./sheetSize";
import type { AnimationTag, Frame } from "../engine/types";
import { tagLanes, toRgbHex, uniqueTagName } from "./tags";

const frames: Frame[] = [1, 2, 3, 4, 5].map((id) => ({ id, duration_ms: 100 }));
const tag = (name: string, from: number, to: number): AnimationTag => ({
  name,
  from,
  to,
  direction: "forward",
  loop: true,
  color: "#5b8cffff",
});

describe("tags", () => {
  it("stacks overlapping tags on separate lanes", () => {
    const spans = tagLanes(
      [tag("a", 1, 3), tag("b", 3, 4), tag("c", 5, 5), tag("d", 4, 2)],
      frames,
    );
    const lane = Object.fromEntries(spans.map((s) => [s.tag.name, [s.from, s.to, s.lane]]));
    expect(lane).toEqual({ a: [0, 2, 0], d: [1, 3, 1], b: [2, 3, 2], c: [4, 4, 0] });
  });

  it("names and colors", () => {
    expect(uniqueTagName([tag("Tag 1", 1, 1), tag("Tag 3", 1, 1)])).toBe("Tag 2");
    expect(toRgbHex("#5b8cffff")).toBe("#5b8cff");
  });
});

describe("sheet size readout (mirrors the dotify-export sheet tests)", () => {
  const o = { columns: null, scale: 1, padding: 0, extrude: 0 };
  it("matches packing", () => {
    expect(sheetSize(4, 3, 4, null, o)).toEqual({ width: 8, height: 6 });
    expect(sheetSize(4, 3, 4, null, { ...o, columns: 4, padding: 2, scale: 2 })).toEqual({
      width: 42,
      height: 10,
    });
    expect(sheetSize(4, 3, 4, null, { ...o, columns: 1, padding: 1, extrude: 1 })).toEqual({
      width: 8,
      height: 25,
    });
    expect(sheetSize(4, 3, 4, [3, 1], o)).toEqual({ width: 12, height: 6 });
  });
});
