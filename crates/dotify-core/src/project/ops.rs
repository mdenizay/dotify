//! Reversible project-level operations. Document edits reuse the document
//! [`Op`]s; applying a `ProjectOp` returns the op that undoes it.

use super::{Artboard, ArtboardId, Folder, FolderId, Project};
use crate::error::{CoreError, CoreResult};
use crate::geometry::Rect;
use crate::model::{FrameId, LayerId, Palette};
use crate::ops::Op;

#[derive(Clone, Debug)]
pub(crate) enum ProjectOp {
    Document {
        artboard: ArtboardId,
        op: Op,
    },
    InsertArtboard {
        index: usize,
        artboard: Box<Artboard>,
    },
    RemoveArtboard {
        id: ArtboardId,
    },
    MoveArtboard {
        id: ArtboardId,
        index: usize,
        folder: Option<FolderId>,
    },
    InsertFolder {
        index: usize,
        folder: Folder,
    },
    RemoveFolder {
        id: FolderId,
    },
    SetFolder {
        folder: Folder,
    },
    /// Sets the project palette and every artboard's copy of it.
    SetPalette(Palette),
    SetName(String),
    SetNextId(u32),
}

fn internal(msg: &str) -> CoreError {
    CoreError::Internal(msg.to_string())
}

impl ProjectOp {
    /// Apply to `project` and return the inverse operation.
    pub(crate) fn apply(self, project: &mut Project) -> CoreResult<ProjectOp> {
        Ok(match self {
            ProjectOp::Document { artboard, op } => {
                let op = op.apply(project.document_mut(artboard)?)?;
                ProjectOp::Document { artboard, op }
            }
            ProjectOp::InsertArtboard { index, mut artboard } => {
                if index > project.artboards.len() {
                    return Err(internal("artboard index out of range"));
                }
                let id = artboard.id;
                artboard.document.palette = project.palette.clone();
                project.artboards.insert(index, *artboard);
                ProjectOp::RemoveArtboard { id }
            }
            ProjectOp::RemoveArtboard { id } => {
                let index = project.artboard_index(id)?;
                let artboard = Box::new(project.artboards.remove(index));
                ProjectOp::InsertArtboard { index, artboard }
            }
            ProjectOp::MoveArtboard { id, index, folder } => {
                let from = project.artboard_index(id)?;
                if index >= project.artboards.len() {
                    return Err(internal("artboard index out of range"));
                }
                let mut artboard = project.artboards.remove(from);
                let old_folder = std::mem::replace(&mut artboard.folder, folder);
                project.artboards.insert(index, artboard);
                ProjectOp::MoveArtboard { id, index: from, folder: old_folder }
            }
            ProjectOp::InsertFolder { index, folder } => {
                if index > project.folders.len() {
                    return Err(internal("folder index out of range"));
                }
                let id = folder.id;
                project.folders.insert(index, folder);
                ProjectOp::RemoveFolder { id }
            }
            ProjectOp::RemoveFolder { id } => {
                let index = project.folder_index(id)?;
                if project.artboards.iter().any(|a| a.folder == Some(id)) {
                    return Err(internal("folder is not empty"));
                }
                ProjectOp::InsertFolder { index, folder: project.folders.remove(index) }
            }
            ProjectOp::SetFolder { folder } => {
                let index = project.folder_index(folder.id)?;
                ProjectOp::SetFolder { folder: std::mem::replace(&mut project.folders[index], folder) }
            }
            ProjectOp::SetPalette(p) => {
                for a in &mut project.artboards {
                    a.document.palette = p.clone();
                }
                ProjectOp::SetPalette(std::mem::replace(&mut project.palette, p))
            }
            ProjectOp::SetName(n) => ProjectOp::SetName(std::mem::replace(&mut project.name, n)),
            ProjectOp::SetNextId(n) => ProjectOp::SetNextId(std::mem::replace(&mut project.next_id, n)),
        })
    }

    /// The pixel region this op touches, or `None` for structural ops.
    pub(crate) fn dirty_region(&self) -> Option<(ArtboardId, LayerId, FrameId, Rect)> {
        match self {
            ProjectOp::Document { artboard, op } => op.dirty_region().map(|(l, f, r)| (*artboard, l, f, r)),
            _ => None,
        }
    }

    /// The artboard whose content (pixels or structure) this op changes.
    pub(crate) fn artboard(&self) -> Option<ArtboardId> {
        match self {
            ProjectOp::Document { artboard, .. } => Some(*artboard),
            ProjectOp::InsertArtboard { artboard, .. } => Some(artboard.id),
            ProjectOp::RemoveArtboard { id } => Some(*id),
            _ => None,
        }
    }

    pub(crate) fn is_structural(&self) -> bool {
        match self {
            ProjectOp::Document { op, .. } => op.is_structural(),
            ProjectOp::SetNextId(_) => false,
            _ => true,
        }
    }
}
