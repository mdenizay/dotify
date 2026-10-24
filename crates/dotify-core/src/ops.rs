//! Low-level reversible operations. Commands compile into these; applying an
//! op returns the op that undoes it.

use crate::error::{CoreError, CoreResult};
use crate::geometry::Rect;
use crate::image::Image;
use crate::model::{AnimationTag, Document, Frame, FrameId, Layer, LayerId, Palette};

#[derive(Clone, Debug)]
pub(crate) enum Op {
    PutRegion { layer: LayerId, frame: FrameId, rect: Rect, pixels: Vec<u8> },
    InsertLayer { index: usize, layer: Layer, cels: Vec<(FrameId, Image)> },
    RemoveLayer { id: LayerId },
    SetLayer { layer: Layer },
    MoveLayer { id: LayerId, index: usize },
    InsertFrame { index: usize, frame: Frame, cels: Vec<(LayerId, Image)> },
    RemoveFrame { id: FrameId },
    SetFrame { frame: Frame },
    MoveFrame { id: FrameId, index: usize },
    SetPalette(Palette),
    SetName(String),
    SetTags(Vec<AnimationTag>),
    SetNextId(u32),
}

fn internal(msg: &str) -> CoreError {
    CoreError::Internal(msg.to_string())
}

impl Op {
    /// Apply to `doc` and return the inverse operation.
    pub(crate) fn apply(self, doc: &mut Document) -> CoreResult<Op> {
        Ok(match self {
            Op::PutRegion { layer, frame, rect, pixels } => {
                let (w, h) = (doc.width, doc.height);
                if rect.right() > w || rect.bottom() > h {
                    return Err(internal("region outside canvas"));
                }
                let cel = doc.cels.entry((layer, frame)).or_insert_with(|| Image::new(w, h));
                let old = cel.read_region(rect);
                cel.write_region(rect, &pixels);
                if cel.is_transparent() {
                    doc.cels.remove(&(layer, frame));
                }
                Op::PutRegion { layer, frame, rect, pixels: old }
            }
            Op::InsertLayer { index, layer, cels } => {
                if index > doc.layers.len() {
                    return Err(internal("layer index out of range"));
                }
                let id = layer.id;
                doc.layers.insert(index, layer);
                for (frame, img) in cels {
                    doc.cels.insert((id, frame), img);
                }
                Op::RemoveLayer { id }
            }
            Op::RemoveLayer { id } => {
                let index = doc.layer_index(id)?;
                let layer = doc.layers.remove(index);
                let keys: Vec<_> = doc.cels.keys().filter(|(l, _)| *l == id).copied().collect();
                let cels = keys.into_iter().filter_map(|k| doc.cels.remove(&k).map(|img| (k.1, img))).collect();
                Op::InsertLayer { index, layer, cels }
            }
            Op::SetLayer { layer } => {
                let index = doc.layer_index(layer.id)?;
                Op::SetLayer { layer: std::mem::replace(&mut doc.layers[index], layer) }
            }
            Op::MoveLayer { id, index } => {
                let from = doc.layer_index(id)?;
                if index >= doc.layers.len() {
                    return Err(internal("layer index out of range"));
                }
                let layer = doc.layers.remove(from);
                doc.layers.insert(index, layer);
                Op::MoveLayer { id, index: from }
            }
            Op::InsertFrame { index, frame, cels } => {
                if index > doc.frames.len() {
                    return Err(internal("frame index out of range"));
                }
                let id = frame.id;
                doc.frames.insert(index, frame);
                for (layer, img) in cels {
                    doc.cels.insert((layer, id), img);
                }
                Op::RemoveFrame { id }
            }
            Op::RemoveFrame { id } => {
                let index = doc.frame_index(id)?;
                let frame = doc.frames.remove(index);
                let keys: Vec<_> = doc.cels.keys().filter(|(_, f)| *f == id).copied().collect();
                let cels = keys.into_iter().filter_map(|k| doc.cels.remove(&k).map(|img| (k.0, img))).collect();
                Op::InsertFrame { index, frame, cels }
            }
            Op::SetFrame { frame } => {
                let index = doc.frame_index(frame.id)?;
                Op::SetFrame { frame: std::mem::replace(&mut doc.frames[index], frame) }
            }
            Op::MoveFrame { id, index } => {
                let from = doc.frame_index(id)?;
                if index >= doc.frames.len() {
                    return Err(internal("frame index out of range"));
                }
                let frame = doc.frames.remove(from);
                doc.frames.insert(index, frame);
                Op::MoveFrame { id, index: from }
            }
            Op::SetPalette(p) => Op::SetPalette(std::mem::replace(&mut doc.palette, p)),
            Op::SetName(n) => Op::SetName(std::mem::replace(&mut doc.name, n)),
            Op::SetTags(t) => Op::SetTags(std::mem::replace(&mut doc.tags, t)),
            Op::SetNextId(n) => Op::SetNextId(std::mem::replace(&mut doc.next_id, n)),
        })
    }

    /// The pixel region this op touches, or `None` for structural ops.
    pub(crate) fn dirty_region(&self) -> Option<(LayerId, FrameId, Rect)> {
        match self {
            Op::PutRegion { layer, frame, rect, .. } => Some((*layer, *frame, *rect)),
            _ => None,
        }
    }

    pub(crate) fn is_structural(&self) -> bool {
        !matches!(self, Op::PutRegion { .. } | Op::SetNextId(_))
    }
}
