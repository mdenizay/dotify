import { describe, expect, it } from "vitest";
import { hitTest, layoutOverview, matchesSearch, UNGROUPED_TITLE } from "../canvas/overviewLayout";
import type { ArtboardSummary, Folder } from "../engine/types";
import { groupArtboards, moveCommand } from "./library";

const art = (id: number, folder: number | null, width = 16, height = 16): ArtboardSummary => ({
  id,
  name: `Art ${id}`,
  folder,
  width,
  height,
  layers: [],
  frames: [],
  tags: [],
});

const folders: Folder[] = [
  { id: 10, name: "Characters" },
  { id: 11, name: "Tiles" },
];

describe("moveCommand", () => {
  // Library order: 1 (root), 2 (Characters), 3 (Characters), 4 (Tiles)
  const list = [art(1, null), art(2, 10), art(3, 10), art(4, 11)];

  it("drops before and after another artboard, taking its folder", () => {
    expect(moveCommand(list, 4, { before: 2 })).toEqual({
      type: "move_artboard",
      artboard: 4,
      index: 1,
      folder: 10,
    });
    expect(moveCommand(list, 1, { after: 3 })).toEqual({
      type: "move_artboard",
      artboard: 1,
      index: 2,
      folder: 10,
    });
  });

  it("drops at the end of a folder or the root", () => {
    expect(moveCommand(list, 1, { folder: 11 })).toEqual({
      type: "move_artboard",
      artboard: 1,
      index: 3,
      folder: 11,
    });
    expect(moveCommand(list, 4, { folder: null })).toEqual({
      type: "move_artboard",
      artboard: 4,
      index: 1,
      folder: null,
    });
    // Empty folder: goes to the end of the library.
    expect(moveCommand([art(1, null), art(2, null)], 1, { folder: 12 })).toEqual({
      type: "move_artboard",
      artboard: 1,
      index: 1,
      folder: 12,
    });
  });

  it("returns null for drops that change nothing", () => {
    expect(moveCommand(list, 2, { before: 3 })).toBeNull();
    expect(moveCommand(list, 3, { after: 2 })).toBeNull();
    expect(moveCommand(list, 3, { folder: 10 })).toBeNull();
    expect(moveCommand(list, 2, { before: 2 })).toBeNull();
    expect(moveCommand(list, 99, { folder: null })).toBeNull();
  });
});

describe("groupArtboards", () => {
  it("puts unknown folders with the ungrouped artboards", () => {
    const groups = groupArtboards([art(1, 10), art(2, null), art(3, 99)], [10]);
    expect(groups.get(null)!.map((a) => a.id)).toEqual([2, 3]);
    expect(groups.get(10)!.map((a) => a.id)).toEqual([1]);
  });
});

describe("overview layout", () => {
  it("groups by folder, wraps rows and keeps relative sizes", () => {
    const l = layoutOverview(
      [art(1, 11, 32, 32), art(2, null, 8, 8), art(3, 11, 32, 16)],
      folders,
      {
        gap: 4,
        header: 10,
        rowWidth: 64,
      },
    );
    expect(l.sections.map((s) => s.title)).toEqual([UNGROUPED_TITLE, "Tiles"]);
    expect(l.items).toEqual([
      { id: 2, x: 0, y: 10, width: 8, height: 8 },
      { id: 1, x: 0, y: 36, width: 32, height: 32 },
      // 32 + 4 + 32 > 64 → wraps onto the next row.
      { id: 3, x: 0, y: 72, width: 32, height: 16 },
    ]);
    expect(l.sections[1]).toMatchObject({ y: 26 });
    expect([l.width, l.height]).toEqual([32, 88]);
    expect(hitTest(l, 5, 40)).toBe(1);
    expect(hitTest(l, 40, 40)).toBeNull();
  });

  it("never wraps an artboard wider than the row onto an empty row", () => {
    const l = layoutOverview([art(1, null, 500, 10), art(2, null, 10, 10)], [], {
      gap: 4,
      header: 0,
      rowWidth: 100,
    });
    expect(l.items.map((i) => [i.x, i.y])).toEqual([
      [0, 0],
      [0, 14],
    ]);
  });

  it("searches artboard and folder names", () => {
    expect(matchesSearch(art(1, 11), folders, "tile")).toBe(true);
    expect(matchesSearch(art(1, 11), folders, "ART 1")).toBe(true);
    expect(matchesSearch(art(1, null), folders, "hero")).toBe(false);
    expect(matchesSearch(art(1, null), folders, "  ")).toBe(true);
  });
});
