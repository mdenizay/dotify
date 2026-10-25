//! The project-level editing vocabulary: library management plus `edit`,
//! which runs any document [`Command`] on one artboard.

use serde::{Deserialize, Serialize};

use super::editor::ProjectCreated;
use super::ops::ProjectOp;
use super::{Artboard, ArtboardId, Folder, FolderId, Project};
use crate::command::{invalid, truncate_name, validate_name, Command, MAX_PALETTE_COLORS};
use crate::error::{CoreError, CoreResult};
use crate::model::{Document, Palette};

pub const MAX_ARTBOARDS: usize = 4096;
pub const MAX_FOLDERS: usize = 256;

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProjectCommand {
    CreateArtboard {
        #[serde(default)]
        name: Option<String>,
        width: u32,
        height: u32,
        #[serde(default)]
        folder: Option<FolderId>,
        /// Position in the library; defaults to the end.
        #[serde(default)]
        index: Option<usize>,
    },
    RenameArtboard {
        artboard: ArtboardId,
        name: String,
    },
    /// Deep copy placed right after the source, in the same folder.
    DuplicateArtboard {
        artboard: ArtboardId,
    },
    DeleteArtboard {
        artboard: ArtboardId,
    },
    /// Move to `index` in the library and into `folder` (`None` = no folder).
    MoveArtboard {
        artboard: ArtboardId,
        index: usize,
        #[serde(default)]
        folder: Option<FolderId>,
    },
    CreateFolder {
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        index: Option<usize>,
    },
    RenameFolder {
        folder: FolderId,
        name: String,
    },
    /// Remove a folder; its artboards move out of it, none are deleted.
    DeleteFolder {
        folder: FolderId,
    },
    /// Set the palette shared by every artboard.
    SetPalette {
        palette: Palette,
    },
    RenameProject {
        name: String,
    },
    /// Run a document command on one artboard.
    Edit {
        artboard: ArtboardId,
        command: Command,
    },
    /// Apply all commands atomically as one undo step.
    Batch {
        #[serde(default)]
        label: Option<String>,
        commands: Vec<ProjectCommand>,
    },
}

pub(crate) struct ProjectCompiled {
    pub ops: Vec<ProjectOp>,
    pub created: Option<ProjectCreated>,
}

impl ProjectCompiled {
    fn ops(ops: Vec<ProjectOp>) -> CoreResult<Self> {
        Ok(Self { ops, created: None })
    }

    fn none() -> CoreResult<Self> {
        Self::ops(Vec::new())
    }
}

impl ProjectCommand {
    /// Human-readable label used in undo/redo menus.
    pub fn label(&self) -> String {
        match self {
            ProjectCommand::CreateArtboard { .. } => "New Artboard",
            ProjectCommand::RenameArtboard { .. } => "Rename Artboard",
            ProjectCommand::DuplicateArtboard { .. } => "Duplicate Artboard",
            ProjectCommand::DeleteArtboard { .. } => "Delete Artboard",
            ProjectCommand::MoveArtboard { .. } => "Move Artboard",
            ProjectCommand::CreateFolder { .. } => "New Folder",
            ProjectCommand::RenameFolder { .. } => "Rename Folder",
            ProjectCommand::DeleteFolder { .. } => "Delete Folder",
            ProjectCommand::SetPalette { .. } => "Palette",
            ProjectCommand::RenameProject { .. } => "Rename Project",
            ProjectCommand::Edit { command, .. } => return command.label(),
            ProjectCommand::Batch { label, .. } => return label.clone().unwrap_or_else(|| "Batch".into()),
        }
        .to_string()
    }

    /// Compile a library command (not `edit` or `batch`, which the editor
    /// expands) against the current project.
    pub(crate) fn compile(&self, project: &Project) -> CoreResult<ProjectCompiled> {
        match self {
            ProjectCommand::CreateArtboard { name, width, height, folder, index } => {
                if project.artboards.len() >= MAX_ARTBOARDS {
                    return Err(invalid(format!("projects are limited to {MAX_ARTBOARDS} artboards")));
                }
                if let Some(folder) = folder {
                    project.folder(*folder)?;
                }
                let index = index.unwrap_or(project.artboards.len());
                if index > project.artboards.len() {
                    return Err(invalid(format!("artboard index {index} out of range")));
                }
                let name = match name {
                    Some(n) => validate_name(n)?,
                    None => next_name(project.artboards.iter().map(|a| a.name()), "Sprite"),
                };
                let mut document = Document::new(name, *width, *height)?;
                document.palette = project.palette.clone();
                let id = ArtboardId(project.next_id);
                Ok(ProjectCompiled {
                    ops: vec![
                        ProjectOp::SetNextId(id.0 + 1),
                        ProjectOp::InsertArtboard {
                            index,
                            artboard: Box::new(Artboard { id, folder: *folder, document }),
                        },
                    ],
                    created: Some(ProjectCreated::Artboard(id)),
                })
            }
            ProjectCommand::RenameArtboard { artboard, name } => {
                let current = project.artboard(*artboard)?;
                let name = validate_name(name)?;
                if name == current.name() {
                    return ProjectCompiled::none();
                }
                ProjectCompiled::ops(vec![ProjectOp::Document {
                    artboard: *artboard,
                    op: crate::ops::Op::SetName(name),
                }])
            }
            ProjectCommand::DuplicateArtboard { artboard } => {
                if project.artboards.len() >= MAX_ARTBOARDS {
                    return Err(invalid(format!("projects are limited to {MAX_ARTBOARDS} artboards")));
                }
                let index = project.artboard_index(*artboard)?;
                let src = &project.artboards[index];
                let mut copy = src.clone();
                copy.id = ArtboardId(project.next_id);
                copy.document.name = truncate_name(format!("{} copy", src.name()));
                let id = copy.id;
                Ok(ProjectCompiled {
                    ops: vec![
                        ProjectOp::SetNextId(id.0 + 1),
                        ProjectOp::InsertArtboard { index: index + 1, artboard: Box::new(copy) },
                    ],
                    created: Some(ProjectCreated::Artboard(id)),
                })
            }
            ProjectCommand::DeleteArtboard { artboard } => {
                project.artboard(*artboard)?;
                if project.artboards.len() == 1 {
                    return Err(CoreError::LastArtboard);
                }
                ProjectCompiled::ops(vec![ProjectOp::RemoveArtboard { id: *artboard }])
            }
            ProjectCommand::MoveArtboard { artboard, index, folder } => {
                let a = project.artboard(*artboard)?;
                if let Some(folder) = folder {
                    project.folder(*folder)?;
                }
                if *index >= project.artboards.len() {
                    return Err(invalid(format!("artboard index {index} out of range")));
                }
                if project.artboard_index(*artboard)? == *index && a.folder == *folder {
                    return ProjectCompiled::none();
                }
                ProjectCompiled::ops(vec![ProjectOp::MoveArtboard { id: *artboard, index: *index, folder: *folder }])
            }
            ProjectCommand::CreateFolder { name, index } => {
                if project.folders.len() >= MAX_FOLDERS {
                    return Err(invalid(format!("projects are limited to {MAX_FOLDERS} folders")));
                }
                let index = index.unwrap_or(project.folders.len());
                if index > project.folders.len() {
                    return Err(invalid(format!("folder index {index} out of range")));
                }
                let name = match name {
                    Some(n) => validate_name(n)?,
                    None => next_name(project.folders.iter().map(|f| f.name.as_str()), "Folder"),
                };
                let id = FolderId(project.next_id);
                Ok(ProjectCompiled {
                    ops: vec![
                        ProjectOp::SetNextId(id.0 + 1),
                        ProjectOp::InsertFolder { index, folder: Folder { id, name } },
                    ],
                    created: Some(ProjectCreated::Folder(id)),
                })
            }
            ProjectCommand::RenameFolder { folder, name } => {
                let current = project.folder(*folder)?;
                let name = validate_name(name)?;
                if name == current.name {
                    return ProjectCompiled::none();
                }
                ProjectCompiled::ops(vec![ProjectOp::SetFolder { folder: Folder { id: *folder, name } }])
            }
            ProjectCommand::DeleteFolder { folder } => {
                project.folder(*folder)?;
                let mut ops: Vec<_> = project
                    .artboards
                    .iter()
                    .enumerate()
                    .filter(|(_, a)| a.folder == Some(*folder))
                    .map(|(index, a)| ProjectOp::MoveArtboard { id: a.id, index, folder: None })
                    .collect();
                ops.push(ProjectOp::RemoveFolder { id: *folder });
                ProjectCompiled::ops(ops)
            }
            ProjectCommand::SetPalette { palette } => {
                if palette.colors.len() > MAX_PALETTE_COLORS {
                    return Err(invalid(format!("palettes are limited to {MAX_PALETTE_COLORS} colors")));
                }
                let palette = Palette { name: validate_name(&palette.name)?, colors: palette.colors.clone() };
                if palette == project.palette {
                    return ProjectCompiled::none();
                }
                ProjectCompiled::ops(vec![ProjectOp::SetPalette(palette)])
            }
            ProjectCommand::RenameProject { name } => {
                let name = validate_name(name)?;
                if name == project.name {
                    return ProjectCompiled::none();
                }
                ProjectCompiled::ops(vec![ProjectOp::SetName(name)])
            }
            ProjectCommand::Edit { .. } | ProjectCommand::Batch { .. } => {
                Err(CoreError::Internal("edit and batch must be expanded by the editor".into()))
            }
        }
    }
}

/// First "`prefix` N" not already taken.
fn next_name<'a>(taken: impl Iterator<Item = &'a str> + Clone, prefix: &str) -> String {
    (1..).map(|n| format!("{prefix} {n}")).find(|n| !taken.clone().any(|t| t == n)).expect("unbounded iterator")
}
