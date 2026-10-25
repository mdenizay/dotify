//! Projects: a library of artboards (each a [`Document`]) sharing one
//! palette, optionally grouped into folders. See `docs/LIBRARY.md`.

mod command;
mod editor;
mod ops;

use std::collections::BTreeSet;
use std::fmt;

use serde::{Deserialize, Serialize};

pub use command::{ProjectCommand, MAX_ARTBOARDS, MAX_FOLDERS};
pub use editor::{ArtboardSummary, ProjectCreated, ProjectDirtyRegion, ProjectEditor, ProjectOutcome, ProjectSummary};

use crate::error::{CoreError, CoreResult};
use crate::model::{Document, Palette};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ArtboardId(pub u32);

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FolderId(pub u32);

impl fmt::Display for ArtboardId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl fmt::Display for FolderId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// A named group of artboards in the library ("Characters", "Tiles", …).
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Folder {
    pub id: FolderId,
    pub name: String,
}

/// One sprite in the project: a full document plus its place in the library.
/// The artboard's name is its document's name.
#[derive(Clone, PartialEq, Debug)]
pub struct Artboard {
    pub(crate) id: ArtboardId,
    pub(crate) folder: Option<FolderId>,
    pub(crate) document: Document,
}

impl Artboard {
    pub fn id(&self) -> ArtboardId {
        self.id
    }

    /// `None` = not in any folder.
    pub fn folder(&self) -> Option<FolderId> {
        self.folder
    }

    pub fn name(&self) -> &str {
        self.document.name()
    }

    pub fn document(&self) -> &Document {
        &self.document
    }
}

/// The authoritative editable project. All mutation goes through
/// [`ProjectEditor`] so it is validated and undoable.
///
/// Invariants: at least one artboard; every artboard's document palette
/// equals the project palette; artboard and folder IDs are unique.
#[derive(Clone, PartialEq, Debug)]
pub struct Project {
    pub(crate) name: String,
    pub(crate) palette: Palette,
    pub(crate) folders: Vec<Folder>,
    /// Library order.
    pub(crate) artboards: Vec<Artboard>,
    /// Allocator for artboard and folder IDs.
    pub(crate) next_id: u32,
    pub(crate) extensions: serde_json::Map<String, serde_json::Value>,
}

impl Project {
    /// A new project with one empty artboard of the given size.
    pub fn new(name: impl Into<String>, width: u32, height: u32) -> CoreResult<Self> {
        let name = name.into();
        Ok(Self::from_document(Document::new(name, width, height)?))
    }

    /// Wrap a single document as a project with one artboard (ID 1). The
    /// document's palette becomes the project palette.
    pub fn from_document(document: Document) -> Self {
        Self {
            name: document.name.clone(),
            palette: document.palette.clone(),
            folders: Vec::new(),
            artboards: vec![Artboard { id: ArtboardId(1), folder: None, document }],
            next_id: 2,
            extensions: serde_json::Map::new(),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn palette(&self) -> &Palette {
        &self.palette
    }

    pub fn folders(&self) -> &[Folder] {
        &self.folders
    }

    pub fn artboards(&self) -> &[Artboard] {
        &self.artboards
    }

    pub fn extensions(&self) -> &serde_json::Map<String, serde_json::Value> {
        &self.extensions
    }

    pub fn artboard(&self, id: ArtboardId) -> CoreResult<&Artboard> {
        self.artboards.iter().find(|a| a.id == id).ok_or(CoreError::ArtboardNotFound(id))
    }

    pub fn artboard_index(&self, id: ArtboardId) -> CoreResult<usize> {
        self.artboards.iter().position(|a| a.id == id).ok_or(CoreError::ArtboardNotFound(id))
    }

    pub fn folder(&self, id: FolderId) -> CoreResult<&Folder> {
        self.folders.iter().find(|f| f.id == id).ok_or(CoreError::FolderNotFound(id))
    }

    pub fn folder_index(&self, id: FolderId) -> CoreResult<usize> {
        self.folders.iter().position(|f| f.id == id).ok_or(CoreError::FolderNotFound(id))
    }

    /// Convenience for callers that address an artboard's document.
    pub fn document(&self, id: ArtboardId) -> CoreResult<&Document> {
        Ok(&self.artboard(id)?.document)
    }

    pub(crate) fn document_mut(&mut self, id: ArtboardId) -> CoreResult<&mut Document> {
        self.artboards.iter_mut().find(|a| a.id == id).map(|a| &mut a.document).ok_or(CoreError::ArtboardNotFound(id))
    }
}

/// Builder used by the format reader to assemble a validated project.
pub(crate) struct ProjectParts {
    pub name: String,
    pub palette: Palette,
    pub folders: Vec<Folder>,
    pub artboards: Vec<(ArtboardId, Option<FolderId>, Document)>,
    pub next_id: u32,
    pub extensions: serde_json::Map<String, serde_json::Value>,
}

impl ProjectParts {
    pub fn build(self) -> CoreResult<Project> {
        let invalid = |m: String| Err(CoreError::InvalidArgument(m));
        if self.artboards.is_empty() {
            return Err(CoreError::LastArtboard);
        }
        let mut ids = BTreeSet::new();
        for id in self.folders.iter().map(|f| f.id.0).chain(self.artboards.iter().map(|a| a.0 .0)) {
            if !ids.insert(id) {
                return invalid(format!("duplicate artboard/folder id {id}"));
            }
        }
        for (id, folder, _) in &self.artboards {
            if let Some(folder) = folder {
                if !self.folders.iter().any(|f| f.id == *folder) {
                    return invalid(format!("artboard {id} references missing folder {folder}"));
                }
            }
        }
        let max_id = ids.iter().max().copied().unwrap_or(0);
        let palette = self.palette;
        let artboards = self
            .artboards
            .into_iter()
            .map(|(id, folder, mut document)| {
                document.palette = palette.clone();
                Artboard { id, folder, document }
            })
            .collect();
        Ok(Project {
            name: self.name,
            palette,
            folders: self.folders,
            artboards,
            next_id: self.next_id.max(max_id + 1),
            extensions: self.extensions,
        })
    }
}
