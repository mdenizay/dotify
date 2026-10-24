//! The public editing vocabulary shared by the desktop UI, CLI, MCP server
//! and AI assistant.
//!
//! Drawing semantics: colors *replace* pixels (no blending), coordinates
//! outside the canvas are clipped, and a command that changes nothing
//! produces no history entry.

use serde::{Deserialize, Serialize};

use crate::color::Rgba;
use crate::error::{CoreError, CoreResult};
use crate::geometry::{Bounds, Point};
use crate::model::{
    AnimationTag, Document, Frame, FrameId, Layer, LayerId, Palette, PlaybackDirection, DEFAULT_FRAME_DURATION_MS,
};
use crate::ops::Op;
use crate::raster;

pub const MAX_PALETTE_COLORS: usize = 256;
pub const MAX_FRAME_DURATION_MS: u32 = 60_000;
pub const MAX_NAME_LEN: usize = 128;
pub const MAX_TAGS: usize = 256;

/// Colors assigned to new tags that don't specify one, in rotation.
const TAG_COLORS: [Rgba; 8] = [
    Rgba::opaque(0x5b, 0x8c, 0xff),
    Rgba::opaque(0xff, 0x7a, 0x59),
    Rgba::opaque(0x4c, 0xc9, 0x7a),
    Rgba::opaque(0xf5, 0xc5, 0x42),
    Rgba::opaque(0xc0, 0x6c, 0xf0),
    Rgba::opaque(0x3e, 0xc9, 0xc9),
    Rgba::opaque(0xff, 0x5f, 0x9e),
    Rgba::opaque(0x9a, 0xa0, 0xa6),
];

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct PixelWrite {
    pub x: i32,
    pub y: i32,
    pub color: Rgba,
}

fn default_true() -> bool {
    true
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Command {
    SetPixels {
        layer: LayerId,
        frame: FrameId,
        pixels: Vec<PixelWrite>,
    },
    DrawLine {
        layer: LayerId,
        frame: FrameId,
        from: Point,
        to: Point,
        color: Rgba,
    },
    DrawRectangle {
        layer: LayerId,
        frame: FrameId,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
        color: Rgba,
        #[serde(default)]
        filled: bool,
    },
    DrawEllipse {
        layer: LayerId,
        frame: FrameId,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
        color: Rgba,
        #[serde(default)]
        filled: bool,
    },
    FillRegion {
        layer: LayerId,
        frame: FrameId,
        x: i32,
        y: i32,
        color: Rgba,
        #[serde(default = "default_true")]
        contiguous: bool,
    },
    ReplaceColor {
        layer: LayerId,
        frame: FrameId,
        from: Rgba,
        to: Rgba,
    },
    ClearCel {
        layer: LayerId,
        frame: FrameId,
    },
    CreateLayer {
        #[serde(default)]
        name: Option<String>,
        /// Position from the bottom; defaults to the top of the stack.
        #[serde(default)]
        index: Option<usize>,
    },
    DuplicateLayer {
        layer: LayerId,
    },
    DeleteLayer {
        layer: LayerId,
    },
    UpdateLayer {
        layer: LayerId,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        visible: Option<bool>,
        #[serde(default)]
        locked: Option<bool>,
        #[serde(default)]
        opacity: Option<u8>,
    },
    MoveLayer {
        layer: LayerId,
        index: usize,
    },
    AddFrame {
        /// Position in the timeline; defaults to the end.
        #[serde(default)]
        index: Option<usize>,
        #[serde(default)]
        duration_ms: Option<u32>,
    },
    DuplicateFrame {
        frame: FrameId,
    },
    DeleteFrame {
        frame: FrameId,
    },
    MoveFrame {
        frame: FrameId,
        index: usize,
    },
    SetFrameDuration {
        frame: FrameId,
        duration_ms: u32,
    },
    /// Name an inclusive frame range as an animation. Names are unique.
    CreateTag {
        name: String,
        from: FrameId,
        to: FrameId,
        #[serde(default)]
        direction: Option<PlaybackDirection>,
        #[serde(default, rename = "loop")]
        looping: Option<bool>,
        /// Defaults to the next color in a fixed rotation.
        #[serde(default)]
        color: Option<Rgba>,
    },
    /// Change any property of the tag currently named `tag`.
    UpdateTag {
        tag: String,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        from: Option<FrameId>,
        #[serde(default)]
        to: Option<FrameId>,
        #[serde(default)]
        direction: Option<PlaybackDirection>,
        #[serde(default, rename = "loop")]
        looping: Option<bool>,
        #[serde(default)]
        color: Option<Rgba>,
    },
    DeleteTag {
        tag: String,
    },
    SetPalette {
        palette: Palette,
    },
    RenameDocument {
        name: String,
    },
    /// Apply all commands atomically as one undo step.
    Batch {
        #[serde(default)]
        label: Option<String>,
        commands: Vec<Command>,
    },
}

/// An object created by a command, reported back to the caller.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Created {
    Layer(LayerId),
    Frame(FrameId),
}

pub(crate) struct Compiled {
    pub ops: Vec<Op>,
    pub created: Option<Created>,
}

impl Compiled {
    fn ops(ops: Vec<Op>) -> CoreResult<Self> {
        Ok(Self { ops, created: None })
    }

    fn none() -> CoreResult<Self> {
        Self::ops(Vec::new())
    }
}

impl Command {
    /// Human-readable label used in undo/redo menus.
    pub fn label(&self) -> String {
        match self {
            Command::SetPixels { .. } => "Draw",
            Command::DrawLine { .. } => "Line",
            Command::DrawRectangle { .. } => "Rectangle",
            Command::DrawEllipse { .. } => "Ellipse",
            Command::FillRegion { .. } => "Fill",
            Command::ReplaceColor { .. } => "Replace Color",
            Command::ClearCel { .. } => "Clear",
            Command::CreateLayer { .. } => "New Layer",
            Command::DuplicateLayer { .. } => "Duplicate Layer",
            Command::DeleteLayer { .. } => "Delete Layer",
            Command::UpdateLayer { .. } => "Layer Properties",
            Command::MoveLayer { .. } => "Move Layer",
            Command::AddFrame { .. } => "New Frame",
            Command::DuplicateFrame { .. } => "Duplicate Frame",
            Command::DeleteFrame { .. } => "Delete Frame",
            Command::MoveFrame { .. } => "Move Frame",
            Command::SetFrameDuration { .. } => "Frame Duration",
            Command::CreateTag { .. } => "New Tag",
            Command::UpdateTag { .. } => "Tag Properties",
            Command::DeleteTag { .. } => "Delete Tag",
            Command::SetPalette { .. } => "Palette",
            Command::RenameDocument { .. } => "Rename",
            Command::Batch { label, .. } => return label.clone().unwrap_or_else(|| "Batch".into()),
        }
        .to_string()
    }

    /// Compile a non-batch command against the current document.
    pub(crate) fn compile(&self, doc: &Document) -> CoreResult<Compiled> {
        match self {
            Command::SetPixels { layer, frame, pixels } => {
                pixel_op(doc, *layer, *frame, pixels.iter().map(|p| (p.x, p.y, p.color)))
            }
            Command::DrawLine { layer, frame, from, to, color } => {
                let mut px = Vec::new();
                raster::line(*from, *to, |x, y| px.push((x, y, *color)));
                pixel_op(doc, *layer, *frame, px)
            }
            Command::DrawRectangle { layer, frame, x, y, width, height, color, filled } => {
                let mut px = Vec::new();
                raster::rectangle(*x, *y, *width, *height, *filled, |x, y| px.push((x, y, *color)));
                pixel_op(doc, *layer, *frame, px)
            }
            Command::DrawEllipse { layer, frame, x, y, width, height, color, filled } => {
                let mut px = Vec::new();
                raster::ellipse(*x, *y, *width, *height, *filled, |x, y| px.push((x, y, *color)));
                pixel_op(doc, *layer, *frame, px)
            }
            Command::FillRegion { layer, frame, x, y, color, contiguous } => {
                check_cel(doc, *layer, *frame)?;
                let img = doc.cel_image(*layer, *frame);
                let px = raster::flood_fill_pixels(&img, *x, *y, *color, *contiguous);
                pixel_op(doc, *layer, *frame, px.into_iter().map(|(x, y)| (x as i32, y as i32, *color)))
            }
            Command::ReplaceColor { layer, frame, from, to } => {
                check_cel(doc, *layer, *frame)?;
                let Some(img) = doc.cel(*layer, *frame) else {
                    // Only transparent pixels exist.
                    return if from.a == 0 {
                        Command::FillRegion { layer: *layer, frame: *frame, x: 0, y: 0, color: *to, contiguous: false }
                            .compile(doc)
                    } else {
                        Compiled::none()
                    };
                };
                let mut px = Vec::new();
                for y in 0..img.height() {
                    for x in 0..img.width() {
                        if img.get(x, y).matches(*from) {
                            px.push((x as i32, y as i32, *to));
                        }
                    }
                }
                pixel_op(doc, *layer, *frame, px)
            }
            Command::ClearCel { layer, frame } => {
                check_cel(doc, *layer, *frame)?;
                match doc.cel(*layer, *frame).and_then(|c| c.opaque_bounds()) {
                    None => Compiled::none(),
                    Some(rect) => Compiled::ops(vec![Op::PutRegion {
                        layer: *layer,
                        frame: *frame,
                        rect,
                        pixels: vec![0; rect.width as usize * rect.height as usize * 4],
                    }]),
                }
            }
            Command::CreateLayer { name, index } => {
                let id = LayerId(doc.alloc_id());
                let index = index.unwrap_or(doc.layers.len());
                if index > doc.layers.len() {
                    return Err(invalid(format!("layer index {index} out of range")));
                }
                let name = match name {
                    Some(n) => validate_name(n)?,
                    None => next_layer_name(doc),
                };
                Ok(Compiled {
                    ops: vec![
                        Op::SetNextId(id.0 + 1),
                        Op::InsertLayer { index, layer: Layer::new(id, name), cels: Vec::new() },
                    ],
                    created: Some(Created::Layer(id)),
                })
            }
            Command::DuplicateLayer { layer } => {
                let src_index = doc.layer_index(*layer)?;
                let src = &doc.layers[src_index];
                let id = LayerId(doc.alloc_id());
                let mut copy = src.clone();
                copy.id = id;
                copy.locked = false;
                copy.name = truncate_name(format!("{} copy", src.name));
                let cels =
                    doc.cels.iter().filter(|((l, _), _)| l == layer).map(|((_, f), img)| (*f, img.clone())).collect();
                Ok(Compiled {
                    ops: vec![Op::SetNextId(id.0 + 1), Op::InsertLayer { index: src_index + 1, layer: copy, cels }],
                    created: Some(Created::Layer(id)),
                })
            }
            Command::DeleteLayer { layer } => {
                doc.layer(*layer)?;
                if doc.layers.len() == 1 {
                    return Err(CoreError::LastLayer);
                }
                Compiled::ops(vec![Op::RemoveLayer { id: *layer }])
            }
            Command::UpdateLayer { layer, name, visible, locked, opacity } => {
                let mut l = doc.layer(*layer)?.clone();
                if let Some(n) = name {
                    l.name = validate_name(n)?;
                }
                if let Some(v) = visible {
                    l.visible = *v;
                }
                if let Some(v) = locked {
                    l.locked = *v;
                }
                if let Some(v) = opacity {
                    l.opacity = *v;
                }
                if &l == doc.layer(*layer)? {
                    return Compiled::none();
                }
                Compiled::ops(vec![Op::SetLayer { layer: l }])
            }
            Command::MoveLayer { layer, index } => {
                let from = doc.layer_index(*layer)?;
                if *index >= doc.layers.len() {
                    return Err(invalid(format!("layer index {index} out of range")));
                }
                if from == *index {
                    return Compiled::none();
                }
                Compiled::ops(vec![Op::MoveLayer { id: *layer, index: *index }])
            }
            Command::AddFrame { index, duration_ms } => {
                let index = index.unwrap_or(doc.frames.len());
                if index > doc.frames.len() {
                    return Err(invalid(format!("frame index {index} out of range")));
                }
                let duration_ms = validate_duration(duration_ms.unwrap_or(DEFAULT_FRAME_DURATION_MS))?;
                let id = FrameId(doc.alloc_id());
                Ok(Compiled {
                    ops: vec![
                        Op::SetNextId(id.0 + 1),
                        Op::InsertFrame { index, frame: Frame { id, duration_ms }, cels: Vec::new() },
                    ],
                    created: Some(Created::Frame(id)),
                })
            }
            Command::DuplicateFrame { frame } => {
                let src_index = doc.frame_index(*frame)?;
                let id = FrameId(doc.alloc_id());
                let copy = Frame { id, duration_ms: doc.frames[src_index].duration_ms };
                let cels =
                    doc.cels.iter().filter(|((_, f), _)| f == frame).map(|((l, _), img)| (*l, img.clone())).collect();
                Ok(Compiled {
                    ops: vec![Op::SetNextId(id.0 + 1), Op::InsertFrame { index: src_index + 1, frame: copy, cels }],
                    created: Some(Created::Frame(id)),
                })
            }
            Command::DeleteFrame { frame } => {
                let index = doc.frame_index(*frame)?;
                if doc.frames.len() == 1 {
                    return Err(CoreError::LastFrame);
                }
                let mut ops = Vec::new();
                if doc.tags.iter().any(|t| t.from == *frame || t.to == *frame) {
                    ops.push(Op::SetTags(tags_without_frame(doc, index)));
                }
                ops.push(Op::RemoveFrame { id: *frame });
                Compiled::ops(ops)
            }
            Command::MoveFrame { frame, index } => {
                let from = doc.frame_index(*frame)?;
                if *index >= doc.frames.len() {
                    return Err(invalid(format!("frame index {index} out of range")));
                }
                if from == *index {
                    return Compiled::none();
                }
                let mut ops = Vec::new();
                if let Some(tags) = tags_after_move(doc, from, *index) {
                    ops.push(Op::SetTags(tags));
                }
                ops.push(Op::MoveFrame { id: *frame, index: *index });
                Compiled::ops(ops)
            }
            Command::SetFrameDuration { frame, duration_ms } => {
                let f = doc.frame(*frame)?;
                let duration_ms = validate_duration(*duration_ms)?;
                if f.duration_ms == duration_ms {
                    return Compiled::none();
                }
                Compiled::ops(vec![Op::SetFrame { frame: Frame { id: *frame, duration_ms } }])
            }
            Command::CreateTag { name, from, to, direction, looping, color } => {
                if doc.tags.len() >= MAX_TAGS {
                    return Err(invalid(format!("documents are limited to {MAX_TAGS} tags")));
                }
                let tag = AnimationTag {
                    name: validate_name(name)?,
                    from: *from,
                    to: *to,
                    direction: direction.unwrap_or_default(),
                    looping: looping.unwrap_or(true),
                    color: color.unwrap_or(TAG_COLORS[doc.tags.len() % TAG_COLORS.len()]),
                };
                validate_tag(doc, &tag, None)?;
                let mut tags = doc.tags.clone();
                tags.push(tag);
                Compiled::ops(vec![Op::SetTags(tags)])
            }
            Command::UpdateTag { tag, name, from, to, direction, looping, color } => {
                let index = tag_index(doc, tag)?;
                let mut t = doc.tags[index].clone();
                if let Some(n) = name {
                    t.name = validate_name(n)?;
                }
                t.from = from.unwrap_or(t.from);
                t.to = to.unwrap_or(t.to);
                t.direction = direction.unwrap_or(t.direction);
                t.looping = looping.unwrap_or(t.looping);
                t.color = color.unwrap_or(t.color);
                validate_tag(doc, &t, Some(index))?;
                if t == doc.tags[index] {
                    return Compiled::none();
                }
                let mut tags = doc.tags.clone();
                tags[index] = t;
                Compiled::ops(vec![Op::SetTags(tags)])
            }
            Command::DeleteTag { tag } => {
                let index = tag_index(doc, tag)?;
                let mut tags = doc.tags.clone();
                tags.remove(index);
                Compiled::ops(vec![Op::SetTags(tags)])
            }
            Command::SetPalette { palette } => {
                if palette.colors.len() > MAX_PALETTE_COLORS {
                    return Err(invalid(format!("palettes are limited to {MAX_PALETTE_COLORS} colors")));
                }
                let palette = Palette { name: validate_name(&palette.name)?, colors: palette.colors.clone() };
                if palette == doc.palette {
                    return Compiled::none();
                }
                Compiled::ops(vec![Op::SetPalette(palette)])
            }
            Command::RenameDocument { name } => {
                let name = validate_name(name)?;
                if name == doc.name {
                    return Compiled::none();
                }
                Compiled::ops(vec![Op::SetName(name)])
            }
            Command::Batch { .. } => Err(CoreError::Internal("batch must be expanded by the editor".into())),
        }
    }
}

fn invalid(msg: String) -> CoreError {
    CoreError::InvalidArgument(msg)
}

fn check_cel(doc: &Document, layer: LayerId, frame: FrameId) -> CoreResult<()> {
    if doc.layer(layer)?.locked {
        return Err(CoreError::LayerLocked(layer));
    }
    doc.frame(frame)?;
    Ok(())
}

/// Turn a list of pixel writes into a single `PutRegion` covering their
/// bounding box. Later writes to the same pixel win.
fn pixel_op(
    doc: &Document,
    layer: LayerId,
    frame: FrameId,
    writes: impl IntoIterator<Item = (i32, i32, crate::color::Rgba)>,
) -> CoreResult<Compiled> {
    check_cel(doc, layer, frame)?;
    let (w, h) = (doc.width as i32, doc.height as i32);
    let writes: Vec<_> = writes.into_iter().filter(|&(x, y, _)| x >= 0 && y >= 0 && x < w && y < h).collect();
    let mut bounds = Bounds::default();
    for &(x, y, _) in &writes {
        bounds.add(x as u32, y as u32);
    }
    let Some(rect) = bounds.rect() else { return Compiled::none() };
    let old = match doc.cel(layer, frame) {
        Some(img) => img.read_region(rect),
        None => vec![0; rect.width as usize * rect.height as usize * 4],
    };
    let mut pixels = old.clone();
    for (x, y, c) in writes {
        let i = ((y as u32 - rect.y) as usize * rect.width as usize + (x as u32 - rect.x) as usize) * 4;
        pixels[i..i + 4].copy_from_slice(&c.to_array());
    }
    if pixels == old {
        return Compiled::none();
    }
    Compiled::ops(vec![Op::PutRegion { layer, frame, rect, pixels }])
}

fn validate_name(name: &str) -> CoreResult<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(invalid("name must not be empty".into()));
    }
    if name.chars().count() > MAX_NAME_LEN {
        return Err(invalid(format!("name is longer than {MAX_NAME_LEN} characters")));
    }
    Ok(name.to_string())
}

fn truncate_name(name: String) -> String {
    name.chars().take(MAX_NAME_LEN).collect()
}

fn validate_duration(ms: u32) -> CoreResult<u32> {
    if ms == 0 || ms > MAX_FRAME_DURATION_MS {
        return Err(invalid(format!("frame duration must be 1..={MAX_FRAME_DURATION_MS} ms")));
    }
    Ok(ms)
}

fn tag_index(doc: &Document, name: &str) -> CoreResult<usize> {
    doc.tags.iter().position(|t| t.name == name).ok_or_else(|| CoreError::TagNotFound(name.to_string()))
}

/// Check a tag's frames and name. `replacing` is the index of the tag being
/// updated, which may keep its own name.
fn validate_tag(doc: &Document, tag: &AnimationTag, replacing: Option<usize>) -> CoreResult<()> {
    let (from, to) = (doc.frame_index(tag.from)?, doc.frame_index(tag.to)?);
    if from > to {
        return Err(invalid(format!("tag {:?} must start at or before its last frame", tag.name)));
    }
    if doc.tags.iter().enumerate().any(|(i, t)| Some(i) != replacing && t.name == tag.name) {
        return Err(invalid(format!("a tag named {:?} already exists", tag.name)));
    }
    Ok(())
}

fn next_layer_name(doc: &Document) -> String {
    (1..).map(|n| format!("Layer {n}")).find(|n| !doc.layers.iter().any(|l| &l.name == n)).expect("unbounded iterator")
}

/// Tags after moving the frame at `from` to `to`, with endpoints swapped
/// where the move would put a tag's last frame before its first. `None`
/// when no tag needs to change.
fn tags_after_move(doc: &Document, from: usize, to: usize) -> Option<Vec<AnimationTag>> {
    let mut order: Vec<FrameId> = doc.frames.iter().map(|f| f.id).collect();
    let id = order.remove(from);
    order.insert(to, id);
    let pos = |f: FrameId| order.iter().position(|x| *x == f);
    let mut changed = false;
    let tags = doc
        .tags
        .iter()
        .map(|t| {
            let mut t = t.clone();
            if pos(t.from) > pos(t.to) {
                std::mem::swap(&mut t.from, &mut t.to);
                changed = true;
            }
            t
        })
        .collect();
    changed.then_some(tags)
}

/// Tags after deleting the frame at `index`: endpoints on that frame move
/// inward; tags that only covered that frame are removed.
fn tags_without_frame(doc: &Document, index: usize) -> Vec<AnimationTag> {
    let removed = doc.frames[index].id;
    let neighbor = |dir: isize| {
        let i = index as isize + dir;
        (i >= 0 && (i as usize) < doc.frames.len()).then(|| doc.frames[i as usize].id)
    };
    doc.tags
        .iter()
        .filter_map(|t| {
            let mut t = t.clone();
            if t.from == removed && t.to == removed {
                return None;
            }
            if t.from == removed {
                t.from = neighbor(1)?;
            }
            if t.to == removed {
                t.to = neighbor(-1)?;
            }
            Some(t)
        })
        .collect()
}
