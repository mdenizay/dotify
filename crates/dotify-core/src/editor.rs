use std::collections::BTreeMap;

use serde::Serialize;

use crate::command::{Command, Created};
use crate::error::{CoreError, CoreResult};
use crate::geometry::Rect;
use crate::model::{AnimationTag, Document, Frame, FrameId, Layer, LayerId, Palette};
use crate::ops::Op;

pub const DEFAULT_HISTORY_LIMIT: usize = 512;

/// A pixel region that changed on one cel.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize)]
pub struct DirtyRegion {
    pub layer: LayerId,
    pub frame: FrameId,
    pub rect: Rect,
}

/// Result of executing, undoing or redoing.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize)]
pub struct Outcome {
    pub revision: u64,
    /// False when the command was valid but changed nothing.
    pub changed: bool,
    /// Layers, frames, palette, tags or document properties changed.
    pub structure_changed: bool,
    pub dirty: Vec<DirtyRegion>,
    pub created: Vec<Created>,
    pub label: Option<String>,
}

#[derive(Debug)]
struct Transaction {
    id: u64,
    label: String,
    /// Ops to apply, in reverse order, to step across this transaction.
    ops: Vec<Op>,
}

#[derive(Default)]
struct Effects {
    dirty: BTreeMap<(LayerId, FrameId), Rect>,
    structure_changed: bool,
}

impl Effects {
    fn record(&mut self, op: &Op) {
        if let Some((l, f, r)) = op.dirty_region() {
            self.dirty.entry((l, f)).and_modify(|e| *e = e.union(&r)).or_insert(r);
        }
        self.structure_changed |= op.is_structural();
    }

    fn into_outcome(self, revision: u64, created: Vec<Created>, label: Option<String>) -> Outcome {
        Outcome {
            revision,
            changed: true,
            structure_changed: self.structure_changed,
            dirty: self.dirty.into_iter().map(|((layer, frame), rect)| DirtyRegion { layer, frame, rect }).collect(),
            created,
            label,
        }
    }
}

/// Snapshot of a document's structure and editor state, suitable for UIs and
/// remote clients. Does not include pixel data.
#[derive(Clone, PartialEq, Debug, Serialize)]
pub struct DocumentSummary {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub layers: Vec<Layer>,
    pub frames: Vec<Frame>,
    pub palette: Palette,
    pub tags: Vec<AnimationTag>,
    pub revision: u64,
    pub dirty: bool,
    pub can_undo: bool,
    pub can_redo: bool,
    pub undo_label: Option<String>,
    pub redo_label: Option<String>,
}

/// A document plus undo/redo history and revision tracking. This is the
/// single entry point through which every client edits a document.
pub struct Editor {
    doc: Document,
    undo: Vec<Transaction>,
    redo: Vec<Transaction>,
    revision: u64,
    next_tx: u64,
    /// ID of the transaction on top of the undo stack when last saved
    /// (`None` = the empty stack).
    saved_at: Option<Option<u64>>,
    history_limit: usize,
}

impl Editor {
    pub fn new(doc: Document) -> Self {
        Self {
            doc,
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

    pub fn document(&self) -> &Document {
        &self.doc
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

    /// True when the document differs from the last saved state.
    pub fn is_dirty(&self) -> bool {
        self.saved_at != Some(self.undo.last().map(|t| t.id))
    }

    pub fn mark_saved(&mut self) {
        self.saved_at = Some(self.undo.last().map(|t| t.id));
    }

    /// Mark the document as having unsaved changes (e.g. restored from a
    /// recovery file).
    pub fn mark_unsaved(&mut self) {
        self.saved_at = None;
    }

    pub fn summary(&self) -> DocumentSummary {
        DocumentSummary {
            name: self.doc.name.clone(),
            width: self.doc.width,
            height: self.doc.height,
            layers: self.doc.layers.clone(),
            frames: self.doc.frames.clone(),
            palette: self.doc.palette.clone(),
            tags: self.doc.tags.clone(),
            revision: self.revision,
            dirty: self.is_dirty(),
            can_undo: self.can_undo(),
            can_redo: self.can_redo(),
            undo_label: self.undo_label().map(str::to_string),
            redo_label: self.redo_label().map(str::to_string),
        }
    }

    pub fn execute(&mut self, cmd: Command) -> CoreResult<Outcome> {
        self.execute_expecting(cmd, None)
    }

    /// Execute `cmd` as one atomic, undoable transaction. If
    /// `expected_revision` is given and does not match, nothing is applied.
    /// On any error the document is left exactly as it was.
    pub fn execute_expecting(&mut self, cmd: Command, expected_revision: Option<u64>) -> CoreResult<Outcome> {
        if let Some(expected) = expected_revision {
            if expected != self.revision {
                return Err(CoreError::RevisionMismatch { expected, actual: self.revision });
            }
        }
        let label = cmd.label();
        let mut applied = Vec::new();
        let mut effects = Effects::default();
        let mut created = Vec::new();
        if let Err(e) = self.run(&cmd, &mut applied, &mut effects, &mut created) {
            self.rollback(applied);
            return Err(e);
        }
        if applied.is_empty() {
            return Ok(Outcome { revision: self.revision, created, ..Default::default() });
        }
        self.revision += 1;
        let id = self.next_tx;
        self.next_tx += 1;
        self.undo.push(Transaction { id, label: label.clone(), ops: applied });
        self.redo.clear();
        if self.undo.len() > self.history_limit {
            self.undo.remove(0);
        }
        Ok(effects.into_outcome(self.revision, created, Some(label)))
    }

    fn run(
        &mut self,
        cmd: &Command,
        applied: &mut Vec<Op>,
        effects: &mut Effects,
        created: &mut Vec<Created>,
    ) -> CoreResult<()> {
        if let Command::Batch { commands, .. } = cmd {
            for c in commands {
                self.run(c, applied, effects, created)?;
            }
            return Ok(());
        }
        let compiled = cmd.compile(&self.doc)?;
        for op in compiled.ops {
            effects.record(&op);
            applied.push(op.apply(&mut self.doc)?);
        }
        created.extend(compiled.created);
        Ok(())
    }

    fn rollback(&mut self, applied: Vec<Op>) {
        for op in applied.into_iter().rev() {
            // Inverses of ops that just succeeded cannot fail.
            let _ = op.apply(&mut self.doc);
        }
    }

    fn step(&mut self, tx: Transaction) -> (Transaction, Outcome) {
        let mut effects = Effects::default();
        let mut reversed = Vec::with_capacity(tx.ops.len());
        for op in tx.ops.into_iter().rev() {
            effects.record(&op);
            match op.apply(&mut self.doc) {
                Ok(inv) => reversed.push(inv),
                Err(e) => panic!("history is inconsistent with the document: {e}"),
            }
        }
        self.revision += 1;
        let outcome = effects.into_outcome(self.revision, Vec::new(), Some(tx.label.clone()));
        (Transaction { id: tx.id, label: tx.label, ops: reversed }, outcome)
    }

    pub fn undo(&mut self) -> Option<Outcome> {
        let tx = self.undo.pop()?;
        let (tx, outcome) = self.step(tx);
        self.redo.push(tx);
        Some(outcome)
    }

    pub fn redo(&mut self) -> Option<Outcome> {
        let tx = self.redo.pop()?;
        let (tx, outcome) = self.step(tx);
        self.undo.push(tx);
        Some(outcome)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Rgba;
    use crate::command::PixelWrite;
    use crate::geometry::Point;
    use crate::model::PlaybackDirection;

    const TAG_COLORS_FIRST: Rgba = Rgba::opaque(0x5b, 0x8c, 0xff);

    const RED: Rgba = Rgba::opaque(255, 0, 0);
    const BLUE: Rgba = Rgba::opaque(0, 0, 255);

    fn editor() -> (Editor, LayerId, FrameId) {
        let doc = Document::new("Test", 8, 8).unwrap();
        let (l, f) = (doc.layers()[0].id, doc.frames()[0].id);
        (Editor::new(doc), l, f)
    }

    fn px(e: &Editor, l: LayerId, f: FrameId, x: u32, y: u32) -> Rgba {
        e.document().cel_image(l, f).get(x, y)
    }

    #[test]
    fn set_pixels_and_undo_redo_are_exact() {
        let (mut e, l, f) = editor();
        let before = e.document().clone();
        let out = e
            .execute(Command::SetPixels {
                layer: l,
                frame: f,
                pixels: vec![PixelWrite { x: 1, y: 2, color: RED }, PixelWrite { x: 3, y: 4, color: BLUE }],
            })
            .unwrap();
        assert!(out.changed && !out.structure_changed);
        assert_eq!(out.dirty, vec![DirtyRegion { layer: l, frame: f, rect: Rect::new(1, 2, 3, 3) }]);
        let after = e.document().clone();
        assert_eq!(px(&e, l, f, 1, 2), RED);
        e.undo().unwrap();
        assert_eq!(e.document(), &before);
        e.redo().unwrap();
        assert_eq!(e.document(), &after);
        assert_eq!(e.revision(), 3);
    }

    #[test]
    fn out_of_bounds_writes_are_clipped_and_noops_skip_history() {
        let (mut e, l, f) = editor();
        let out = e
            .execute(Command::SetPixels { layer: l, frame: f, pixels: vec![PixelWrite { x: -1, y: 99, color: RED }] })
            .unwrap();
        assert!(!out.changed);
        assert!(!e.can_undo());
        assert_eq!(e.revision(), 0);
    }

    #[test]
    fn shapes_draw_expected_pixels() {
        let (mut e, l, f) = editor();
        e.execute(Command::DrawLine { layer: l, frame: f, from: Point::new(0, 0), to: Point::new(7, 7), color: RED })
            .unwrap();
        for i in 0..8 {
            assert_eq!(px(&e, l, f, i, i), RED);
        }
        e.execute(Command::DrawRectangle {
            layer: l,
            frame: f,
            x: 2,
            y: 2,
            width: 3,
            height: 3,
            color: BLUE,
            filled: true,
        })
        .unwrap();
        assert_eq!(px(&e, l, f, 3, 3), BLUE);
        assert_eq!(px(&e, l, f, 1, 1), RED);
    }

    #[test]
    fn fill_and_replace_color() {
        let (mut e, l, f) = editor();
        e.execute(Command::DrawLine { layer: l, frame: f, from: Point::new(4, 0), to: Point::new(4, 7), color: RED })
            .unwrap();
        e.execute(Command::FillRegion { layer: l, frame: f, x: 0, y: 0, color: BLUE, contiguous: true }).unwrap();
        assert_eq!(px(&e, l, f, 3, 7), BLUE);
        assert_eq!(px(&e, l, f, 5, 0), Rgba::TRANSPARENT);
        e.execute(Command::ReplaceColor { layer: l, frame: f, from: RED, to: Rgba::WHITE }).unwrap();
        assert_eq!(px(&e, l, f, 4, 4), Rgba::WHITE);
    }

    #[test]
    fn locked_layers_reject_drawing() {
        let (mut e, l, f) = editor();
        e.execute(Command::UpdateLayer { layer: l, name: None, visible: None, locked: Some(true), opacity: None })
            .unwrap();
        let err = e
            .execute(Command::DrawLine { layer: l, frame: f, from: Point::new(0, 0), to: Point::new(1, 1), color: RED })
            .unwrap_err();
        assert_eq!(err, CoreError::LayerLocked(l));
        assert_eq!(err.code(), "layer_locked");
    }

    #[test]
    fn batch_is_atomic() {
        let (mut e, l, f) = editor();
        let before = e.document().clone();
        let err = e
            .execute(Command::Batch {
                label: Some("AI edit".into()),
                commands: vec![
                    Command::SetPixels { layer: l, frame: f, pixels: vec![PixelWrite { x: 0, y: 0, color: RED }] },
                    Command::CreateLayer { name: Some("New".into()), index: None },
                    Command::DeleteLayer { layer: LayerId(999) },
                ],
            })
            .unwrap_err();
        assert_eq!(err, CoreError::LayerNotFound(LayerId(999)));
        assert_eq!(e.document(), &before);
        assert!(!e.can_undo());
        assert_eq!(e.revision(), 0);
    }

    #[test]
    fn batch_undoes_as_one_step_and_reports_created_ids() {
        let (mut e, l, f) = editor();
        let before = e.document().clone();
        let out = e
            .execute(Command::Batch {
                label: Some("AI edit".into()),
                commands: vec![
                    Command::CreateLayer { name: Some("Outline".into()), index: None },
                    Command::SetPixels { layer: l, frame: f, pixels: vec![PixelWrite { x: 0, y: 0, color: RED }] },
                    Command::AddFrame { index: None, duration_ms: Some(80) },
                ],
            })
            .unwrap();
        assert_eq!(out.created.len(), 2);
        assert_eq!(e.undo_label(), Some("AI edit"));
        assert_eq!(e.document().layers().len(), 2);
        e.undo().unwrap();
        assert_eq!(e.document(), &before);
        assert!(!e.can_undo());
    }

    #[test]
    fn revision_checking() {
        let (mut e, _, _) = editor();
        let cmd = Command::RenameDocument { name: "Hero".into() };
        assert_eq!(
            e.execute_expecting(cmd.clone(), Some(5)).unwrap_err(),
            CoreError::RevisionMismatch { expected: 5, actual: 0 }
        );
        e.execute_expecting(cmd, Some(0)).unwrap();
        assert_eq!(e.revision(), 1);
    }

    #[test]
    fn layer_lifecycle() {
        let (mut e, l1, f) = editor();
        let out = e.execute(Command::CreateLayer { name: None, index: None }).unwrap();
        let Created::Layer(l2) = out.created[0] else { panic!() };
        assert_eq!(e.document().layer(l2).unwrap().name, "Layer 2");
        e.execute(Command::SetPixels { layer: l2, frame: f, pixels: vec![PixelWrite { x: 0, y: 0, color: RED }] })
            .unwrap();
        let out = e.execute(Command::DuplicateLayer { layer: l2 }).unwrap();
        let Created::Layer(l3) = out.created[0] else { panic!() };
        assert_eq!(px(&e, l3, f, 0, 0), RED);
        assert_eq!(e.document().layer_index(l3).unwrap(), 2);
        e.execute(Command::MoveLayer { layer: l3, index: 0 }).unwrap();
        assert_eq!(e.document().layers()[0].id, l3);
        let snapshot = e.document().clone();
        e.execute(Command::DeleteLayer { layer: l2 }).unwrap();
        assert!(e.document().cel(l2, f).is_none());
        e.undo().unwrap();
        assert_eq!(e.document(), &snapshot);
        assert_eq!(px(&e, l2, f, 0, 0), RED);
        e.execute(Command::DeleteLayer { layer: l2 }).unwrap();
        e.execute(Command::DeleteLayer { layer: l3 }).unwrap();
        assert_eq!(e.execute(Command::DeleteLayer { layer: l1 }).unwrap_err(), CoreError::LastLayer);
    }

    #[test]
    fn update_layer_validates_and_skips_noops() {
        let (mut e, l, _) = editor();
        assert!(matches!(
            e.execute(Command::UpdateLayer {
                layer: l,
                name: Some("  ".into()),
                visible: None,
                locked: None,
                opacity: None
            }),
            Err(CoreError::InvalidArgument(_))
        ));
        let out = e
            .execute(Command::UpdateLayer { layer: l, name: None, visible: Some(true), locked: None, opacity: None })
            .unwrap();
        assert!(!out.changed);
        e.execute(Command::UpdateLayer {
            layer: l,
            name: Some("Ink".into()),
            visible: Some(false),
            locked: None,
            opacity: Some(128),
        })
        .unwrap();
        let layer = e.document().layer(l).unwrap();
        assert_eq!((layer.name.as_str(), layer.visible, layer.opacity), ("Ink", false, 128));
    }

    #[test]
    fn frame_lifecycle_and_tags() {
        let (mut e, l, f1) = editor();
        e.execute(Command::SetPixels { layer: l, frame: f1, pixels: vec![PixelWrite { x: 1, y: 1, color: BLUE }] })
            .unwrap();
        let Created::Frame(f2) = e.execute(Command::DuplicateFrame { frame: f1 }).unwrap().created[0] else { panic!() };
        assert_eq!(px(&e, l, f2, 1, 1), BLUE);
        let Created::Frame(f3) = e.execute(Command::AddFrame { index: Some(0), duration_ms: None }).unwrap().created[0]
        else {
            panic!()
        };
        assert_eq!(e.document().frames()[0].id, f3);
        e.execute(Command::MoveFrame { frame: f3, index: 2 }).unwrap();
        assert_eq!(e.document().frames().iter().map(|f| f.id).collect::<Vec<_>>(), vec![f1, f2, f3]);
        e.execute(Command::SetFrameDuration { frame: f2, duration_ms: 250 }).unwrap();
        assert_eq!(e.document().frame(f2).unwrap().duration_ms, 250);
        assert!(e.execute(Command::SetFrameDuration { frame: f2, duration_ms: 0 }).is_err());

        e.execute(tag_cmd("Walk", f1, f2)).unwrap();
        e.execute(Command::DeleteFrame { frame: f1 }).unwrap();
        assert_eq!((e.document().tags()[0].from, e.document().tags()[0].to), (f2, f2));
        e.undo().unwrap();
        assert_eq!(e.document().tags()[0].from, f1);
        assert_eq!(px(&e, l, f1, 1, 1), BLUE);
    }

    fn tag_cmd(name: &str, from: FrameId, to: FrameId) -> Command {
        Command::CreateTag { name: name.into(), from, to, direction: None, looping: None, color: None }
    }

    #[derive(Default)]
    struct TagPatch {
        name: Option<String>,
        from: Option<FrameId>,
        to: Option<FrameId>,
        direction: Option<PlaybackDirection>,
        looping: Option<bool>,
        color: Option<Rgba>,
    }

    fn update_tag(tag: &str, p: TagPatch) -> Command {
        let TagPatch { name, from, to, direction, looping, color } = p;
        Command::UpdateTag { tag: tag.into(), name, from, to, direction, looping, color }
    }

    fn three_frames() -> (Editor, [FrameId; 3]) {
        let (mut e, _, f1) = editor();
        let mut ids = [f1; 3];
        for id in &mut ids[1..] {
            let Created::Frame(f) = e.execute(Command::AddFrame { index: None, duration_ms: None }).unwrap().created[0]
            else {
                panic!()
            };
            *id = f;
        }
        (e, ids)
    }

    #[test]
    fn tag_lifecycle_undo_redo() {
        let (mut e, [f1, f2, f3]) = three_frames();
        let empty = e.document().clone();

        let out = e.execute(tag_cmd("Idle", f1, f2)).unwrap();
        assert!(out.structure_changed);
        assert_eq!(e.undo_label(), Some("New Tag"));
        let tag = &e.document().tags()[0];
        assert_eq!((tag.from, tag.to, tag.direction, tag.looping), (f1, f2, PlaybackDirection::Forward, true));
        assert_eq!(tag.color, TAG_COLORS_FIRST);
        let created = e.document().clone();

        e.execute(update_tag(
            "Idle",
            TagPatch {
                name: Some("Run".into()),
                to: Some(f3),
                direction: Some(PlaybackDirection::PingPong),
                looping: Some(false),
                color: Some(BLUE),
                ..Default::default()
            },
        ))
        .unwrap();
        let tag = e.document().tag("Run").unwrap().clone();
        assert_eq!((tag.to, tag.direction, tag.looping, tag.color), (f3, PlaybackDirection::PingPong, false, BLUE));
        assert_eq!(e.document().tag_range(&tag).unwrap(), 0..=2);
        let updated = e.document().clone();

        e.execute(Command::DeleteTag { tag: "Run".into() }).unwrap();
        assert!(e.document().tags().is_empty());

        e.undo().unwrap();
        assert_eq!(e.document(), &updated);
        e.undo().unwrap();
        assert_eq!(e.document(), &created);
        e.undo().unwrap();
        assert_eq!(e.document(), &empty);
        e.redo().unwrap();
        e.redo().unwrap();
        assert_eq!(e.document(), &updated);
        e.redo().unwrap();
        assert!(e.document().tags().is_empty());
    }

    #[test]
    fn tag_validation() {
        let (mut e, [f1, f2, f3]) = three_frames();
        e.execute(tag_cmd("Idle", f1, f2)).unwrap();
        let before = e.document().clone();
        let code = |r: CoreResult<Outcome>| r.unwrap_err().code();
        assert_eq!(code(e.execute(tag_cmd("Idle", f2, f3))), "invalid_argument");
        assert_eq!(code(e.execute(tag_cmd("Back", f3, f1))), "invalid_argument");
        assert_eq!(code(e.execute(tag_cmd(" ", f1, f1))), "invalid_argument");
        assert_eq!(code(e.execute(tag_cmd("Gone", f1, FrameId(999)))), "frame_not_found");
        assert_eq!(code(e.execute(Command::DeleteTag { tag: "Nope".into() })), "tag_not_found");
        e.execute(tag_cmd("Walk", f3, f3)).unwrap();
        assert_eq!(
            code(e.execute(update_tag("Walk", TagPatch { name: Some("Idle".into()), ..Default::default() }))),
            "invalid_argument"
        );
        e.undo().unwrap();
        assert_eq!(e.document(), &before);
        // Renaming to its own name and other no-ops skip history.
        let out = e.execute(update_tag("Idle", TagPatch { name: Some("Idle".into()), ..Default::default() })).unwrap();
        assert!(!out.changed);
    }

    #[test]
    fn moving_frames_keeps_tags_ordered() {
        let (mut e, [f1, f2, f3]) = three_frames();
        e.execute(tag_cmd("Walk", f1, f2)).unwrap();
        let before = e.document().clone();
        e.execute(Command::MoveFrame { frame: f1, index: 2 }).unwrap();
        let tag = &e.document().tags()[0];
        assert_eq!((tag.from, tag.to), (f2, f1));
        assert_eq!(e.document().tag_range(tag).unwrap(), 0..=2);
        e.undo().unwrap();
        assert_eq!(e.document(), &before);
        // Moves that keep the order leave tags alone.
        e.execute(Command::MoveFrame { frame: f3, index: 0 }).unwrap();
        assert_eq!((e.document().tags()[0].from, e.document().tags()[0].to), (f1, f2));
    }

    #[test]
    fn tag_commands_deserialize_from_json() {
        let json = r##"{"type":"update_tag","tag":"Idle","loop":false,"direction":"ping_pong","color":"#ff0000"}"##;
        let cmd: Command = serde_json::from_str(json).unwrap();
        assert_eq!(
            cmd,
            update_tag(
                "Idle",
                TagPatch {
                    looping: Some(false),
                    direction: Some(PlaybackDirection::PingPong),
                    color: Some(RED),
                    ..Default::default()
                }
            )
        );
    }

    #[test]
    fn dirty_tracking_follows_history() {
        let (mut e, l, f) = editor();
        assert!(!e.is_dirty());
        e.execute(Command::ClearCel { layer: l, frame: f }).unwrap();
        assert!(!e.is_dirty(), "no-op must not dirty the document");
        e.execute(Command::SetPixels { layer: l, frame: f, pixels: vec![PixelWrite { x: 0, y: 0, color: RED }] })
            .unwrap();
        assert!(e.is_dirty());
        e.mark_saved();
        assert!(!e.is_dirty());
        e.undo().unwrap();
        assert!(e.is_dirty());
        e.redo().unwrap();
        assert!(!e.is_dirty());
    }

    #[test]
    fn history_limit_drops_oldest() {
        let (e, l, f) = editor();
        let mut e = Editor { history_limit: 2, ..e };
        for x in 0..4 {
            e.execute(Command::SetPixels { layer: l, frame: f, pixels: vec![PixelWrite { x, y: 0, color: RED }] })
                .unwrap();
        }
        assert!(e.undo().is_some());
        assert!(e.undo().is_some());
        assert!(e.undo().is_none());
        assert_eq!(px(&e, l, f, 1, 0), RED);
        assert_eq!(px(&e, l, f, 2, 0), Rgba::TRANSPARENT);
    }

    #[test]
    fn commands_deserialize_from_json() {
        let json =
            r##"{"type":"draw_rectangle","layer":1,"frame":2,"x":0,"y":0,"width":4,"height":2,"color":"#ff0000"}"##;
        let cmd: Command = serde_json::from_str(json).unwrap();
        assert_eq!(
            cmd,
            Command::DrawRectangle {
                layer: LayerId(1),
                frame: FrameId(2),
                x: 0,
                y: 0,
                width: 4,
                height: 2,
                color: RED,
                filled: false
            }
        );
    }

    fn new_frame(e: &mut Editor) -> FrameId {
        let Created::Frame(f) = e.execute(Command::AddFrame { index: None, duration_ms: None }).unwrap().created[0]
        else {
            panic!()
        };
        f
    }

    fn dot(e: &mut Editor, l: LayerId, f: FrameId, x: i32, y: i32, color: Rgba) {
        e.execute(Command::SetPixels { layer: l, frame: f, pixels: vec![PixelWrite { x, y, color }] }).unwrap();
    }

    #[test]
    fn copy_frame_replaces_target_on_unlocked_layers_and_undoes() {
        let (mut e, l1, f1) = editor();
        let Created::Layer(l2) = e.execute(Command::CreateLayer { name: None, index: None }).unwrap().created[0] else {
            panic!()
        };
        let f2 = new_frame(&mut e);
        dot(&mut e, l1, f1, 1, 1, RED);
        dot(&mut e, l2, f1, 2, 2, BLUE);
        dot(&mut e, l1, f2, 6, 6, BLUE); // overwritten by the copy
        dot(&mut e, l2, f2, 7, 7, RED); // kept: l2 is locked below
        e.execute(Command::UpdateLayer { layer: l2, name: None, visible: None, locked: Some(true), opacity: None })
            .unwrap();
        let before = e.document().clone();

        e.execute(Command::CopyFrame { from: f1, to: f2, layer: None }).unwrap();
        assert_eq!(px(&e, l1, f2, 1, 1), RED);
        assert_eq!(px(&e, l1, f2, 6, 6), Rgba::TRANSPARENT);
        assert_eq!(px(&e, l2, f2, 7, 7), RED, "locked layers are skipped");
        assert_eq!(px(&e, l2, f2, 2, 2), Rgba::TRANSPARENT);
        assert_eq!(px(&e, l1, f1, 1, 1), RED, "source is untouched");
        assert_eq!(e.undo_label(), Some("Copy Frame"));

        e.undo().unwrap();
        assert_eq!(e.document(), &before);
    }

    #[test]
    fn copy_frame_validates_its_arguments() {
        let (mut e, l, f1) = editor();
        let f2 = new_frame(&mut e);
        assert!(matches!(
            e.execute(Command::CopyFrame { from: f1, to: f1, layer: None }),
            Err(CoreError::InvalidArgument(_))
        ));
        e.execute(Command::UpdateLayer { layer: l, name: None, visible: None, locked: Some(true), opacity: None })
            .unwrap();
        assert_eq!(
            e.execute(Command::CopyFrame { from: f1, to: f2, layer: Some(l) }).unwrap_err(),
            CoreError::LayerLocked(l)
        );
        // Both frames empty: valid, but nothing changes.
        e.execute(Command::UpdateLayer { layer: l, name: None, visible: None, locked: Some(false), opacity: None })
            .unwrap();
        assert!(!e.execute(Command::CopyFrame { from: f1, to: f2, layer: None }).unwrap().changed);
    }

    #[test]
    fn copy_silhouette_adds_a_flat_locked_layer_on_the_target_frame() {
        let (mut e, l1, f1) = editor();
        let f2 = new_frame(&mut e);
        dot(&mut e, l1, f1, 1, 1, RED);
        dot(&mut e, l1, f1, 3, 4, Rgba::new(0, 255, 0, 40));
        let gray = Rgba::opaque(128, 128, 128);
        let out = e
            .execute(Command::CopySilhouette {
                from: f1,
                to: f2,
                color: gray,
                name: Some("Silhouette 1".into()),
                index: Some(0),
                opacity: Some(100),
                locked: true,
            })
            .unwrap();
        let Created::Layer(sil) = out.created[0] else { panic!() };
        let layer = e.document().layer(sil).unwrap();
        assert_eq!((layer.name.as_str(), layer.opacity, layer.locked), ("Silhouette 1", 100, true));
        assert_eq!(e.document().layer_index(sil).unwrap(), 0, "placed below the drawing layer");
        assert_eq!(px(&e, sil, f2, 1, 1), gray);
        assert_eq!(px(&e, sil, f2, 3, 4), gray, "any visible pixel joins the silhouette");
        assert_eq!(px(&e, sil, f2, 0, 0), Rgba::TRANSPARENT);
        assert!(e.document().cel(sil, f1).is_none(), "only the target frame gets pixels");
        e.undo().unwrap();
        assert!(e.document().layer(sil).is_err());
    }

    #[test]
    fn copy_silhouette_rejects_empty_or_hidden_sources() {
        let (mut e, l1, f1) = editor();
        let f2 = new_frame(&mut e);
        let sil = |from| Command::CopySilhouette {
            from,
            to: f2,
            color: RED,
            name: None,
            index: None,
            opacity: None,
            locked: true,
        };
        assert!(matches!(e.execute(sil(f1)), Err(CoreError::InvalidArgument(_))));
        dot(&mut e, l1, f1, 0, 0, BLUE);
        e.execute(Command::UpdateLayer { layer: l1, name: None, visible: Some(false), locked: None, opacity: None })
            .unwrap();
        assert!(matches!(e.execute(sil(f1)), Err(CoreError::InvalidArgument(_))), "hidden layers don't count");
        assert!(!e.can_redo());
    }
}
