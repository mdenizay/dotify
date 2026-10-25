use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use super::command::ProjectCommand;
use super::ops::ProjectOp;
use super::{ArtboardId, Folder, FolderId, Project};
use crate::command::{Command, Created};
use crate::editor::{DocumentSummary, DEFAULT_HISTORY_LIMIT};
use crate::error::{CoreError, CoreResult};
use crate::geometry::Rect;
use crate::model::{AnimationTag, Frame, FrameId, Layer, LayerId, Palette};

/// An object created by a project command, reported back to the caller.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectCreated {
    Artboard(ArtboardId),
    Folder(FolderId),
    Layer(LayerId),
    Frame(FrameId),
}

impl From<Created> for ProjectCreated {
    fn from(c: Created) -> Self {
        match c {
            Created::Layer(id) => ProjectCreated::Layer(id),
            Created::Frame(id) => ProjectCreated::Frame(id),
        }
    }
}

/// A pixel region that changed on one cel of one artboard.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize)]
pub struct ProjectDirtyRegion {
    pub artboard: ArtboardId,
    pub layer: LayerId,
    pub frame: FrameId,
    pub rect: Rect,
}

/// Result of executing, undoing or redoing a project command.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize)]
pub struct ProjectOutcome {
    pub revision: u64,
    /// False when the command was valid but changed nothing.
    pub changed: bool,
    /// Library, palette, layers, frames, tags or names changed.
    pub structure_changed: bool,
    pub dirty: Vec<ProjectDirtyRegion>,
    /// Artboards whose content changed (for refreshing thumbnails).
    pub artboards: Vec<ArtboardId>,
    pub created: Vec<ProjectCreated>,
    pub label: Option<String>,
}

#[derive(Debug)]
struct Transaction {
    id: u64,
    label: String,
    /// Ops to apply, in reverse order, to step across this transaction.
    ops: Vec<ProjectOp>,
}

#[derive(Default)]
struct Effects {
    dirty: BTreeMap<(ArtboardId, LayerId, FrameId), Rect>,
    artboards: BTreeSet<ArtboardId>,
    structure_changed: bool,
}

impl Effects {
    fn record(&mut self, op: &ProjectOp) {
        if let Some((a, l, f, r)) = op.dirty_region() {
            self.dirty.entry((a, l, f)).and_modify(|e| *e = e.union(&r)).or_insert(r);
        }
        self.artboards.extend(op.artboard());
        self.structure_changed |= op.is_structural();
    }

    fn into_outcome(self, revision: u64, created: Vec<ProjectCreated>, label: Option<String>) -> ProjectOutcome {
        ProjectOutcome {
            revision,
            changed: true,
            structure_changed: self.structure_changed,
            dirty: self
                .dirty
                .into_iter()
                .map(|((artboard, layer, frame), rect)| ProjectDirtyRegion { artboard, layer, frame, rect })
                .collect(),
            artboards: self.artboards.into_iter().collect(),
            created,
            label,
        }
    }
}

/// Library entry of one artboard, without pixel data.
#[derive(Clone, PartialEq, Debug, Serialize)]
pub struct ArtboardSummary {
    pub id: ArtboardId,
    pub name: String,
    pub folder: Option<FolderId>,
    pub width: u32,
    pub height: u32,
    pub layers: Vec<Layer>,
    pub frames: Vec<Frame>,
    pub tags: Vec<AnimationTag>,
}

/// Snapshot of a project's structure and editor state. No pixel data.
#[derive(Clone, PartialEq, Debug, Serialize)]
pub struct ProjectSummary {
    pub name: String,
    pub palette: Palette,
    pub folders: Vec<Folder>,
    pub artboards: Vec<ArtboardSummary>,
    pub revision: u64,
    pub dirty: bool,
    pub can_undo: bool,
    pub can_redo: bool,
    pub undo_label: Option<String>,
    pub redo_label: Option<String>,
}

/// A project plus one undo/redo history covering every artboard. This is
/// the single entry point through which clients edit a project.
pub struct ProjectEditor {
    project: Project,
    undo: Vec<Transaction>,
    redo: Vec<Transaction>,
    revision: u64,
    next_tx: u64,
    /// ID of the transaction on top of the undo stack when last saved
    /// (`None` = the empty stack).
    saved_at: Option<Option<u64>>,
    history_limit: usize,
}

impl ProjectEditor {
    pub fn new(project: Project) -> Self {
        Self {
            project,
            undo: Vec::new(),
            redo: Vec::new(),
            revision: 0,
            next_tx: 1,
            saved_at: Some(None),
            history_limit: DEFAULT_HISTORY_LIMIT,
        }
    }

    pub fn with_history_limit(mut self, limit: usize) -> Self {
        self.history_limit = limit.max(1);
        self
    }

    pub fn project(&self) -> &Project {
        &self.project
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo_label(&self) -> Option<&str> {
        self.undo.last().map(|t| t.label.as_str())
    }

    pub fn redo_label(&self) -> Option<&str> {
        self.redo.last().map(|t| t.label.as_str())
    }

    /// True when the project differs from the last saved state.
    pub fn is_dirty(&self) -> bool {
        self.saved_at != Some(self.undo.last().map(|t| t.id))
    }

    pub fn mark_saved(&mut self) {
        self.saved_at = Some(self.undo.last().map(|t| t.id));
    }

    /// Mark the project as having unsaved changes (e.g. restored from a
    /// recovery file).
    pub fn mark_unsaved(&mut self) {
        self.saved_at = None;
    }

    pub fn summary(&self) -> ProjectSummary {
        let p = &self.project;
        ProjectSummary {
            name: p.name.clone(),
            palette: p.palette.clone(),
            folders: p.folders.clone(),
            artboards: p
                .artboards
                .iter()
                .map(|a| ArtboardSummary {
                    id: a.id,
                    name: a.document.name.clone(),
                    folder: a.folder,
                    width: a.document.width,
                    height: a.document.height,
                    layers: a.document.layers.clone(),
                    frames: a.document.frames.clone(),
                    tags: a.document.tags.clone(),
                })
                .collect(),
            revision: self.revision,
            dirty: self.is_dirty(),
            can_undo: self.can_undo(),
            can_redo: self.can_redo(),
            undo_label: self.undo_label().map(str::to_string),
            redo_label: self.redo_label().map(str::to_string),
        }
    }

    /// Summary of one artboard in the shape of a single-document editor,
    /// with the project's history state.
    pub fn document_summary(&self, artboard: ArtboardId) -> CoreResult<DocumentSummary> {
        let doc = self.project.document(artboard)?;
        Ok(DocumentSummary {
            name: doc.name.clone(),
            width: doc.width,
            height: doc.height,
            layers: doc.layers.clone(),
            frames: doc.frames.clone(),
            palette: doc.palette.clone(),
            tags: doc.tags.clone(),
            revision: self.revision,
            dirty: self.is_dirty(),
            can_undo: self.can_undo(),
            can_redo: self.can_redo(),
            undo_label: self.undo_label().map(str::to_string),
            redo_label: self.redo_label().map(str::to_string),
        })
    }

    pub fn execute(&mut self, cmd: ProjectCommand) -> CoreResult<ProjectOutcome> {
        self.execute_expecting(cmd, None)
    }

    /// Execute `cmd` as one atomic, undoable transaction. If
    /// `expected_revision` is given and does not match, nothing is applied.
    /// On any error the project is left exactly as it was.
    pub fn execute_expecting(
        &mut self,
        cmd: ProjectCommand,
        expected_revision: Option<u64>,
    ) -> CoreResult<ProjectOutcome> {
        if let Some(expected) = expected_revision {
            if expected != self.revision {
                return Err(CoreError::RevisionMismatch { expected, actual: self.revision });
            }
        }
        let label = cmd.label();
        let mut tx = Pending::default();
        if let Err(e) = self.run(&cmd, &mut tx) {
            self.rollback(tx.applied);
            return Err(e);
        }
        if tx.applied.is_empty() {
            return Ok(ProjectOutcome { revision: self.revision, created: tx.created, ..Default::default() });
        }
        self.revision += 1;
        let id = self.next_tx;
        self.next_tx += 1;
        self.undo.push(Transaction { id, label: label.clone(), ops: tx.applied });
        self.redo.clear();
        if self.undo.len() > self.history_limit {
            self.undo.remove(0);
        }
        Ok(tx.effects.into_outcome(self.revision, tx.created, Some(label)))
    }

    fn run(&mut self, cmd: &ProjectCommand, tx: &mut Pending) -> CoreResult<()> {
        match cmd {
            ProjectCommand::Batch { commands, .. } => commands.iter().try_for_each(|c| self.run(c, tx)),
            ProjectCommand::Edit { artboard, command } => self.run_edit(*artboard, command, tx),
            _ => {
                let compiled = cmd.compile(&self.project)?;
                self.apply_all(compiled.ops, tx)?;
                tx.created.extend(compiled.created);
                Ok(())
            }
        }
    }

    fn run_edit(&mut self, artboard: ArtboardId, cmd: &Command, tx: &mut Pending) -> CoreResult<()> {
        match cmd {
            Command::Batch { commands, .. } => commands.iter().try_for_each(|c| self.run_edit(artboard, c, tx)),
            // The palette is shared: route it to the project.
            Command::SetPalette { palette } => {
                self.project.artboard(artboard)?;
                self.run(&ProjectCommand::SetPalette { palette: palette.clone() }, tx)
            }
            _ => {
                let compiled = cmd.compile(self.project.document(artboard)?)?;
                let ops = compiled.ops.into_iter().map(|op| ProjectOp::Document { artboard, op }).collect();
                self.apply_all(ops, tx)?;
                tx.created.extend(compiled.created.map(ProjectCreated::from));
                Ok(())
            }
        }
    }

    fn apply_all(&mut self, ops: Vec<ProjectOp>, tx: &mut Pending) -> CoreResult<()> {
        for op in ops {
            tx.effects.record(&op);
            tx.applied.push(op.apply(&mut self.project)?);
        }
        Ok(())
    }

    fn rollback(&mut self, applied: Vec<ProjectOp>) {
        for op in applied.into_iter().rev() {
            // Inverses of ops that just succeeded cannot fail.
            let _ = op.apply(&mut self.project);
        }
    }

    fn step(&mut self, tx: Transaction) -> (Transaction, ProjectOutcome) {
        let mut effects = Effects::default();
        let mut reversed = Vec::with_capacity(tx.ops.len());
        for op in tx.ops.into_iter().rev() {
            effects.record(&op);
            match op.apply(&mut self.project) {
                Ok(inv) => reversed.push(inv),
                Err(e) => panic!("history is inconsistent with the project: {e}"),
            }
        }
        self.revision += 1;
        let outcome = effects.into_outcome(self.revision, Vec::new(), Some(tx.label.clone()));
        (Transaction { id: tx.id, label: tx.label, ops: reversed }, outcome)
    }

    pub fn undo(&mut self) -> Option<ProjectOutcome> {
        let tx = self.undo.pop()?;
        let (tx, outcome) = self.step(tx);
        self.redo.push(tx);
        Some(outcome)
    }

    pub fn redo(&mut self) -> Option<ProjectOutcome> {
        let tx = self.redo.pop()?;
        let (tx, outcome) = self.step(tx);
        self.undo.push(tx);
        Some(outcome)
    }
}

/// A transaction being built by `execute`.
#[derive(Default)]
struct Pending {
    applied: Vec<ProjectOp>,
    effects: Effects,
    created: Vec<ProjectCreated>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Rgba;
    use crate::command::PixelWrite;
    use crate::model::Document;

    const RED: Rgba = Rgba::opaque(255, 0, 0);

    fn editor() -> (ProjectEditor, ArtboardId) {
        let project = Project::new("Game", 8, 8).unwrap();
        let id = project.artboards()[0].id();
        (ProjectEditor::new(project), id)
    }

    fn created_artboard(o: &ProjectOutcome) -> ArtboardId {
        match o.created[0] {
            ProjectCreated::Artboard(id) => id,
            other => panic!("expected an artboard, got {other:?}"),
        }
    }

    fn created_folder(o: &ProjectOutcome) -> FolderId {
        match o.created[0] {
            ProjectCreated::Folder(id) => id,
            other => panic!("expected a folder, got {other:?}"),
        }
    }

    fn draw(artboard: ArtboardId, doc: &Document, x: i32) -> ProjectCommand {
        ProjectCommand::Edit {
            artboard,
            command: Command::SetPixels {
                layer: doc.layers()[0].id,
                frame: doc.frames()[0].id,
                pixels: vec![PixelWrite { x, y: 0, color: RED }],
            },
        }
    }

    fn order(e: &ProjectEditor) -> Vec<ArtboardId> {
        e.project().artboards().iter().map(|a| a.id()).collect()
    }

    /// Execute, then check undo restores `before` and redo restores the result.
    fn assert_undoable(e: &mut ProjectEditor, cmd: ProjectCommand) -> ProjectOutcome {
        let before = e.project().clone();
        let out = e.execute(cmd).unwrap();
        assert!(out.changed);
        let after = e.project().clone();
        e.undo().unwrap();
        assert_eq!(e.project(), &before);
        e.redo().unwrap();
        assert_eq!(e.project(), &after);
        out
    }

    #[test]
    fn create_artboard_uses_project_palette_and_fresh_ids() {
        let (mut e, a1) = editor();
        let out = assert_undoable(
            &mut e,
            ProjectCommand::CreateArtboard { name: None, width: 16, height: 24, folder: None, index: None },
        );
        let a2 = created_artboard(&out);
        assert_ne!(a1, a2);
        assert_eq!(out.artboards, vec![a2]);
        let art = e.project().artboard(a2).unwrap();
        assert_eq!((art.name(), art.document().width(), art.document().height()), ("Sprite 1", 16, 24));
        assert_eq!(art.document().palette(), e.project().palette());
        assert!(matches!(
            e.execute(ProjectCommand::CreateArtboard { name: None, width: 0, height: 1, folder: None, index: None }),
            Err(CoreError::InvalidCanvasSize { .. })
        ));
        assert!(matches!(
            e.execute(ProjectCommand::CreateArtboard {
                name: None,
                width: 1,
                height: 1,
                folder: Some(FolderId(99)),
                index: None
            }),
            Err(CoreError::FolderNotFound(FolderId(99)))
        ));
    }

    #[test]
    fn rename_artboard_is_the_document_name() {
        let (mut e, a) = editor();
        assert_undoable(&mut e, ProjectCommand::RenameArtboard { artboard: a, name: " Hero ".into() });
        assert_eq!(e.project().artboard(a).unwrap().name(), "Hero");
        let out = e.execute(ProjectCommand::RenameArtboard { artboard: a, name: "Hero".into() }).unwrap();
        assert!(!out.changed);
        e.execute(ProjectCommand::Edit { artboard: a, command: Command::RenameDocument { name: "Knight".into() } })
            .unwrap();
        assert_eq!(e.summary().artboards[0].name, "Knight");
        assert!(e.execute(ProjectCommand::RenameArtboard { artboard: a, name: "".into() }).is_err());
    }

    #[test]
    fn duplicate_artboard_deep_copies_after_source() {
        let (mut e, a) = editor();
        let doc = e.project().document(a).unwrap().clone();
        e.execute(draw(a, &doc, 1)).unwrap();
        let folder = created_folder(&e.execute(ProjectCommand::CreateFolder { name: None, index: None }).unwrap());
        e.execute(ProjectCommand::MoveArtboard { artboard: a, index: 0, folder: Some(folder) }).unwrap();
        let out = assert_undoable(&mut e, ProjectCommand::DuplicateArtboard { artboard: a });
        let copy = created_artboard(&out);
        assert_eq!(order(&e), vec![a, copy]);
        let c = e.project().artboard(copy).unwrap();
        assert_eq!((c.name(), c.folder()), ("Game copy", Some(folder)));
        // Same layer/frame IDs, independent pixels.
        assert_eq!(c.document().layers(), doc.layers());
        e.execute(draw(copy, &doc, 2)).unwrap();
        let px = |id| e.project().document(id).unwrap().cel_image(doc.layers()[0].id, doc.frames()[0].id).get(2, 0);
        assert_eq!((px(a), px(copy)), (Rgba::TRANSPARENT, RED));
    }

    #[test]
    fn delete_artboard_keeps_the_last_one_and_undo_restores_pixels() {
        let (mut e, a1) = editor();
        let a2 = created_artboard(
            &e.execute(ProjectCommand::CreateArtboard {
                name: None,
                width: 4,
                height: 4,
                folder: None,
                index: Some(0),
            })
            .unwrap(),
        );
        assert_eq!(order(&e), vec![a2, a1]);
        let doc = e.project().document(a1).unwrap().clone();
        e.execute(draw(a1, &doc, 3)).unwrap();
        assert_undoable(&mut e, ProjectCommand::DeleteArtboard { artboard: a1 });
        assert_eq!(e.execute(ProjectCommand::DeleteArtboard { artboard: a2 }).unwrap_err(), CoreError::LastArtboard);
        assert_eq!(
            e.execute(ProjectCommand::DeleteArtboard { artboard: a1 }).unwrap_err().code(),
            "artboard_not_found"
        );
        e.undo().unwrap();
        assert_eq!(order(&e), vec![a2, a1]);
        assert_eq!(e.project().document(a1).unwrap().cel_image(doc.layers()[0].id, doc.frames()[0].id).get(3, 0), RED);
    }

    #[test]
    fn move_artboard_reorders_and_changes_folder() {
        let (mut e, a1) = editor();
        let new = |e: &mut ProjectEditor| {
            created_artboard(
                &e.execute(ProjectCommand::CreateArtboard {
                    name: None,
                    width: 4,
                    height: 4,
                    folder: None,
                    index: None,
                })
                .unwrap(),
            )
        };
        let (a2, a3) = (new(&mut e), new(&mut e));
        let tiles = created_folder(
            &e.execute(ProjectCommand::CreateFolder { name: Some("Tiles".into()), index: None }).unwrap(),
        );
        assert_undoable(&mut e, ProjectCommand::MoveArtboard { artboard: a3, index: 0, folder: Some(tiles) });
        assert_eq!(order(&e), vec![a3, a1, a2]);
        assert_eq!(e.project().artboard(a3).unwrap().folder(), Some(tiles));
        let out = e.execute(ProjectCommand::MoveArtboard { artboard: a3, index: 0, folder: Some(tiles) }).unwrap();
        assert!(!out.changed);
        assert!(e.execute(ProjectCommand::MoveArtboard { artboard: a3, index: 3, folder: None }).is_err());
        assert!(e
            .execute(ProjectCommand::MoveArtboard { artboard: a3, index: 0, folder: Some(FolderId(77)) })
            .is_err());
    }

    #[test]
    fn folders_lifecycle_never_deletes_art() {
        let (mut e, a) = editor();
        let out = assert_undoable(&mut e, ProjectCommand::CreateFolder { name: None, index: None });
        let f = created_folder(&out);
        assert_eq!(e.project().folder(f).unwrap().name, "Folder 1");
        assert_undoable(&mut e, ProjectCommand::RenameFolder { folder: f, name: "Characters".into() });
        e.execute(ProjectCommand::MoveArtboard { artboard: a, index: 0, folder: Some(f) }).unwrap();
        assert_undoable(&mut e, ProjectCommand::DeleteFolder { folder: f });
        assert!(e.project().folders().is_empty());
        assert_eq!(e.project().artboard(a).unwrap().folder(), None);
        e.undo().unwrap();
        assert_eq!(e.project().artboard(a).unwrap().folder(), Some(f));
        assert_eq!(e.project().folder(f).unwrap().name, "Characters");
    }

    #[test]
    fn palette_is_shared_by_every_artboard() {
        let (mut e, a1) = editor();
        let a2 = created_artboard(
            &e.execute(ProjectCommand::CreateArtboard { name: None, width: 4, height: 4, folder: None, index: None })
                .unwrap(),
        );
        let palette = Palette { name: "Mini".into(), colors: vec![RED, Rgba::WHITE] };
        assert_undoable(&mut e, ProjectCommand::SetPalette { palette: palette.clone() });
        assert_eq!(e.project().palette(), &palette);
        for id in [a1, a2] {
            assert_eq!(e.project().document(id).unwrap().palette(), &palette);
        }
        // A document-level palette edit is routed to the project.
        let other = Palette { name: "Two".into(), colors: vec![Rgba::BLACK] };
        e.execute(ProjectCommand::Edit { artboard: a2, command: Command::SetPalette { palette: other.clone() } })
            .unwrap();
        assert_eq!(e.project().document(a1).unwrap().palette(), &other);
        e.undo().unwrap();
        assert_eq!(e.project().document(a2).unwrap().palette(), &palette);
    }

    #[test]
    fn edits_report_artboard_scoped_dirty_regions_and_created_ids() {
        let (mut e, a) = editor();
        let doc = e.project().document(a).unwrap().clone();
        let out = e.execute(draw(a, &doc, 2)).unwrap();
        assert!(!out.structure_changed);
        assert_eq!(out.artboards, vec![a]);
        assert_eq!(out.dirty[0].artboard, a);
        assert_eq!(out.dirty[0].rect, Rect::new(2, 0, 1, 1));
        let out = e
            .execute(ProjectCommand::Edit { artboard: a, command: Command::CreateLayer { name: None, index: None } })
            .unwrap();
        assert!(matches!(out.created[..], [ProjectCreated::Layer(_)]));
        assert_eq!(serde_json::to_value(&out.created).unwrap()[0].as_object().unwrap().keys().next().unwrap(), "layer");
        assert_eq!(
            e.execute(ProjectCommand::Edit {
                artboard: ArtboardId(42),
                command: Command::AddFrame { index: None, duration_ms: None }
            })
            .unwrap_err(),
            CoreError::ArtboardNotFound(ArtboardId(42))
        );
    }

    #[test]
    fn batches_across_artboards_are_atomic_and_one_undo_step() {
        let (mut e, a) = editor();
        let doc = e.project().document(a).unwrap().clone();
        let before = e.project().clone();
        let err = e
            .execute(ProjectCommand::Batch {
                label: Some("AI".into()),
                commands: vec![
                    draw(a, &doc, 0),
                    ProjectCommand::CreateArtboard { name: None, width: 4, height: 4, folder: None, index: None },
                    ProjectCommand::DeleteFolder { folder: FolderId(99) },
                ],
            })
            .unwrap_err();
        assert_eq!(err, CoreError::FolderNotFound(FolderId(99)));
        assert_eq!(e.project(), &before);
        assert_eq!(e.revision(), 0);

        let out = e
            .execute(ProjectCommand::Batch {
                label: Some("AI".into()),
                commands: vec![
                    ProjectCommand::CreateArtboard {
                        name: Some("Coin".into()),
                        width: 4,
                        height: 4,
                        folder: None,
                        index: None,
                    },
                    draw(a, &doc, 0),
                    ProjectCommand::Edit {
                        artboard: a,
                        command: Command::Batch {
                            label: None,
                            commands: vec![Command::AddFrame { index: None, duration_ms: None }],
                        },
                    },
                ],
            })
            .unwrap();
        assert_eq!(out.created.len(), 2);
        assert_eq!(e.undo_label(), Some("AI"));
        e.undo().unwrap();
        assert_eq!(e.project(), &before);
        assert!(!e.can_undo());
    }

    #[test]
    fn revision_checking_and_dirty_tracking() {
        let (mut e, _) = editor();
        let cmd = ProjectCommand::RenameProject { name: "Quest".into() };
        assert_eq!(
            e.execute_expecting(cmd.clone(), Some(3)).unwrap_err(),
            CoreError::RevisionMismatch { expected: 3, actual: 0 }
        );
        assert!(!e.is_dirty());
        e.execute_expecting(cmd, Some(0)).unwrap();
        assert!(e.is_dirty());
        assert_eq!(e.summary().name, "Quest");
        e.mark_saved();
        assert!(!e.is_dirty());
        e.undo().unwrap();
        assert!(e.is_dirty());
        e.redo().unwrap();
        assert!(!e.is_dirty());
    }

    #[test]
    fn project_commands_deserialize_from_json() {
        let json = r##"[
            {"type":"create_artboard","width":16,"height":16,"name":"Slime"},
            {"type":"move_artboard","artboard":1,"index":0},
            {"type":"edit","artboard":1,"command":{"type":"add_frame"}}
        ]"##;
        let cmds: Vec<ProjectCommand> = serde_json::from_str(json).unwrap();
        assert_eq!(cmds[1], ProjectCommand::MoveArtboard { artboard: ArtboardId(1), index: 0, folder: None });
        let (mut e, _) = editor();
        e.execute(ProjectCommand::Batch { label: None, commands: cmds }).unwrap();
        assert_eq!(e.project().artboards().len(), 2);
    }
}
