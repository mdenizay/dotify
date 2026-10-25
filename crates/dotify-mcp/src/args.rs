//! Tool arguments. Each edit tool's arguments map onto one
//! `dotify_core::Command`; the MCP layer owns these types (and their JSON
//! Schemas) so the core stays free of schema dependencies.

use std::borrow::Cow;

use dotify_core::{
    ArtboardId, Command, FolderId, FrameId, LayerId, PixelWrite, PlaybackDirection, Point, ProjectCommand, Rgba,
};
use dotify_export::meta::Pivot;
use dotify_export::sheet::{SheetLayout, SheetOptions};
use schemars::{json_schema, JsonSchema, Schema, SchemaGenerator};
use serde::Deserialize;

/// Schema for colors, which deserialize through `Rgba`'s own parser.
pub struct HexColor;

impl JsonSchema for HexColor {
    fn schema_name() -> Cow<'static, str> {
        "HexColor".into()
    }

    fn inline_schema() -> bool {
        true
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "pattern": "^#([0-9a-fA-F]{3}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})$",
            "description": "Color as #rgb, #rrggbb or #rrggbbaa (straight alpha; #00000000 is transparent)."
        })
    }
}

#[derive(Deserialize, JsonSchema, Clone, Copy, Debug)]
#[serde(deny_unknown_fields)]
pub struct PixelArg {
    pub x: i32,
    pub y: i32,
    #[schemars(with = "HexColor")]
    pub color: Rgba,
}

#[derive(Deserialize, JsonSchema, Clone, Copy, Debug)]
#[serde(deny_unknown_fields)]
pub struct PointArg {
    pub x: i32,
    pub y: i32,
}

#[derive(Deserialize, JsonSchema, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Forward,
    Reverse,
    PingPong,
}

impl From<Direction> for PlaybackDirection {
    fn from(d: Direction) -> Self {
        match d {
            Direction::Forward => PlaybackDirection::Forward,
            Direction::Reverse => PlaybackDirection::Reverse,
            Direction::PingPong => PlaybackDirection::PingPong,
        }
    }
}

/// Set individual pixels. Colors replace pixels (no blending); coordinates
/// outside the canvas are ignored.
#[derive(Deserialize, JsonSchema, Debug)]
#[serde(deny_unknown_fields)]
pub struct SetPixels {
    /// Layer ID (from project_get / layer_list).
    pub layer: u32,
    /// Frame ID (from project_get).
    pub frame: u32,
    pub pixels: Vec<PixelArg>,
}

/// A 1-pixel Bresenham line between two points, inclusive.
#[derive(Deserialize, JsonSchema, Debug)]
#[serde(deny_unknown_fields)]
pub struct DrawLine {
    pub layer: u32,
    pub frame: u32,
    pub from: PointArg,
    pub to: PointArg,
    #[schemars(with = "HexColor")]
    pub color: Rgba,
}

/// An axis-aligned rectangle (outline unless `filled`) or an ellipse inside
/// that bounding box.
#[derive(Deserialize, JsonSchema, Debug)]
#[serde(deny_unknown_fields)]
pub struct DrawShape {
    pub layer: u32,
    pub frame: u32,
    /// Left edge of the bounding box.
    pub x: i32,
    /// Top edge of the bounding box.
    pub y: i32,
    pub width: u32,
    pub height: u32,
    #[schemars(with = "HexColor")]
    pub color: Rgba,
    #[serde(default)]
    pub filled: bool,
}

/// Bucket fill starting at (x, y).
#[derive(Deserialize, JsonSchema, Debug)]
#[serde(deny_unknown_fields)]
pub struct FillRegion {
    pub layer: u32,
    pub frame: u32,
    pub x: i32,
    pub y: i32,
    #[schemars(with = "HexColor")]
    pub color: Rgba,
    /// Only the 4-connected area of the start color (default), or every
    /// pixel of that color on the cel.
    #[serde(default)]
    pub contiguous: Option<bool>,
}

#[derive(Deserialize, JsonSchema, Debug)]
#[serde(deny_unknown_fields)]
pub struct ReplaceColor {
    pub layer: u32,
    pub frame: u32,
    #[schemars(with = "HexColor")]
    pub from: Rgba,
    #[schemars(with = "HexColor")]
    pub to: Rgba,
}

#[derive(Deserialize, JsonSchema, Debug)]
#[serde(deny_unknown_fields)]
pub struct LayerCreate {
    #[serde(default)]
    pub name: Option<String>,
    /// Position from the bottom (0 = bottom); defaults to the top.
    #[serde(default)]
    pub index: Option<usize>,
}

#[derive(Deserialize, JsonSchema, Debug)]
#[serde(deny_unknown_fields)]
pub struct LayerUpdate {
    pub layer: u32,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub visible: Option<bool>,
    /// Locked layers reject drawing.
    #[serde(default)]
    pub locked: Option<bool>,
    /// 0 (invisible) to 255 (opaque).
    #[serde(default)]
    pub opacity: Option<u8>,
    /// Move the layer to this position from the bottom.
    #[serde(default)]
    pub index: Option<usize>,
}

#[derive(Deserialize, JsonSchema, Debug)]
#[serde(deny_unknown_fields)]
pub struct LayerRef {
    pub layer: u32,
}

#[derive(Deserialize, JsonSchema, Debug)]
#[serde(deny_unknown_fields)]
pub struct AddFrame {
    /// Timeline position; defaults to the end.
    #[serde(default)]
    pub index: Option<usize>,
    /// Defaults to 100 ms.
    #[serde(default)]
    pub duration_ms: Option<u32>,
}

#[derive(Deserialize, JsonSchema, Debug)]
#[serde(deny_unknown_fields)]
pub struct FrameRef {
    pub frame: u32,
}

#[derive(Deserialize, JsonSchema, Debug)]
#[serde(deny_unknown_fields)]
pub struct MoveFrame {
    pub frame: u32,
    /// New timeline position.
    pub index: usize,
}

#[derive(Deserialize, JsonSchema, Debug)]
#[serde(deny_unknown_fields)]
pub struct SetFrameDuration {
    pub frame: u32,
    /// 1 to 60000 ms.
    pub duration_ms: u32,
}

/// Replace the pixels of frame `to` with those of frame `from`, on every
/// unlocked layer or only on `layer`.
#[derive(Deserialize, JsonSchema, Debug)]
#[serde(deny_unknown_fields)]
pub struct CopyFrame {
    pub from: u32,
    pub to: u32,
    #[serde(default)]
    pub layer: Option<u32>,
}

/// Put a one-color silhouette of frame `from` on frame `to` as a new
/// (by default locked) layer, as a reference to draw the next pose over.
#[derive(Deserialize, JsonSchema, Debug)]
#[serde(deny_unknown_fields)]
pub struct CopySilhouette {
    pub from: u32,
    pub to: u32,
    #[schemars(with = "HexColor")]
    pub color: Rgba,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub index: Option<usize>,
    #[serde(default)]
    pub opacity: Option<u8>,
    #[serde(default)]
    pub locked: Option<bool>,
}

/// Name an inclusive frame range as an animation (names are unique).
#[derive(Deserialize, JsonSchema, Debug)]
#[serde(deny_unknown_fields)]
pub struct TagCreate {
    pub name: String,
    /// First frame ID.
    pub from: u32,
    /// Last frame ID.
    pub to: u32,
    #[serde(default)]
    pub direction: Option<Direction>,
    /// Repeat when the range ends (default true).
    #[serde(default, rename = "loop")]
    pub looping: Option<bool>,
    #[serde(default)]
    #[schemars(with = "Option<HexColor>")]
    pub color: Option<Rgba>,
}

/// Change any property of the tag currently named `tag`.
#[derive(Deserialize, JsonSchema, Debug)]
#[serde(deny_unknown_fields)]
pub struct TagUpdate {
    pub tag: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub from: Option<u32>,
    #[serde(default)]
    pub to: Option<u32>,
    #[serde(default)]
    pub direction: Option<Direction>,
    #[serde(default, rename = "loop")]
    pub looping: Option<bool>,
    #[serde(default)]
    #[schemars(with = "Option<HexColor>")]
    pub color: Option<Rgba>,
}

#[derive(Deserialize, JsonSchema, Debug)]
#[serde(deny_unknown_fields)]
pub struct TagRef {
    pub tag: String,
}

#[derive(Deserialize, JsonSchema, Debug)]
#[serde(deny_unknown_fields)]
pub struct ArtboardCreate {
    /// Defaults to "Sprite N".
    #[serde(default)]
    pub name: Option<String>,
    /// Canvas width in pixels (1 to 4096).
    pub width: u32,
    /// Canvas height in pixels (1 to 4096).
    pub height: u32,
    /// Library folder ID to put it in.
    #[serde(default)]
    pub folder: Option<u32>,
    /// Position in the library; defaults to the end.
    #[serde(default)]
    pub index: Option<usize>,
}

#[derive(Deserialize, JsonSchema, Debug)]
#[serde(deny_unknown_fields)]
pub struct ArtboardRename {
    /// Artboard ID (from project_get / artboard_list).
    pub artboard: u32,
    pub name: String,
}

#[derive(Deserialize, JsonSchema, Debug)]
#[serde(deny_unknown_fields)]
pub struct ArtboardRef {
    /// Artboard ID (from project_get / artboard_list).
    pub artboard: u32,
}

/// The palette shared by every artboard of the project.
#[derive(Deserialize, JsonSchema, Debug)]
#[serde(deny_unknown_fields)]
pub struct SetPalette {
    pub name: String,
    /// Up to 256 colors.
    #[schemars(with = "Vec<HexColor>")]
    pub colors: Vec<Rgba>,
}

/// One edit, as used by `batch`. `op` is the name of the matching single
/// edit tool; the other fields are that tool's arguments without `project`.
/// Artboard edits may name their `artboard`; the batch's `artboard` is the
/// default.
#[derive(Deserialize, JsonSchema, Debug)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum EditOp {
    SetPixels(SetPixels),
    DrawLine(DrawLine),
    DrawRectangle(DrawShape),
    DrawEllipse(DrawShape),
    FillRegion(FillRegion),
    ReplaceColor(ReplaceColor),
    LayerCreate(LayerCreate),
    LayerUpdate(LayerUpdate),
    LayerDelete(LayerRef),
    AddFrame(AddFrame),
    DuplicateFrame(FrameRef),
    DeleteFrame(FrameRef),
    MoveFrame(MoveFrame),
    SetFrameDuration(SetFrameDuration),
    CopyFrame(CopyFrame),
    CopySilhouette(CopySilhouette),
    TagCreate(TagCreate),
    TagUpdate(TagUpdate),
    TagDelete(TagRef),
    SetPalette(SetPalette),
    ArtboardCreate(ArtboardCreate),
    ArtboardRename(ArtboardRename),
    ArtboardDuplicate(ArtboardRef),
    ArtboardDelete(ArtboardRef),
}

/// Ops that act on the project library (or its shared palette) rather than
/// on one artboard. They take no `artboard` default.
pub const LIBRARY_OPS: &[&str] =
    &["set_palette", "artboard_create", "artboard_rename", "artboard_duplicate", "artboard_delete"];

/// What an [`EditOp`] compiles to.
pub enum OpCommand {
    /// A document command for one artboard.
    Document(Command),
    /// A project-level command.
    Project(ProjectCommand),
}

impl EditOp {
    /// Ops that remove pixels or structure and need `confirm: true`.
    pub fn is_destructive(&self) -> bool {
        matches!(self, EditOp::LayerDelete(_) | EditOp::DeleteFrame(_) | EditOp::ArtboardDelete(_))
    }

    pub fn into_command(self) -> OpCommand {
        let a = ArtboardId;
        let project = match self {
            EditOp::SetPalette(p) => {
                ProjectCommand::SetPalette { palette: dotify_core::Palette { name: p.name, colors: p.colors } }
            }
            EditOp::ArtboardCreate(p) => ProjectCommand::CreateArtboard {
                name: p.name,
                width: p.width,
                height: p.height,
                folder: p.folder.map(FolderId),
                index: p.index,
            },
            EditOp::ArtboardRename(p) => ProjectCommand::RenameArtboard { artboard: a(p.artboard), name: p.name },
            EditOp::ArtboardDuplicate(p) => ProjectCommand::DuplicateArtboard { artboard: a(p.artboard) },
            EditOp::ArtboardDelete(p) => ProjectCommand::DeleteArtboard { artboard: a(p.artboard) },
            doc => return OpCommand::Document(doc.into_document_command()),
        };
        OpCommand::Project(project)
    }

    fn into_document_command(self) -> Command {
        let (l, f) = (LayerId, FrameId);
        match self {
            EditOp::SetPixels(a) => Command::SetPixels {
                layer: l(a.layer),
                frame: f(a.frame),
                pixels: a.pixels.into_iter().map(|p| PixelWrite { x: p.x, y: p.y, color: p.color }).collect(),
            },
            EditOp::DrawLine(a) => Command::DrawLine {
                layer: l(a.layer),
                frame: f(a.frame),
                from: Point::new(a.from.x, a.from.y),
                to: Point::new(a.to.x, a.to.y),
                color: a.color,
            },
            EditOp::DrawRectangle(a) => Command::DrawRectangle {
                layer: l(a.layer),
                frame: f(a.frame),
                x: a.x,
                y: a.y,
                width: a.width,
                height: a.height,
                color: a.color,
                filled: a.filled,
            },
            EditOp::DrawEllipse(a) => Command::DrawEllipse {
                layer: l(a.layer),
                frame: f(a.frame),
                x: a.x,
                y: a.y,
                width: a.width,
                height: a.height,
                color: a.color,
                filled: a.filled,
            },
            EditOp::FillRegion(a) => Command::FillRegion {
                layer: l(a.layer),
                frame: f(a.frame),
                x: a.x,
                y: a.y,
                color: a.color,
                contiguous: a.contiguous.unwrap_or(true),
            },
            EditOp::ReplaceColor(a) => {
                Command::ReplaceColor { layer: l(a.layer), frame: f(a.frame), from: a.from, to: a.to }
            }
            EditOp::LayerCreate(a) => Command::CreateLayer { name: a.name, index: a.index },
            EditOp::LayerUpdate(a) => {
                let update = Command::UpdateLayer {
                    layer: l(a.layer),
                    name: a.name,
                    visible: a.visible,
                    locked: a.locked,
                    opacity: a.opacity,
                };
                match a.index {
                    None => update,
                    Some(index) => Command::Batch {
                        label: Some("Layer Properties".into()),
                        commands: vec![update, Command::MoveLayer { layer: l(a.layer), index }],
                    },
                }
            }
            EditOp::LayerDelete(a) => Command::DeleteLayer { layer: l(a.layer) },
            EditOp::AddFrame(a) => Command::AddFrame { index: a.index, duration_ms: a.duration_ms },
            EditOp::DuplicateFrame(a) => Command::DuplicateFrame { frame: f(a.frame) },
            EditOp::DeleteFrame(a) => Command::DeleteFrame { frame: f(a.frame) },
            EditOp::MoveFrame(a) => Command::MoveFrame { frame: f(a.frame), index: a.index },
            EditOp::SetFrameDuration(a) => Command::SetFrameDuration { frame: f(a.frame), duration_ms: a.duration_ms },
            EditOp::CopyFrame(a) => Command::CopyFrame { from: f(a.from), to: f(a.to), layer: a.layer.map(l) },
            EditOp::CopySilhouette(a) => Command::CopySilhouette {
                from: f(a.from),
                to: f(a.to),
                color: a.color,
                name: a.name,
                index: a.index,
                opacity: a.opacity,
                locked: a.locked.unwrap_or(true),
            },
            EditOp::TagCreate(a) => Command::CreateTag {
                name: a.name,
                from: f(a.from),
                to: f(a.to),
                direction: a.direction.map(Into::into),
                looping: a.looping,
                color: a.color,
            },
            EditOp::TagUpdate(a) => Command::UpdateTag {
                tag: a.tag,
                name: a.name,
                from: a.from.map(f),
                to: a.to.map(f),
                direction: a.direction.map(Into::into),
                looping: a.looping,
                color: a.color,
            },
            EditOp::TagDelete(a) => Command::DeleteTag { tag: a.tag },
            EditOp::SetPalette(_)
            | EditOp::ArtboardCreate(_)
            | EditOp::ArtboardRename(_)
            | EditOp::ArtboardDuplicate(_)
            | EditOp::ArtboardDelete(_) => unreachable!("library ops are handled by into_command"),
        }
    }
}

// ── Non-edit tools ──────────────────────────────────────────────────────

#[derive(Deserialize, JsonSchema, Debug)]
#[serde(deny_unknown_fields)]
pub struct Empty {}

#[derive(Deserialize, JsonSchema, Debug)]
#[serde(deny_unknown_fields)]
pub struct ProjectCreate {
    /// Canvas width in pixels (1 to 4096).
    pub width: u32,
    /// Canvas height in pixels (1 to 4096).
    pub height: u32,
    /// Defaults to the file name, or "Untitled".
    #[serde(default)]
    pub name: Option<String>,
    /// Where to save the new `.dotify` file (relative paths are inside the
    /// first root). Without it the project lives in memory until
    /// project_save is given a path.
    #[serde(default)]
    pub path: Option<String>,
    /// Required to replace an existing file at `path`.
    #[serde(default)]
    pub confirm: bool,
}

#[derive(Deserialize, JsonSchema, Debug)]
#[serde(deny_unknown_fields)]
pub struct ProjectOpen {
    /// Path to a `.dotify` file inside the allowed roots.
    pub path: String,
}

#[derive(Deserialize, JsonSchema, Debug)]
#[serde(deny_unknown_fields)]
pub struct ProjectRef {
    /// Project handle returned by project_create / project_open.
    pub project: String,
}

#[derive(Deserialize, JsonSchema, Debug)]
#[serde(deny_unknown_fields)]
pub struct ArtboardScoped {
    pub project: String,
    /// Artboard ID; may be left out when the project has one artboard.
    #[serde(default)]
    pub artboard: Option<u32>,
}

#[derive(Deserialize, JsonSchema, Debug)]
#[serde(deny_unknown_fields)]
pub struct ProjectSave {
    pub project: String,
    /// Save to this `.dotify` path instead of the project's own file (it
    /// becomes the project's file).
    #[serde(default)]
    pub path: Option<String>,
    /// Required when the target is an existing file other than the
    /// project's own.
    #[serde(default)]
    pub confirm: bool,
}

#[derive(Deserialize, JsonSchema, Debug)]
#[serde(deny_unknown_fields)]
pub struct ProjectClose {
    pub project: String,
    /// Required to discard unsaved changes.
    #[serde(default)]
    pub confirm: bool,
}

#[derive(Deserialize, JsonSchema, Debug)]
#[serde(deny_unknown_fields)]
pub struct GetCanvas {
    pub project: String,
    /// Artboard ID; may be left out when the project has one artboard.
    #[serde(default)]
    pub artboard: Option<u32>,
    /// Frame ID; defaults to the first frame.
    #[serde(default)]
    pub frame: Option<u32>,
    /// Only this layer's pixels (ignores its visibility); defaults to the
    /// composite of all visible layers.
    #[serde(default)]
    pub layer: Option<u32>,
    /// Integer upscale for the PNG (1 to 32). Defaults to a factor that
    /// makes the image about 256 pixels on its long side.
    #[serde(default)]
    pub scale: Option<u32>,
    /// Also return the unscaled pixels as text rows, one character per
    /// pixel with a color legend ("." is transparent). Up to 128×128 pixels
    /// and 62 colors.
    #[serde(default)]
    pub include_pixels: bool,
}

#[derive(Deserialize, JsonSchema, Debug)]
#[serde(deny_unknown_fields)]
pub struct ExportPng {
    pub project: String,
    /// Artboard ID; may be left out when the project has one artboard.
    #[serde(default)]
    pub artboard: Option<u32>,
    /// Output `.png` path.
    pub path: String,
    /// Frame ID; defaults to the first frame.
    #[serde(default)]
    pub frame: Option<u32>,
    /// Integer upscale (1 to 64).
    #[serde(default)]
    pub scale: Option<u32>,
    /// Required to replace existing files.
    #[serde(default)]
    pub confirm: bool,
}

#[derive(Deserialize, JsonSchema, Debug)]
#[serde(deny_unknown_fields)]
pub struct ExportGif {
    pub project: String,
    /// Artboard ID; may be left out when the project has one artboard.
    #[serde(default)]
    pub artboard: Option<u32>,
    /// Output `.gif` path.
    pub path: String,
    /// Only this tag, with its direction and loop setting; defaults to every
    /// frame, looping.
    #[serde(default)]
    pub tag: Option<String>,
    #[serde(default)]
    pub scale: Option<u32>,
    #[serde(default)]
    pub confirm: bool,
}

#[derive(Deserialize, JsonSchema, Clone, Copy, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Layout {
    /// Every frame once, left to right, top to bottom.
    Grid,
    /// One row per tag, then untagged frames.
    RowsPerTag,
}

#[derive(Deserialize, JsonSchema, Clone, Copy, Debug)]
#[serde(deny_unknown_fields)]
pub struct PivotArg {
    pub x: f64,
    pub y: f64,
}

#[derive(Deserialize, JsonSchema, Default, Clone, Copy, Debug)]
#[serde(deny_unknown_fields)]
pub struct SheetArgs {
    #[serde(default)]
    pub layout: Option<Layout>,
    /// Grid columns; defaults to a near-square grid.
    #[serde(default)]
    pub columns: Option<u32>,
    #[serde(default)]
    pub scale: Option<u32>,
    /// Transparent pixels around and between frames.
    #[serde(default)]
    pub padding: Option<u32>,
    /// Repeat each frame's edge pixels outwards by this many pixels.
    #[serde(default)]
    pub extrude: Option<u32>,
    /// Sprite origin in frame pixels; defaults to the centre.
    #[serde(default)]
    pub pivot: Option<PivotArg>,
}

impl SheetArgs {
    pub fn options(&self) -> SheetOptions {
        let d = SheetOptions::default();
        SheetOptions {
            layout: match self.layout {
                Some(Layout::RowsPerTag) => SheetLayout::RowsPerTag,
                Some(Layout::Grid) | None => SheetLayout::Grid,
            },
            columns: self.columns,
            scale: self.scale.unwrap_or(d.scale),
            padding: self.padding.unwrap_or(d.padding),
            extrude: self.extrude.unwrap_or(d.extrude),
        }
    }

    pub fn pivot(&self) -> Option<Pivot> {
        self.pivot.map(|p| Pivot { x: p.x, y: p.y })
    }
}

#[derive(Deserialize, JsonSchema, Debug)]
#[serde(deny_unknown_fields)]
pub struct ExportSpritesheet {
    pub project: String,
    /// Artboard ID; may be left out when the project has one artboard.
    #[serde(default)]
    pub artboard: Option<u32>,
    /// Output `.png` path; the JSON metadata is written next to it.
    pub path: String,
    #[serde(default)]
    pub sheet: SheetArgs,
    #[serde(default)]
    pub confirm: bool,
}

#[derive(Deserialize, JsonSchema, Debug)]
#[serde(deny_unknown_fields)]
pub struct ExportGodot {
    pub project: String,
    /// Artboard ID; may be left out when the project has one artboard.
    #[serde(default)]
    pub artboard: Option<u32>,
    /// Output directory; `<name>.png`, `<name>.tres` (SpriteFrames, one
    /// animation per tag) and `<name>.json` are written into it.
    pub dir: String,
    /// File stem; defaults to the document name.
    #[serde(default)]
    pub name: Option<String>,
    /// Texture directory inside the Godot project, such as `res://sprites`.
    /// By default the .tres refers to the PNG relative to itself.
    #[serde(default)]
    pub res_dir: Option<String>,
    #[serde(default)]
    pub sheet: SheetArgs,
    #[serde(default)]
    pub confirm: bool,
}

#[derive(Deserialize, JsonSchema, Debug)]
#[serde(deny_unknown_fields)]
pub struct Batch {
    pub project: String,
    /// Edits applied in order as one atomic, undoable transaction. If any
    /// fails, none is applied.
    pub ops: Vec<EditOp>,
    /// Undo label.
    #[serde(default)]
    pub label: Option<String>,
    /// Refuse to apply unless the project is at this revision.
    #[serde(default)]
    pub expected_revision: Option<u64>,
    /// Default artboard for artboard edits that don't name one; may be
    /// left out when the project has one artboard.
    #[serde(default)]
    pub artboard: Option<u32>,
    /// Required when the batch deletes layers, frames or artboards.
    #[serde(default)]
    pub confirm: bool,
}

/// [`Batch`] as parsed: ops stay raw until each one's `artboard` is taken
/// out. `Batch` itself only describes the schema.
#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct BatchArgs {
    pub project: String,
    pub ops: Vec<serde_json::Map<String, serde_json::Value>>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub expected_revision: Option<u64>,
    #[serde(default)]
    pub artboard: Option<u32>,
    #[serde(default)]
    pub confirm: bool,
}
