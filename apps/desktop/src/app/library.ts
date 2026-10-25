// Pure helpers for the library sidebar, kept separate so they can be tested.

import type { ArtboardId, ArtboardSummary, FolderId, ProjectCommand } from "../engine/types";

/** Where a dragged artboard is dropped. */
export type DropTarget =
  | { before: ArtboardId }
  | { after: ArtboardId }
  /** The end of a folder (`null` = ungrouped). */
  | { folder: FolderId | null };

/**
 * The `move_artboard` command for dropping `id` on `target`, or `null` when
 * the drop changes nothing. `index` is the final position in the library
 * order, as the engine expects.
 */
export function moveCommand(
  artboards: ArtboardSummary[],
  id: ArtboardId,
  target: DropTarget,
): ProjectCommand | null {
  const src = artboards.find((a) => a.id === id);
  if (!src) return null;
  const rest = artboards.filter((a) => a.id !== id);
  let index: number;
  let folder: FolderId | null;
  if ("folder" in target) {
    folder = target.folder;
    let last = -1;
    rest.forEach((a, i) => {
      if (a.folder === folder) last = i;
    });
    index = last >= 0 ? last + 1 : rest.length;
  } else {
    const ref = "before" in target ? target.before : target.after;
    if (ref === id) return null;
    const i = rest.findIndex((a) => a.id === ref);
    if (i < 0) return null;
    folder = rest[i]!.folder;
    index = "before" in target ? i : i + 1;
  }
  const from = artboards.indexOf(src);
  if (index === from && folder === src.folder) return null;
  return { type: "move_artboard", artboard: id, index, folder };
}

/** Group artboards for display: ungrouped first, then each folder. */
export function groupArtboards<T extends { folder: FolderId | null }>(
  artboards: T[],
  folderIds: FolderId[],
): Map<FolderId | null, T[]> {
  const groups = new Map<FolderId | null, T[]>([[null, []]]);
  for (const f of folderIds) groups.set(f, []);
  for (const a of artboards) {
    const key = a.folder !== null && groups.has(a.folder) ? a.folder : null;
    groups.get(key)!.push(a);
  }
  return groups;
}
