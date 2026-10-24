//! Spritesheet packing. Every frame is composited once, scaled with integer
//! nearest-neighbour and copied into a cell; the sheet is pixel-exact.

use std::ops::RangeInclusive;

use dotify_core::{Document, FrameId, Image, PlaybackDirection, Rect, Rgba};
use serde::{Deserialize, Serialize};

use crate::{render_frame, ExportError, MAX_SHEET_SIDE};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SheetLayout {
    /// Every frame once, left to right, top to bottom.
    #[default]
    Grid,
    /// One row per tag (frames shared by tags are repeated), then a row with
    /// untagged frames, if any.
    RowsPerTag,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct SheetOptions {
    pub layout: SheetLayout,
    /// Grid columns; `None` picks a near-square grid.
    pub columns: Option<u32>,
    pub scale: u32,
    /// Transparent pixels around the sheet and between cells.
    pub padding: u32,
    /// Pixels by which each cell's edge is repeated outwards, so texture
    /// sampling at cell borders never picks up a neighbour.
    pub extrude: u32,
}

impl Default for SheetOptions {
    fn default() -> Self {
        Self { layout: SheetLayout::Grid, columns: None, scale: 1, padding: 0, extrude: 0 }
    }
}

pub const MAX_PADDING: u32 = 64;

/// One packed frame image.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SheetCell {
    pub frame: FrameId,
    /// Position of `frame` in the timeline.
    pub index: usize,
    /// Content rectangle in sheet pixels (excludes extrusion).
    pub rect: Rect,
    pub duration_ms: u32,
}

/// A tag resolved against the sheet.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SheetAnimation {
    pub name: String,
    pub direction: PlaybackDirection,
    pub looping: bool,
    pub color: Rgba,
    /// Inclusive timeline range.
    pub from: usize,
    pub to: usize,
    /// Cells for `from..=to` in timeline order (indices into `cells`).
    pub cells: Vec<usize>,
}

impl SheetAnimation {
    /// Cells in the order they are shown, with the direction applied.
    pub fn playback(&self) -> Vec<usize> {
        playback_order(0..=self.cells.len() - 1, self.direction).into_iter().map(|i| self.cells[i]).collect()
    }
}

#[derive(Clone, Debug)]
pub struct Spritesheet {
    pub image: Image,
    /// Scaled size of one frame.
    pub frame_width: u32,
    pub frame_height: u32,
    pub scale: u32,
    pub cells: Vec<SheetCell>,
    /// One per document tag, in document order.
    pub animations: Vec<SheetAnimation>,
}

impl Spritesheet {
    /// The document's tags, or a single looping `default` animation over
    /// every frame when there are none.
    pub fn animations_or_default(&self) -> Vec<SheetAnimation> {
        if !self.animations.is_empty() {
            return self.animations.clone();
        }
        // Without tags every layout packs each frame exactly once, in order.
        vec![SheetAnimation {
            name: "default".into(),
            direction: PlaybackDirection::Forward,
            looping: true,
            color: Rgba::WHITE,
            from: 0,
            to: self.cells.len() - 1,
            cells: (0..self.cells.len()).collect(),
        }]
    }
}

/// Frame indices of `range` in display order for `direction`. Ping-pong
/// does not repeat the end frames, so looping it has no stutter.
pub fn playback_order(range: RangeInclusive<usize>, direction: PlaybackDirection) -> Vec<usize> {
    let (a, b) = (*range.start(), *range.end());
    match direction {
        PlaybackDirection::Forward => range.collect(),
        PlaybackDirection::Reverse => range.rev().collect(),
        PlaybackDirection::PingPong => (a..=b).chain((a + 1..b).rev()).collect(),
    }
}

pub fn pack(doc: &Document, opts: &SheetOptions) -> Result<Spritesheet, ExportError> {
    if opts.padding > MAX_PADDING || opts.extrude > MAX_PADDING {
        return Err(ExportError::InvalidArgument(format!("padding and extrude are limited to {MAX_PADDING} pixels")));
    }
    let frames = doc.frames();
    let ranges = doc.tags().iter().map(|t| doc.tag_range(t)).collect::<Result<Vec<_>, _>>()?;

    // Rows of timeline indices, and for each tag the row/column it starts at.
    let mut rows: Vec<Vec<usize>> = Vec::new();
    let mut tag_starts = Vec::new();
    match opts.layout {
        SheetLayout::Grid => {
            let n = frames.len() as u32;
            let columns = match opts.columns {
                Some(0) => return Err(ExportError::InvalidArgument("columns must be at least 1".into())),
                Some(c) => c.min(n),
                None => (1..=n).find(|c| c * c >= n).unwrap_or(1),
            };
            rows = (0..frames.len()).collect::<Vec<_>>().chunks(columns as usize).map(<[usize]>::to_vec).collect();
            tag_starts.extend(ranges.iter().map(|r| *r.start()));
        }
        SheetLayout::RowsPerTag => {
            for r in &ranges {
                tag_starts.push(rows.iter().map(Vec::len).sum());
                rows.push(r.clone().collect());
            }
            let untagged: Vec<usize> = (0..frames.len()).filter(|i| !ranges.iter().any(|r| r.contains(i))).collect();
            if !untagged.is_empty() {
                rows.push(untagged);
            }
        }
    }

    let (fw, fh) = (doc.width() * opts.scale, doc.height() * opts.scale);
    let (e, pad) = (opts.extrude, opts.padding);
    let (pitch_x, pitch_y) = (fw as u64 + 2 * e as u64 + pad as u64, fh as u64 + 2 * e as u64 + pad as u64);
    let columns = rows.iter().map(Vec::len).max().unwrap_or(1) as u64;
    let (sheet_w, sheet_h) = (pad as u64 + columns * pitch_x, pad as u64 + rows.len() as u64 * pitch_y);
    if sheet_w.max(sheet_h) > MAX_SHEET_SIDE as u64 {
        return Err(ExportError::TooLarge(sheet_w.max(sheet_h)));
    }

    let rendered = frames.iter().map(|f| render_frame(doc, f.id, opts.scale)).collect::<Result<Vec<_>, _>>()?;
    let mut image = Image::new(sheet_w as u32, sheet_h as u32);
    let mut cells = Vec::new();
    for (r, row) in rows.iter().enumerate() {
        for (c, &index) in row.iter().enumerate() {
            let (x0, y0) = (pad + c as u32 * pitch_x as u32, pad + r as u32 * pitch_y as u32);
            blit_extruded(&mut image, &rendered[index], x0, y0, e);
            cells.push(SheetCell {
                frame: frames[index].id,
                index,
                rect: Rect::new(x0 + e, y0 + e, fw, fh),
                duration_ms: frames[index].duration_ms,
            });
        }
    }

    let animations = doc
        .tags()
        .iter()
        .zip(&ranges)
        .zip(&tag_starts)
        .map(|((tag, range), &start)| {
            let cells = match opts.layout {
                // Grid cells are in timeline order, one per frame.
                SheetLayout::Grid => range.clone().collect(),
                SheetLayout::RowsPerTag => (start..start + range.clone().count()).collect(),
            };
            SheetAnimation {
                name: tag.name.clone(),
                direction: tag.direction,
                looping: tag.looping,
                color: tag.color,
                from: *range.start(),
                to: *range.end(),
                cells,
            }
        })
        .collect();

    Ok(Spritesheet { image, frame_width: fw, frame_height: fh, scale: opts.scale, cells, animations })
}

/// Copy `src` to `(x0 + e, y0 + e)` and repeat its edge pixels `e` times
/// outwards on every side.
fn blit_extruded(dst: &mut Image, src: &Image, x0: u32, y0: u32, e: u32) {
    let (w, h) = (src.width(), src.height());
    for y in 0..h + 2 * e {
        let sy = y.saturating_sub(e).min(h - 1);
        for x in 0..w + 2 * e {
            let sx = x.saturating_sub(e).min(w - 1);
            dst.set(x0 + x, y0 + y, src.get(sx, sy));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::animated_doc;

    #[test]
    fn playback_orders() {
        use PlaybackDirection::*;
        assert_eq!(playback_order(2..=5, Forward), vec![2, 3, 4, 5]);
        assert_eq!(playback_order(2..=5, Reverse), vec![5, 4, 3, 2]);
        assert_eq!(playback_order(2..=5, PingPong), vec![2, 3, 4, 5, 4, 3]);
        assert_eq!(playback_order(1..=2, PingPong), vec![1, 2]);
        assert_eq!(playback_order(3..=3, PingPong), vec![3]);
    }

    #[test]
    fn grid_packs_every_frame_once() {
        let doc = animated_doc();
        let sheet = pack(&doc, &SheetOptions::default()).unwrap();
        // 4 frames of 4×3 → 2×2 grid.
        assert_eq!((sheet.image.width(), sheet.image.height()), (8, 6));
        assert_eq!(sheet.cells.len(), 4);
        assert_eq!(sheet.cells[3].rect, Rect::new(4, 3, 4, 3));
        for (i, cell) in sheet.cells.iter().enumerate() {
            let composite = render_frame(&doc, doc.frames()[i].id, 1).unwrap();
            assert_eq!(sheet.image.crop(cell.rect), composite, "cell {i}");
            assert_eq!(cell.duration_ms, doc.frames()[i].duration_ms);
        }
        assert_eq!(sheet.animations[0].cells, vec![0, 1, 2]);
        assert_eq!(sheet.animations[1].cells, vec![3]);
    }

    #[test]
    fn grid_columns_padding_and_scale() {
        let doc = animated_doc();
        let opts = SheetOptions { columns: Some(4), padding: 2, scale: 2, ..Default::default() };
        let sheet = pack(&doc, &opts).unwrap();
        // pad + 4 × (8 + pad) = 42; pad + (6 + pad) = 10.
        assert_eq!((sheet.image.width(), sheet.image.height()), (42, 10));
        assert_eq!(sheet.cells[1].rect, Rect::new(12, 2, 8, 6));
        // Padding stays transparent.
        assert_eq!(sheet.image.get(10, 2), Rgba::TRANSPARENT);
        assert_eq!(sheet.image.get(0, 0), Rgba::TRANSPARENT);
        let composite = render_frame(&doc, doc.frames()[1].id, 2).unwrap();
        assert_eq!(sheet.image.crop(sheet.cells[1].rect), composite);
    }

    #[test]
    fn extrude_repeats_edges() {
        let doc = animated_doc();
        let opts = SheetOptions { columns: Some(1), extrude: 1, padding: 1, ..Default::default() };
        let sheet = pack(&doc, &opts).unwrap();
        // pad + (4 + 2e + pad) = 8 wide, pad + 4 × (3 + 2e + pad) = 25 high.
        assert_eq!((sheet.image.width(), sheet.image.height()), (8, 25));
        let r = sheet.cells[0].rect;
        assert_eq!(r, Rect::new(2, 2, 4, 3));
        let px = |x: u32, y: u32| sheet.image.get(x, y);
        // Frame 0 has red at (0, 0): extruded to the left, top and corner.
        assert_eq!(px(r.x, r.y), Rgba::opaque(255, 0, 0));
        assert_eq!(px(r.x - 1, r.y), Rgba::opaque(255, 0, 0));
        assert_eq!(px(r.x, r.y - 1), Rgba::opaque(255, 0, 0));
        assert_eq!(px(r.x - 1, r.y - 1), Rgba::opaque(255, 0, 0));
        // But not into the padding.
        assert_eq!(px(r.x - 2, r.y), Rgba::TRANSPARENT);
        assert_eq!(sheet.image.crop(r), render_frame(&doc, doc.frames()[0].id, 1).unwrap());
    }

    #[test]
    fn rows_per_tag() {
        let doc = animated_doc();
        let sheet = pack(&doc, &SheetOptions { layout: SheetLayout::RowsPerTag, ..Default::default() }).unwrap();
        // Rows: Walk (0..=2), Hit (3). Every frame is tagged.
        assert_eq!((sheet.image.width(), sheet.image.height()), (12, 6));
        assert_eq!(sheet.cells.iter().map(|c| c.index).collect::<Vec<_>>(), vec![0, 1, 2, 3]);
        assert_eq!(sheet.cells[3].rect, Rect::new(0, 3, 4, 3));
        assert_eq!(sheet.animations[1].cells, vec![3]);
        assert_eq!(sheet.animations[1].playback(), vec![3]);
        assert_eq!(sheet.animations[0].playback(), vec![0, 1, 2, 1]);
    }

    #[test]
    fn rejects_oversized_sheets_and_bad_options() {
        let doc = animated_doc();
        let huge = SheetOptions { scale: 64, columns: Some(4), padding: 64, ..Default::default() };
        assert!(pack(&doc, &huge).is_ok());
        let doc = Document::new("big", 4096, 4096).unwrap();
        assert!(matches!(pack(&doc, &SheetOptions { scale: 8, ..Default::default() }), Err(ExportError::TooLarge(_))));
        assert!(pack(&doc, &SheetOptions { padding: 65, ..Default::default() }).is_err());
        assert!(pack(&doc, &SheetOptions { columns: Some(0), ..Default::default() }).is_err());
    }
}
