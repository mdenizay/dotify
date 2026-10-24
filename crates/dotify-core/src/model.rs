use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::color::Rgba;
use crate::error::{CoreError, CoreResult};
use crate::image::Image;
use crate::MAX_CANVAS_SIZE;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LayerId(pub u32);

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FrameId(pub u32);

impl fmt::Display for LayerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl fmt::Display for FrameId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LayerKind {
    #[default]
    Pixel,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlendMode {
    #[default]
    Normal,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Layer {
    pub id: LayerId,
    pub name: String,
    #[serde(default)]
    pub kind: LayerKind,
    pub visible: bool,
    pub locked: bool,
    /// 0 = invisible, 255 = fully opaque.
    pub opacity: u8,
    #[serde(default)]
    pub blend_mode: BlendMode,
}

impl Layer {
    pub fn new(id: LayerId, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            kind: LayerKind::Pixel,
            visible: true,
            locked: false,
            opacity: 255,
            blend_mode: BlendMode::Normal,
        }
    }
}

pub const DEFAULT_FRAME_DURATION_MS: u32 = 100;

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Frame {
    pub id: FrameId,
    pub duration_ms: u32,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Palette {
    pub name: String,
    pub colors: Vec<Rgba>,
}

impl Default for Palette {
    /// The PICO-8 16 color palette.
    fn default() -> Self {
        const HEX: [u32; 16] = [
            0x000000, 0x1d2b53, 0x7e2553, 0x008751, 0xab5236, 0x5f574f, 0xc2c3c7, 0xfff1e8, 0xff004d, 0xffa300,
            0xffec27, 0x00e436, 0x29adff, 0x83769c, 0xff77a8, 0xffccaa,
        ];
        Palette {
            name: "PICO-8".into(),
            colors: HEX.iter().map(|h| Rgba::opaque((h >> 16) as u8, (h >> 8) as u8, *h as u8)).collect(),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaybackDirection {
    #[default]
    Forward,
    Reverse,
    PingPong,
}

/// A named animation range. References frames by ID so it stays valid when
/// frames are reordered, and is independent from pixel data.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct AnimationTag {
    pub name: String,
    pub from: FrameId,
    pub to: FrameId,
    #[serde(default)]
    pub direction: PlaybackDirection,
    pub color: Rgba,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct ExportSettings {
    pub png_scale: u32,
}

impl Default for ExportSettings {
    fn default() -> Self {
        Self { png_scale: 1 }
    }
}

/// The authoritative editable document.
///
/// Fields are crate-private: all mutation goes through
/// [`Editor`](crate::Editor) commands so it is validated and undoable.
#[derive(Clone, Debug)]
pub struct Document {
    pub(crate) name: String,
    pub(crate) width: u32,
    pub(crate) height: u32,
    /// Bottom to top.
    pub(crate) layers: Vec<Layer>,
    /// Playback order.
    pub(crate) frames: Vec<Frame>,
    /// Missing entries are fully transparent. Every stored image is canvas-sized.
    pub(crate) cels: BTreeMap<(LayerId, FrameId), Image>,
    pub(crate) palette: Palette,
    pub(crate) tags: Vec<AnimationTag>,
    pub(crate) export: ExportSettings,
    pub(crate) next_id: u32,
    pub(crate) extensions: serde_json::Map<String, serde_json::Value>,
}

impl Document {
    /// A new document with one empty layer and one frame.
    pub fn new(name: impl Into<String>, width: u32, height: u32) -> CoreResult<Self> {
        validate_canvas_size(width, height)?;
        Ok(Self {
            name: name.into(),
            width,
            height,
            layers: vec![Layer::new(LayerId(1), "Layer 1")],
            frames: vec![Frame { id: FrameId(2), duration_ms: DEFAULT_FRAME_DURATION_MS }],
            cels: BTreeMap::new(),
            palette: Palette::default(),
            tags: Vec::new(),
            export: ExportSettings::default(),
            next_id: 3,
            extensions: serde_json::Map::new(),
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn layers(&self) -> &[Layer] {
        &self.layers
    }

    pub fn frames(&self) -> &[Frame] {
        &self.frames
    }

    pub fn palette(&self) -> &Palette {
        &self.palette
    }

    pub fn tags(&self) -> &[AnimationTag] {
        &self.tags
    }

    pub fn export_settings(&self) -> &ExportSettings {
        &self.export
    }

    pub fn extensions(&self) -> &serde_json::Map<String, serde_json::Value> {
        &self.extensions
    }

    pub fn layer(&self, id: LayerId) -> CoreResult<&Layer> {
        self.layers.iter().find(|l| l.id == id).ok_or(CoreError::LayerNotFound(id))
    }

    pub fn frame(&self, id: FrameId) -> CoreResult<&Frame> {
        self.frames.iter().find(|f| f.id == id).ok_or(CoreError::FrameNotFound(id))
    }

    pub fn layer_index(&self, id: LayerId) -> CoreResult<usize> {
        self.layers.iter().position(|l| l.id == id).ok_or(CoreError::LayerNotFound(id))
    }

    pub fn frame_index(&self, id: FrameId) -> CoreResult<usize> {
        self.frames.iter().position(|f| f.id == id).ok_or(CoreError::FrameNotFound(id))
    }

    /// The stored cel, if any. `None` means fully transparent.
    pub fn cel(&self, layer: LayerId, frame: FrameId) -> Option<&Image> {
        self.cels.get(&(layer, frame))
    }

    /// The cel pixels, materializing a transparent image when none is stored.
    pub fn cel_image(&self, layer: LayerId, frame: FrameId) -> Image {
        self.cel(layer, frame).cloned().unwrap_or_else(|| Image::new(self.width, self.height))
    }

    /// Iterate over stored, non-transparent cels in deterministic order.
    pub fn cels(&self) -> impl Iterator<Item = (LayerId, FrameId, &Image)> {
        self.cels.iter().filter(|(_, img)| !img.is_transparent()).map(|((l, f), img)| (*l, *f, img))
    }

    pub(crate) fn alloc_id(&self) -> u32 {
        self.next_id
    }
}

/// Documents are equal when their visible content and metadata match;
/// a missing cel and a fully transparent cel are considered identical.
impl PartialEq for Document {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.width == other.width
            && self.height == other.height
            && self.layers == other.layers
            && self.frames == other.frames
            && self.palette == other.palette
            && self.tags == other.tags
            && self.export == other.export
            && self.next_id == other.next_id
            && self.extensions == other.extensions
            && self.cels().eq(other.cels())
    }
}

pub fn validate_canvas_size(width: u32, height: u32) -> CoreResult<()> {
    if width == 0 || height == 0 || width > MAX_CANVAS_SIZE || height > MAX_CANVAS_SIZE {
        return Err(CoreError::InvalidCanvasSize { width, height });
    }
    Ok(())
}

/// Builder used by the format reader to assemble a validated document.
pub(crate) struct DocumentParts {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub layers: Vec<Layer>,
    pub frames: Vec<Frame>,
    pub cels: BTreeMap<(LayerId, FrameId), Image>,
    pub palette: Palette,
    pub tags: Vec<AnimationTag>,
    pub export: ExportSettings,
    pub next_id: u32,
    pub extensions: serde_json::Map<String, serde_json::Value>,
}

impl DocumentParts {
    pub fn build(self) -> CoreResult<Document> {
        validate_canvas_size(self.width, self.height)?;
        let invalid = |m: String| Err(CoreError::InvalidArgument(m));
        if self.layers.is_empty() {
            return Err(CoreError::LastLayer);
        }
        if self.frames.is_empty() {
            return Err(CoreError::LastFrame);
        }
        let mut ids = std::collections::BTreeSet::new();
        for id in self.layers.iter().map(|l| l.id.0).chain(self.frames.iter().map(|f| f.id.0)) {
            if !ids.insert(id) {
                return invalid(format!("duplicate id {id}"));
            }
        }
        let max_id = ids.iter().max().copied().unwrap_or(0);
        let next_id = self.next_id.max(max_id + 1);
        for ((l, f), img) in &self.cels {
            if !self.layers.iter().any(|x| x.id == *l) || !self.frames.iter().any(|x| x.id == *f) {
                return invalid(format!("cel ({l}, {f}) references a missing layer or frame"));
            }
            if img.width() != self.width || img.height() != self.height {
                return invalid(format!("cel ({l}, {f}) has the wrong size"));
            }
        }
        for tag in &self.tags {
            if !self.frames.iter().any(|f| f.id == tag.from) || !self.frames.iter().any(|f| f.id == tag.to) {
                return invalid(format!("tag {:?} references a missing frame", tag.name));
            }
        }
        Ok(Document {
            name: self.name,
            width: self.width,
            height: self.height,
            layers: self.layers,
            frames: self.frames,
            cels: self.cels,
            palette: self.palette,
            tags: self.tags,
            export: self.export,
            next_id,
            extensions: self.extensions,
        })
    }
}
