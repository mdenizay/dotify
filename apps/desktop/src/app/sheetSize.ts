/** Pixel size of the packed sheet; mirrors `pack` in crates/dotify-export/src/sheet.rs. */
export function sheetSize(
  frameW: number,
  frameH: number,
  frames: number,
  rows: number[] | null,
  o: { columns: number | null; scale: number; padding: number; extrude: number },
): { width: number; height: number } {
  const cols = rows
    ? Math.max(1, ...rows)
    : Math.min(o.columns ?? Math.ceil(Math.sqrt(frames)), frames);
  const rowCount = rows ? rows.length : Math.ceil(frames / Math.max(1, cols));
  const pitchX = frameW * o.scale + 2 * o.extrude + o.padding;
  const pitchY = frameH * o.scale + 2 * o.extrude + o.padding;
  return { width: o.padding + cols * pitchX, height: o.padding + rowCount * pitchY };
}
