import { describe, expect, it } from "vitest";
import { blendOver, hsvaToRgba, parseHex, rgbaToHsva, toHex } from "./color";
import { boxFrom, brushOffsets, ellipse, line, rectangle } from "./raster";
import { fit, nextZoom, scaleOf, toCanvas, zoomAt } from "./viewport";

const collect = (f: (plot: (x: number, y: number) => void) => void) => {
  const pts: Array<[number, number]> = [];
  f((x, y) => pts.push([x, y]));
  return pts;
};

describe("raster (mirrors dotify-core tests)", () => {
  it("line endpoints and length", () => {
    const pts = collect((p) => line(0, 0, 5, 2, p));
    expect(pts[0]).toEqual([0, 0]);
    expect(pts.at(-1)).toEqual([5, 2]);
    expect(pts).toHaveLength(6);
    expect(collect((p) => line(0, 0, -4, 4, p))).toEqual([
      [0, 0],
      [-1, 1],
      [-2, 2],
      [-3, 3],
      [-4, 4],
    ]);
  });

  it("rectangle counts", () => {
    expect(collect((p) => rectangle(0, 0, 4, 3, false, p))).toHaveLength(10);
    expect(collect((p) => rectangle(0, 0, 4, 3, true, p))).toHaveLength(12);
  });

  it("8x8 ellipse outline matches the engine", () => {
    const grid = Array.from({ length: 8 }, () => Array(8).fill(" "));
    ellipse(0, 0, 8, 8, false, (x, y) => (grid[y]![x] = "#"));
    expect(grid.map((r) => r.join(""))).toEqual([
      "  ####  ",
      " #    # ",
      "#      #",
      "#      #",
      "#      #",
      "#      #",
      " #    # ",
      "  ####  ",
    ]);
  });

  it("ellipses are symmetric", () => {
    for (const [w, h] of [
      [5, 3],
      [13, 6],
      [1, 9],
    ] as const) {
      const pts = collect((p) => ellipse(0, 0, w, h, false, p));
      const set = new Set(pts.map(([x, y]) => `${x},${y}`));
      for (const [x, y] of pts) expect(set.has(`${w - 1 - x},${y}`)).toBe(true);
    }
  });

  it("boxFrom normalises and constrains", () => {
    expect(boxFrom(5, 5, 2, 3, false)).toEqual({ x: 2, y: 3, width: 4, height: 3 });
    expect(boxFrom(0, 0, 3, 1, true)).toEqual({ x: 0, y: 0, width: 4, height: 4 });
  });

  it("brush offsets are centred", () => {
    expect(brushOffsets(1)).toEqual([[0, 0]]);
    expect(brushOffsets(3)).toHaveLength(9);
    expect(brushOffsets(3)[0]).toEqual([-1, -1]);
  });
});

describe("color", () => {
  it("hex parsing", () => {
    expect(parseHex("#fff")).toEqual([255, 255, 255, 255]);
    expect(parseHex("#10203040")).toEqual([16, 32, 48, 64]);
    expect(parseHex("nope")).toBeNull();
    expect(toHex([1, 2, 3, 4])).toBe("#01020304");
  });

  it("blendOver matches the Rust test vectors", () => {
    expect(blendOver([1, 2, 3, 255], [9, 8, 7, 255], 255)).toEqual([9, 8, 7, 255]);
    expect(blendOver([0, 0, 0, 255], [255, 255, 255, 255], 128)).toEqual([128, 128, 128, 255]);
    expect(blendOver([0, 0, 255, 128], [255, 0, 0, 128], 255)).toEqual([170, 0, 85, 192]);
    expect(blendOver([0, 0, 0, 0], [200, 100, 50, 128], 255)).toEqual([200, 100, 50, 128]);
  });

  it("hsv round trip", () => {
    for (const c of [
      [255, 0, 77, 255],
      [12, 200, 90, 10],
      [128, 128, 128, 255],
    ] as const) {
      expect(hsvaToRgba(rgbaToHsva([...c]))).toEqual([...c]);
    }
  });
});

describe("viewport", () => {
  it("zoomAt keeps the point under the cursor", () => {
    const v = { zoom: 4, dpr: 2, ox: 100, oy: 50 };
    const before = toCanvas(v, 300, 200);
    const after = toCanvas(zoomAt(v, 16, 300, 200), 300, 200);
    expect(Math.abs(after.x - before.x)).toBeLessThan(1 / 32);
    expect(Math.abs(after.y - before.y)).toBeLessThan(1 / 32);
  });

  it("fit picks an integer zoom that fits", () => {
    const v = fit(32, 32, 800, 600, 2, 0);
    expect(v.zoom).toBe(16);
    expect(scaleOf(v)).toBe(32);
    expect(v.ox).toBe((1600 - 32 * 32) / 2);
  });

  it("zoom steps", () => {
    expect(nextZoom(4, 1)).toBe(6);
    expect(nextZoom(4, -1)).toBe(3);
    expect(nextZoom(1, -1)).toBe(1);
  });
});
