// Pure layout for the artboard overview, kept separate from DOM code so it
// can be tested. Units are artboard pixels ("world" space): artboards keep
// their relative sizes, like artboards on a design canvas.

import type { ArtboardId, ArtboardSummary, Folder, FolderId } from "../engine/types";

export interface PlacedArtboard {
  id: ArtboardId;
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface OverviewSection {
  folder: FolderId | null;
  title: string;
  /** Top-left of the section header. */
  x: number;
  y: number;
}

export interface OverviewLayout {
  items: PlacedArtboard[];
  sections: OverviewSection[];
  width: number;
  height: number;
}

export interface LayoutOptions {
  /** Space between artboards. */
  gap: number;
  /** Space reserved above each section's first row for its title. */
  header: number;
  /** Rows wrap once they would grow wider than this. */
  rowWidth: number;
}

export const DEFAULT_LAYOUT: LayoutOptions = { gap: 16, header: 20, rowWidth: 640 };

export const UNGROUPED_TITLE = "No folder";

/**
 * Flow layout: one section per folder (ungrouped artboards first, then
 * folders in library order), artboards left to right in library order,
 * wrapping into rows. Empty sections are skipped.
 */
export function layoutOverview(
  artboards: ArtboardSummary[],
  folders: Folder[],
  opts: LayoutOptions = DEFAULT_LAYOUT,
): OverviewLayout {
  const { gap, header } = opts;
  const rowWidth = Math.max(opts.rowWidth, ...artboards.map((a) => a.width));
  const groups: Array<{ folder: FolderId | null; title: string }> = [
    { folder: null, title: UNGROUPED_TITLE },
    ...folders.map((f) => ({ folder: f.id, title: f.name })),
  ];
  const known = new Set(folders.map((f) => f.id));
  const items: PlacedArtboard[] = [];
  const sections: OverviewSection[] = [];
  let y = 0;
  let width = 0;
  for (const group of groups) {
    const members = artboards.filter((a) =>
      group.folder === null ? a.folder === null || !known.has(a.folder) : a.folder === group.folder,
    );
    if (members.length === 0) continue;
    if (sections.length > 0) y += gap * 2;
    sections.push({ folder: group.folder, title: group.title, x: 0, y });
    y += header;
    let x = 0;
    let rowHeight = 0;
    for (const a of members) {
      if (x > 0 && x + a.width > rowWidth) {
        x = 0;
        y += rowHeight + gap;
        rowHeight = 0;
      }
      items.push({ id: a.id, x, y, width: a.width, height: a.height });
      x += a.width + gap;
      rowHeight = Math.max(rowHeight, a.height);
      width = Math.max(width, x - gap);
    }
    y += rowHeight;
  }
  return { items, sections, width, height: y };
}

/** Topmost artboard under a world-space point. */
export function hitTest(layout: OverviewLayout, x: number, y: number): ArtboardId | null {
  for (let i = layout.items.length - 1; i >= 0; i--) {
    const it = layout.items[i]!;
    if (x >= it.x && y >= it.y && x < it.x + it.width && y < it.y + it.height) return it.id;
  }
  return null;
}

/** Case-insensitive substring match on artboard and folder names. */
export function matchesSearch(a: ArtboardSummary, folders: Folder[], query: string): boolean {
  const q = query.trim().toLowerCase();
  if (!q) return true;
  const folder = folders.find((f) => f.id === a.folder)?.name ?? "";
  return a.name.toLowerCase().includes(q) || folder.toLowerCase().includes(q);
}
