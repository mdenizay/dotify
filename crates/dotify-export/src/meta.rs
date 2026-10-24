//! Engine-neutral JSON metadata for a packed spritesheet. All coordinates
//! are in spritesheet pixels (already multiplied by `scale`).

use dotify_core::{Document, PlaybackDirection, Rgba};
use serde::{Deserialize, Serialize};

use crate::sheet::Spritesheet;

/// Bumped on incompatible changes to the JSON layout.
pub const METADATA_VERSION: u32 = 1;

/// Sprite origin in unscaled frame pixels. Defaults to the frame centre.
#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub struct Pivot {
    pub x: f64,
    pub y: f64,
}

impl Pivot {
    pub fn center(doc: &Document) -> Self {
        Self { x: doc.width() as f64 / 2.0, y: doc.height() as f64 / 2.0 }
    }
}

#[derive(Serialize)]
pub struct Metadata<'a> {
    pub version: u32,
    pub generator: String,
    pub name: &'a str,
    pub image: &'a str,
    pub size: Size,
    pub scale: u32,
    pub frame_size: Size,
    pub pivot: Pivot,
    pub frames: Vec<FrameMeta>,
    pub tags: Vec<TagMeta<'a>>,
}

#[derive(Serialize)]
pub struct Size {
    pub w: u32,
    pub h: u32,
}

#[derive(Serialize)]
pub struct FrameMeta {
    /// Position in the timeline.
    pub index: usize,
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    pub duration_ms: u32,
}

#[derive(Serialize)]
pub struct TagMeta<'a> {
    pub name: &'a str,
    /// Inclusive timeline range.
    pub from: usize,
    pub to: usize,
    pub direction: PlaybackDirection,
    #[serde(rename = "loop")]
    pub looping: bool,
    pub color: Rgba,
    /// Indices into `frames`, in playback order with the direction applied.
    pub sequence: Vec<usize>,
}

/// Build the metadata for `sheet`, whose image is saved as `image`.
pub fn metadata<'a>(doc: &'a Document, sheet: &'a Spritesheet, image: &'a str, pivot: Pivot) -> Metadata<'a> {
    let s = sheet.scale as f64;
    Metadata {
        version: METADATA_VERSION,
        generator: concat!("dotify ", env!("CARGO_PKG_VERSION")).into(),
        name: doc.name(),
        image,
        size: Size { w: sheet.image.width(), h: sheet.image.height() },
        scale: sheet.scale,
        frame_size: Size { w: sheet.frame_width, h: sheet.frame_height },
        pivot: Pivot { x: pivot.x * s, y: pivot.y * s },
        frames: sheet
            .cells
            .iter()
            .map(|c| FrameMeta {
                index: c.index,
                x: c.rect.x,
                y: c.rect.y,
                w: c.rect.width,
                h: c.rect.height,
                duration_ms: c.duration_ms,
            })
            .collect(),
        tags: sheet
            .animations
            .iter()
            .map(|a| TagMeta {
                name: &a.name,
                from: a.from,
                to: a.to,
                direction: a.direction,
                looping: a.looping,
                color: a.color,
                sequence: a.playback(),
            })
            .collect(),
    }
}

pub fn metadata_json(doc: &Document, sheet: &Spritesheet, image: &str, pivot: Pivot) -> String {
    let mut json = serde_json::to_string_pretty(&metadata(doc, sheet, image, pivot)).expect("metadata serializes");
    json.push('\n');
    json
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sheet::{pack, SheetLayout, SheetOptions};
    use crate::tests::animated_doc;

    #[test]
    fn describes_frames_and_tags() {
        let doc = animated_doc();
        let opts = SheetOptions { layout: SheetLayout::RowsPerTag, scale: 2, padding: 1, ..Default::default() };
        let sheet = pack(&doc, &opts).unwrap();
        let v: serde_json::Value =
            serde_json::from_str(&metadata_json(&doc, &sheet, "hero.png", Pivot { x: 2.0, y: 3.0 })).unwrap();
        assert_eq!(v["image"], "hero.png");
        assert_eq!(v["scale"], 2);
        assert_eq!(v["frame_size"], serde_json::json!({ "w": 8, "h": 6 }));
        assert_eq!(v["pivot"], serde_json::json!({ "x": 4.0, "y": 6.0 }));
        assert_eq!(
            v["frames"][1],
            serde_json::json!({ "index": 1, "x": 10, "y": 1, "w": 8, "h": 6, "duration_ms": 100 })
        );
        assert_eq!(v["frames"][3]["y"], 8);
        assert_eq!(
            v["tags"][0],
            serde_json::json!({ "name": "Walk", "from": 0, "to": 2, "direction": "ping_pong", "loop": true,
                "color": "#5b8cffff", "sequence": [0, 1, 2, 1] })
        );
        assert_eq!(v["tags"][1]["loop"], false);
        assert_eq!(v["tags"][1]["sequence"], serde_json::json!([3]));
    }
}
