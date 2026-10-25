//! The Dotify tool set, independent of the MCP transport. Every edit goes
//! through `dotify_core::Editor::execute_expecting`, so each tool call (or
//! `batch`) is one undoable transaction, exactly as in the desktop app.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use dotify_core::composite::composite_frame;
use dotify_core::format::{self, WriteOptions};
use dotify_core::{Command, Document, Editor, FrameId, LayerId, Outcome};
use dotify_export::gif::GifOptions;
use dotify_export::godot::GodotOptions;
use dotify_export::PngExportOptions;
use schemars::generate::SchemaSettings;
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde_json::{json, Map, Value};

use crate::args::*;
use crate::error::{ToolError, ToolResult};
use crate::sandbox::Sandbox;
use crate::workspace::{OpenProject, Workspace};

pub type JsonObject = Map<String, Value>;

/// Largest side of a get_canvas PNG, in pixels.
pub const MAX_PREVIEW_SIDE: u32 = 4096;
/// Largest region get_canvas returns as text.
pub const MAX_TEXT_PIXELS_SIDE: u32 = 128;
pub const MAX_BATCH_OPS: usize = 10_000;
const PIXEL_CHARS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";

pub struct ToolDef {
    pub name: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    /// Changes a document or writes files; hidden and refused in read-only mode.
    pub writes: bool,
    /// Removes content; needs `confirm: true`.
    pub destructive: bool,
    pub input_schema: Arc<JsonObject>,
}

pub enum ToolOutput {
    Json(Value),
    /// A PNG plus JSON describing it.
    Image {
        png: Vec<u8>,
        info: Value,
    },
}

pub struct Options {
    pub sandbox: Sandbox,
    pub read_only: bool,
}

/// Shared server state: configuration plus the open projects. One instance
/// serves every MCP session.
pub struct Dotify {
    sandbox: Sandbox,
    read_only: bool,
    workspace: Mutex<Workspace>,
    tools: Vec<ToolDef>,
}

fn schema_of<T: JsonSchema>() -> JsonObject {
    let generator = SchemaSettings::draft2020_12()
        .with(|s| {
            s.inline_subschemas = true;
            s.meta_schema = None;
        })
        .into_generator();
    match generator.into_root_schema_for::<T>().to_value() {
        Value::Object(mut o) => {
            o.remove("title");
            o
        }
        _ => unreachable!("object schemas only"),
    }
}

/// Schema of an edit tool: the op's own fields plus `project`,
/// `expected_revision` and, for destructive ops, `confirm`.
fn edit_schema<T: JsonSchema>(destructive: bool) -> JsonObject {
    let mut s = schema_of::<T>();
    let props = s.entry("properties").or_insert_with(|| json!({})).as_object_mut().expect("properties object");
    props.insert(
        "project".into(),
        json!({ "type": "string", "description": "Project handle returned by project_create / project_open." }),
    );
    props.insert(
        "expected_revision".into(),
        json!({
            "type": "integer", "minimum": 0,
            "description": "Refuse to apply unless the project is at this revision (from the last result or project_get)."
        }),
    );
    if destructive {
        props.insert(
            "confirm".into(),
            json!({ "type": "boolean", "description": "Must be true: this removes content (it can still be undone)." }),
        );
    }
    let required = s.entry("required").or_insert_with(|| json!([])).as_array_mut().expect("required array");
    required.insert(0, json!("project"));
    if destructive {
        required.push(json!("confirm"));
    }
    s
}

macro_rules! tools {
    ($( $name:literal, $title:literal, $writes:literal, $destructive:literal, $schema:expr, $desc:literal; )*) => {
        vec![$(ToolDef {
            name: $name,
            title: $title,
            description: $desc,
            writes: $writes,
            destructive: $destructive,
            input_schema: Arc::new($schema),
        }),*]
    };
}

fn definitions() -> Vec<ToolDef> {
    tools! {
        "project_list", "List projects", false, false, schema_of::<Empty>(),
            "List the open projects and the .dotify files inside the allowed root directories.";
        "project_create", "Create project", true, false, schema_of::<ProjectCreate>(),
            "Create a new project with one layer and one frame, optionally saving it to a .dotify path. Returns its handle and structure.";
        "project_open", "Open project", false, false, schema_of::<ProjectOpen>(),
            "Open a .dotify file. Returns the project handle and its structure (layer and frame IDs, palette, tags, revision).";
        "project_get", "Get project", false, false, schema_of::<ProjectRef>(),
            "Get a project's structure: size, layers (bottom to top) and frames with their IDs, palette, tags, revision, unsaved changes and undo state.";
        "project_save", "Save project", true, false, schema_of::<ProjectSave>(),
            "Save a project to its file, or to a new .dotify path. Replacing a different existing file needs confirm: true.";
        "project_close", "Close project", false, false, schema_of::<ProjectClose>(),
            "Close a project. Discarding unsaved changes needs confirm: true.";
        "get_canvas", "Get canvas", false, false, schema_of::<GetCanvas>(),
            "Render a frame (all visible layers, or one layer) as a PNG image so you can see it. Optionally also returns the exact pixels as text.";
        "layer_list", "List layers", false, false, schema_of::<ProjectRef>(),
            "List a project's layers from bottom to top with their IDs, visibility, lock state and opacity.";
        "set_pixels", "Set pixels", true, false, edit_schema::<SetPixels>(false),
            "Set individual pixels on one layer and frame. Colors replace pixels; use #00000000 to erase.";
        "draw_line", "Draw line", true, false, edit_schema::<DrawLine>(false),
            "Draw a 1-pixel line between two points (inclusive).";
        "draw_rectangle", "Draw rectangle", true, false, edit_schema::<DrawShape>(false),
            "Draw a rectangle outline, or a filled rectangle with filled: true.";
        "draw_ellipse", "Draw ellipse", true, false, edit_schema::<DrawShape>(false),
            "Draw an ellipse inside a bounding box, outlined or filled.";
        "fill_region", "Fill region", true, false, edit_schema::<FillRegion>(false),
            "Bucket fill from a point: the connected area of that color (default), or every pixel of that color with contiguous: false.";
        "replace_color", "Replace color", true, false, edit_schema::<ReplaceColor>(false),
            "Replace every pixel of one color with another on one layer and frame.";
        "layer_create", "Create layer", true, false, edit_schema::<LayerCreate>(false),
            "Add an empty layer. The result's `created` holds its ID.";
        "layer_update", "Update layer", true, false, edit_schema::<LayerUpdate>(false),
            "Rename, show/hide, lock/unlock, change opacity or move a layer.";
        "layer_delete", "Delete layer", true, true, edit_schema::<LayerRef>(true),
            "Delete a layer and its pixels on every frame. Needs confirm: true.";
        "add_frame", "Add frame", true, false, edit_schema::<AddFrame>(false),
            "Add an empty frame. The result's `created` holds its ID.";
        "duplicate_frame", "Duplicate frame", true, false, edit_schema::<FrameRef>(false),
            "Insert a copy of a frame right after it.";
        "delete_frame", "Delete frame", true, true, edit_schema::<FrameRef>(true),
            "Delete a frame and its pixels on every layer. Needs confirm: true.";
        "move_frame", "Move frame", true, false, edit_schema::<MoveFrame>(false),
            "Move a frame to another timeline position.";
        "set_frame_duration", "Set frame duration", true, false, edit_schema::<SetFrameDuration>(false),
            "Set how long a frame is shown, in milliseconds.";
        "copy_frame", "Copy frame", true, false, edit_schema::<CopyFrame>(false),
            "Replace the pixels of one frame with those of another, on every unlocked layer or one layer.";
        "copy_silhouette", "Copy silhouette", true, false, edit_schema::<CopySilhouette>(false),
            "Put a one-color silhouette of a frame onto another frame as a new, locked reference layer.";
        "tag_create", "Create tag", true, false, edit_schema::<TagCreate>(false),
            "Name a frame range as an animation (exported as a Godot animation or GIF).";
        "tag_update", "Update tag", true, false, edit_schema::<TagUpdate>(false),
            "Change a tag's name, frame range, direction, looping or color.";
        "tag_delete", "Delete tag", true, false, edit_schema::<TagRef>(false),
            "Delete a tag. Frames and pixels are kept.";
        "set_palette", "Set palette", true, false, edit_schema::<SetPalette>(false),
            "Replace the document palette.";
        "batch", "Batch edit", true, false, schema_of::<Batch>(),
            "Apply several edits atomically as one undo step. Each op has `op` set to an edit tool name (set_pixels, draw_line, layer_create, …) plus that tool's arguments without `project`. If any op fails, nothing is applied.";
        "undo", "Undo", true, false, schema_of::<ProjectRef>(),
            "Undo the last edit transaction.";
        "redo", "Redo", true, false, schema_of::<ProjectRef>(),
            "Redo the last undone transaction.";
        "export_png", "Export PNG", true, false, schema_of::<ExportPng>(),
            "Export one frame as a PNG. Replacing existing files needs confirm: true.";
        "export_gif", "Export GIF", true, false, schema_of::<ExportGif>(),
            "Export an animated GIF of all frames or one tag (exact colors, at most 255 per frame). Replacing existing files needs confirm: true.";
        "export_spritesheet", "Export spritesheet", true, false, schema_of::<ExportSpritesheet>(),
            "Export a spritesheet PNG plus JSON metadata next to it. Replacing existing files needs confirm: true.";
        "export_godot", "Export for Godot", true, false, schema_of::<ExportGodot>(),
            "Export for Godot 4: <name>.png, a <name>.tres SpriteFrames resource with one animation per tag, and <name>.json. Replacing existing files needs confirm: true.";
    }
}

/// Deserialize tool arguments strictly (unknown fields are rejected).
fn parse<T: DeserializeOwned>(args: JsonObject) -> ToolResult<T> {
    serde_json::from_value(Value::Object(args)).map_err(|e| ToolError::invalid(e.to_string()))
}

fn require_confirm(confirm: bool, what: impl Into<String>) -> ToolResult<()> {
    if confirm {
        Ok(())
    } else {
        Err(ToolError::confirmation_required(format!("{}; call again with confirm: true", what.into())))
    }
}

/// Refuse to replace existing files unless confirmed.
fn check_overwrite(paths: &[&Path], confirm: bool) -> ToolResult<()> {
    let existing: Vec<String> = paths.iter().filter(|p| p.exists()).map(|p| p.display().to_string()).collect();
    if existing.is_empty() || confirm {
        return Ok(());
    }
    Err(ToolError::confirmation_required(format!(
        "{} already exists; call again with confirm: true to replace it",
        existing.join(", ")
    ))
    .with_details(json!({ "existing": existing })))
}

fn project_json(id: &str, p: &OpenProject) -> Value {
    json!({
        "project": id,
        "path": p.path,
        "summary": p.editor.summary(),
    })
}

fn outcome_json(id: &str, outcome: &Outcome, editor: &Editor) -> Value {
    json!({
        "project": id,
        "revision": outcome.revision,
        "changed": outcome.changed,
        "label": outcome.label,
        "created": outcome.created,
        "changed_regions": outcome.dirty,
        "structure_changed": outcome.structure_changed,
        "unsaved_changes": editor.is_dirty(),
    })
}

fn first_frame(doc: &Document, frame: Option<u32>) -> ToolResult<FrameId> {
    match frame {
        Some(f) => Ok(doc.frame(FrameId(f))?.id),
        None => Ok(doc.frames()[0].id),
    }
}

impl Dotify {
    pub fn new(options: Options) -> Self {
        Self {
            sandbox: options.sandbox,
            read_only: options.read_only,
            workspace: Mutex::new(Workspace::default()),
            tools: definitions(),
        }
    }

    pub fn read_only(&self) -> bool {
        self.read_only
    }

    pub fn sandbox(&self) -> &Sandbox {
        &self.sandbox
    }

    /// The tools this server offers (writing tools are left out in
    /// read-only mode).
    pub fn tools(&self) -> impl Iterator<Item = &ToolDef> {
        self.tools.iter().filter(|t| !(self.read_only && t.writes))
    }

    pub fn tool(&self, name: &str) -> Option<&ToolDef> {
        self.tools().find(|t| t.name == name)
    }

    fn workspace(&self) -> MutexGuard<'_, Workspace> {
        // A panic mid-call leaves the editor consistent (commands roll back),
        // so a poisoned lock is still usable.
        self.workspace.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Run a tool. Unknown tools and tools hidden by read-only mode fail
    /// with `unknown_tool` / `read_only`.
    pub fn call(&self, name: &str, args: JsonObject) -> ToolResult<ToolOutput> {
        let Some(def) = self.tools.iter().find(|t| t.name == name) else {
            return Err(ToolError::new("unknown_tool", format!("no tool named {name:?}")));
        };
        if self.read_only && def.writes {
            return Err(ToolError::new("read_only", format!("{name} is not available: the server is read-only")));
        }
        let json = |v: ToolResult<Value>| v.map(ToolOutput::Json);
        match name {
            "project_list" => json(parse::<Empty>(args).map(|_| self.project_list())),
            "project_create" => json(self.project_create(parse(args)?)),
            "project_open" => json(self.project_open(parse(args)?)),
            "project_get" => {
                let a: ProjectRef = parse(args)?;
                let ws = self.workspace();
                Ok(ToolOutput::Json(project_json(&a.project, ws.get(&a.project)?)))
            }
            "project_save" => json(self.project_save(parse(args)?)),
            "project_close" => json(self.project_close(parse(args)?)),
            "get_canvas" => self.get_canvas(parse(args)?),
            "layer_list" => {
                let a: ProjectRef = parse(args)?;
                let ws = self.workspace();
                let doc = ws.get(&a.project)?.editor.document();
                Ok(ToolOutput::Json(json!({ "project": a.project, "layers": doc.layers() })))
            }
            "batch" => json(self.batch(parse(args)?)),
            "undo" | "redo" => json(self.undo_redo(parse(args)?, name == "undo")),
            "export_png" => json(self.export_png(parse(args)?)),
            "export_gif" => json(self.export_gif(parse(args)?)),
            "export_spritesheet" => json(self.export_spritesheet(parse(args)?)),
            "export_godot" => json(self.export_godot(parse(args)?)),
            _ => json(self.edit_tool(def, args)),
        }
    }

    fn edit_tool(&self, def: &ToolDef, mut args: JsonObject) -> ToolResult<Value> {
        let project = match args.remove("project") {
            Some(Value::String(s)) => s,
            Some(_) => return Err(ToolError::invalid("project must be a string")),
            None => return Err(ToolError::invalid("missing field `project`")),
        };
        let expected = match args.remove("expected_revision") {
            None | Some(Value::Null) => None,
            Some(v) => Some(v.as_u64().ok_or_else(|| ToolError::invalid("expected_revision must be an integer"))?),
        };
        let confirm = match (def.destructive, args.remove("confirm")) {
            (_, None | Some(Value::Null)) => false,
            (true, Some(Value::Bool(b))) => b,
            (true, Some(_)) => return Err(ToolError::invalid("confirm must be a boolean")),
            (false, Some(_)) => return Err(ToolError::invalid("unknown field `confirm`")),
        };
        args.insert("op".into(), Value::String(def.name.into()));
        let op: EditOp = parse(args)?;
        if op.is_destructive() {
            require_confirm(confirm, format!("{} removes content", def.name))?;
        }
        self.execute(&project, op.into_command(), expected)
    }

    fn execute(&self, project: &str, cmd: Command, expected: Option<u64>) -> ToolResult<Value> {
        let mut ws = self.workspace();
        let p = ws.get_mut(project)?;
        let outcome = p.editor.execute_expecting(cmd, expected)?;
        Ok(outcome_json(project, &outcome, &p.editor))
    }

    fn batch(&self, a: Batch) -> ToolResult<Value> {
        if a.ops.is_empty() {
            return Err(ToolError::invalid("ops must not be empty"));
        }
        if a.ops.len() > MAX_BATCH_OPS {
            return Err(ToolError::invalid(format!("a batch is limited to {MAX_BATCH_OPS} ops")));
        }
        let destructive: Vec<usize> =
            a.ops.iter().enumerate().filter(|(_, o)| o.is_destructive()).map(|(i, _)| i).collect();
        if !destructive.is_empty() && !a.confirm {
            return Err(ToolError::confirmation_required(
                "this batch deletes layers or frames; call again with confirm: true",
            )
            .with_details(json!({ "ops": destructive })));
        }
        let commands = a.ops.into_iter().map(EditOp::into_command).collect();
        self.execute(&a.project, Command::Batch { label: a.label, commands }, a.expected_revision)
    }

    fn undo_redo(&self, a: ProjectRef, undo: bool) -> ToolResult<Value> {
        let mut ws = self.workspace();
        let p = ws.get_mut(&a.project)?;
        let outcome = if undo { p.editor.undo() } else { p.editor.redo() };
        Ok(match outcome {
            Some(o) => outcome_json(&a.project, &o, &p.editor),
            None => json!({
                "project": a.project,
                "revision": p.editor.revision(),
                "changed": false,
                "message": if undo { "nothing to undo" } else { "nothing to redo" },
            }),
        })
    }

    fn project_list(&self) -> Value {
        let ws = self.workspace();
        let open: Vec<Value> = ws
            .iter()
            .map(|(id, p)| {
                json!({
                    "project": id,
                    "name": p.editor.document().name(),
                    "path": p.path,
                    "revision": p.editor.revision(),
                    "unsaved_changes": p.editor.is_dirty(),
                })
            })
            .collect();
        json!({
            "roots": self.sandbox.roots(),
            "read_only": self.read_only,
            "open": open,
            "files": self.sandbox.find_projects(6, 500),
        })
    }

    /// Refuse to write over a file another open project uses.
    fn check_not_open(ws: &Workspace, path: &Path, except: Option<&str>) -> ToolResult<()> {
        match ws.find_path(path) {
            Some(id) if Some(id) != except => Err(ToolError::new(
                "project_open",
                format!("{} is open as project {id}; close it first", path.display()),
            )),
            _ => Ok(()),
        }
    }

    fn project_create(&self, a: ProjectCreate) -> ToolResult<Value> {
        let path = a.path.as_deref().map(|p| self.sandbox.writable_file(p, &["dotify"])).transpose()?;
        let mut ws = self.workspace();
        if let Some(path) = &path {
            Self::check_not_open(&ws, path, None)?;
            check_overwrite(&[path], a.confirm)?;
        }
        let name = a.name.unwrap_or_else(|| {
            path.as_ref()
                .and_then(|p| p.file_stem())
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "Untitled".into())
        });
        let doc = Document::new(name, a.width, a.height)?;
        if let Some(path) = &path {
            format::save(&doc, path, WriteOptions::default())?;
        }
        let id = ws.insert(OpenProject { editor: Editor::new(doc), path })?;
        Ok(project_json(&id, ws.get(&id)?))
    }

    fn project_open(&self, a: ProjectOpen) -> ToolResult<Value> {
        let path = self.sandbox.existing_file(&a.path, &["dotify"])?;
        let mut ws = self.workspace();
        if let Some(id) = ws.find_path(&path).map(str::to_string) {
            let mut v = project_json(&id, ws.get(&id)?);
            v["already_open"] = json!(true);
            return Ok(v);
        }
        let doc = format::load(&path)?;
        let id = ws.insert(OpenProject { editor: Editor::new(doc), path: Some(path) })?;
        Ok(project_json(&id, ws.get(&id)?))
    }

    fn project_save(&self, a: ProjectSave) -> ToolResult<Value> {
        let requested = a.path.as_deref().map(|p| self.sandbox.writable_file(p, &["dotify"])).transpose()?;
        let mut ws = self.workspace();
        let own = ws.get(&a.project)?.path.clone();
        let target = match (requested, &own) {
            (Some(t), _) => t,
            (None, Some(own)) => own.clone(),
            (None, None) => {
                return Err(ToolError::new("no_path", "this project has no file yet; pass a .dotify path"));
            }
        };
        Self::check_not_open(&ws, &target, Some(&a.project))?;
        if own.as_ref() != Some(&target) {
            check_overwrite(&[&target], a.confirm)?;
        }
        let p = ws.get_mut(&a.project)?;
        format::save(p.editor.document(), &target, WriteOptions::default())?;
        p.editor.mark_saved();
        p.path = Some(target.clone());
        Ok(json!({ "project": a.project, "path": target, "revision": p.editor.revision() }))
    }

    fn project_close(&self, a: ProjectClose) -> ToolResult<Value> {
        let mut ws = self.workspace();
        if ws.get(&a.project)?.editor.is_dirty() {
            require_confirm(a.confirm, format!("project {} has unsaved changes", a.project))?;
        }
        ws.remove(&a.project)?;
        Ok(json!({ "project": a.project, "closed": true }))
    }

    fn get_canvas(&self, a: GetCanvas) -> ToolResult<ToolOutput> {
        let ws = self.workspace();
        let doc = ws.get(&a.project)?.editor.document();
        let frame = first_frame(doc, a.frame)?;
        let image = match a.layer {
            Some(l) => {
                doc.layer(LayerId(l))?;
                doc.cel_image(LayerId(l), frame)
            }
            None => composite_frame(doc, frame)?,
        };
        let side = doc.width().max(doc.height());
        let scale = a.scale.unwrap_or((256 / side).clamp(1, 16));
        if scale == 0 || scale > 32 {
            return Err(ToolError::invalid("scale must be between 1 and 32"));
        }
        if side * scale > MAX_PREVIEW_SIDE {
            return Err(ToolError::new(
                "too_large",
                format!("the preview would be {} pixels wide; use a smaller scale", side * scale),
            ));
        }
        let png = dotify_core::png_io::encode_rgba(&image.scaled(scale)).map_err(dotify_export::ExportError::from)?;
        let mut info = json!({
            "project": a.project,
            "revision": ws.get(&a.project)?.editor.revision(),
            "frame": frame,
            "layer": a.layer,
            "width": doc.width(),
            "height": doc.height(),
            "scale": scale,
        });
        if a.include_pixels {
            info["pixels"] = pixels_text(&image)?;
        }
        Ok(ToolOutput::Image { png, info })
    }

    fn export_png(&self, a: ExportPng) -> ToolResult<Value> {
        let path = self.sandbox.writable_file(&a.path, &["png"])?;
        check_overwrite(&[&path], a.confirm)?;
        let ws = self.workspace();
        let doc = ws.get(&a.project)?.editor.document();
        let frame = first_frame(doc, a.frame)?;
        dotify_export::export_png(doc, PngExportOptions { frame, scale: a.scale.unwrap_or(1) }, &path)?;
        Ok(json!({ "files": [path] }))
    }

    fn export_gif(&self, a: ExportGif) -> ToolResult<Value> {
        let path = self.sandbox.writable_file(&a.path, &["gif"])?;
        check_overwrite(&[&path], a.confirm)?;
        let ws = self.workspace();
        let doc = ws.get(&a.project)?.editor.document();
        let opts = GifOptions { scale: a.scale.unwrap_or(1), tag: a.tag };
        let report = dotify_export::gif::export_gif(doc, &opts, &path)?;
        Ok(json!({ "files": [path], "report": report }))
    }

    fn export_spritesheet(&self, a: ExportSpritesheet) -> ToolResult<Value> {
        let path = self.sandbox.writable_file(&a.path, &["png"])?;
        let json_path = path.with_extension("json");
        check_overwrite(&[&path, &json_path], a.confirm)?;
        let ws = self.workspace();
        let doc = ws.get(&a.project)?.editor.document();
        let json_path = dotify_export::export_spritesheet(doc, &a.sheet.options(), a.sheet.pivot(), &path)?;
        Ok(json!({ "files": [path, json_path] }))
    }

    fn export_godot(&self, a: ExportGodot) -> ToolResult<Value> {
        let dir = self.sandbox.writable_dir(&a.dir)?;
        let ws = self.workspace();
        let doc = ws.get(&a.project)?.editor.document();
        let stem = dotify_export::godot::file_stem(a.name.as_deref().unwrap_or(doc.name()));
        let files: Vec<PathBuf> = ["png", "tres", "json"].iter().map(|e| dir.join(format!("{stem}.{e}"))).collect();
        check_overwrite(&files.iter().map(PathBuf::as_path).collect::<Vec<_>>(), a.confirm)?;
        let opts = GodotOptions { name: a.name, sheet: a.sheet.options(), res_dir: a.res_dir, pivot: a.sheet.pivot() };
        let written = dotify_export::godot::export_godot(doc, &opts, &dir)?;
        Ok(json!({ "files": [written.png, written.tres, written.json] }))
    }
}

/// The image as rows of one character per pixel, with a legend.
fn pixels_text(image: &dotify_core::Image) -> ToolResult<Value> {
    let (w, h) = (image.width(), image.height());
    if w > MAX_TEXT_PIXELS_SIDE || h > MAX_TEXT_PIXELS_SIDE {
        return Err(ToolError::new(
            "too_large",
            format!("include_pixels supports up to {MAX_TEXT_PIXELS_SIDE}×{MAX_TEXT_PIXELS_SIDE} pixels"),
        ));
    }
    let mut colors: Vec<dotify_core::Rgba> = Vec::new();
    let mut rows = Vec::with_capacity(h as usize);
    for y in 0..h {
        let mut row = String::with_capacity(w as usize);
        for x in 0..w {
            let c = image.get(x, y);
            if c.a == 0 {
                row.push('.');
                continue;
            }
            let i = match colors.iter().position(|k| *k == c) {
                Some(i) => i,
                None => {
                    if colors.len() == PIXEL_CHARS.len() {
                        return Err(ToolError::new(
                            "too_many_colors",
                            format!("include_pixels supports up to {} colors", PIXEL_CHARS.len()),
                        ));
                    }
                    colors.push(c);
                    colors.len() - 1
                }
            };
            row.push(PIXEL_CHARS[i] as char);
        }
        rows.push(row);
    }
    let legend: Map<String, Value> =
        colors.iter().enumerate().map(|(i, c)| ((PIXEL_CHARS[i] as char).to_string(), json!(c.to_hex()))).collect();
    Ok(json!({ "legend": legend, "rows": rows }))
}
