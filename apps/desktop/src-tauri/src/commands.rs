//! IPC commands exposed to the webview. File paths never come from the
//! webview: every path is chosen by the user in a native dialog opened here.

use std::path::PathBuf;

use dotify_core::composite::composite_frame;
use dotify_core::format::{self, WriteOptions};
use dotify_core::{ArtboardId, FrameId, LayerId, Project, ProjectCommand, ProjectOutcome, Rect};
use dotify_export::project::PngArtboardExporter;
use dotify_export::PngExportOptions;
use serde::Serialize;
use tauri::ipc::Response;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::DialogExt;

use crate::session::{ensure_extension, AppState, RecoveryInfo, Session, SessionView};

#[derive(Debug, Serialize)]
pub struct CmdError {
    code: String,
    message: String,
}

impl CmdError {
    fn new(code: &str, message: impl Into<String>) -> Self {
        Self { code: code.into(), message: message.into() }
    }
}

impl From<dotify_core::CoreError> for CmdError {
    fn from(e: dotify_core::CoreError) -> Self {
        Self::new(e.code(), e.to_string())
    }
}

impl From<format::FormatError> for CmdError {
    fn from(e: format::FormatError) -> Self {
        Self::new(e.code(), e.to_string())
    }
}

impl From<dotify_export::ExportError> for CmdError {
    fn from(e: dotify_export::ExportError) -> Self {
        Self::new(e.code(), e.to_string())
    }
}

type CmdResult<T> = Result<T, CmdError>;

/// Keep the native window title in sync with the project.
fn sync_title(app: &AppHandle, session: &Session) {
    if let Some(win) = app.get_webview_window("main") {
        let dirty = if session.editor.is_dirty() { " — Edited" } else { "" };
        let _ = win.set_title(&format!("{}{dirty}", session.file_name()));
    }
}

fn replace_session(app: &AppHandle, state: &AppState, session: Session) -> SessionView {
    let mut s = state.lock();
    *s = session;
    sync_title(app, &s);
    s.view()
}

#[tauri::command]
pub fn doc_view(app: AppHandle, state: State<'_, AppState>) -> SessionView {
    let s = state.lock();
    sync_title(&app, &s);
    s.view()
}

#[tauri::command]
pub fn doc_new(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
    width: u32,
    height: u32,
) -> CmdResult<SessionView> {
    let name = if name.trim().is_empty() { "Untitled".to_string() } else { name.trim().to_string() };
    let project = Project::new(name, width, height)?;
    state.discard_recovery();
    Ok(replace_session(&app, &state, Session::new(project, None)))
}

#[tauri::command]
pub async fn doc_open(app: AppHandle, state: State<'_, AppState>) -> CmdResult<Option<SessionView>> {
    let picked = app
        .dialog()
        .file()
        .set_title("Open Project")
        .add_filter("Dotify Project", &[format::FILE_EXTENSION])
        .blocking_pick_file();
    let Some(path) = picked.and_then(|p| p.into_path().ok()) else { return Ok(None) };
    let project = format::load(&path)?;
    state.discard_recovery();
    Ok(Some(replace_session(&app, &state, Session::new(project, Some(path)))))
}

fn pick_save_path(app: &AppHandle, suggested: &str) -> Option<PathBuf> {
    app.dialog()
        .file()
        .set_title("Save Project")
        .set_file_name(suggested)
        .add_filter("Dotify Project", &[format::FILE_EXTENSION])
        .blocking_save_file()
        .and_then(|p| p.into_path().ok())
        .map(|p| ensure_extension(&p))
}

fn save_to(app: &AppHandle, state: &AppState, path: PathBuf) -> CmdResult<SessionView> {
    let (project, revision) = {
        let s = state.lock();
        (s.editor.project().clone(), s.editor.revision())
    };
    format::save(&project, &path, WriteOptions::default())?;
    let mut s = state.lock();
    // Only mark clean if nothing changed while we were writing.
    if s.editor.revision() == revision {
        s.editor.mark_saved();
    }
    s.path = Some(path);
    sync_title(app, &s);
    drop(s);
    state.discard_recovery();
    Ok(state.lock().view())
}

#[tauri::command]
pub async fn doc_save(app: AppHandle, state: State<'_, AppState>) -> CmdResult<Option<SessionView>> {
    let (path, suggested) = {
        let s = state.lock();
        (s.path.clone(), s.file_name())
    };
    let Some(path) = path.or_else(|| pick_save_path(&app, &suggested)) else { return Ok(None) };
    save_to(&app, &state, path).map(Some)
}

#[tauri::command]
pub async fn doc_save_as(app: AppHandle, state: State<'_, AppState>) -> CmdResult<Option<SessionView>> {
    let suggested = state.lock().file_name();
    let Some(path) = pick_save_path(&app, &suggested) else { return Ok(None) };
    save_to(&app, &state, path).map(Some)
}

#[tauri::command]
pub fn execute(
    app: AppHandle,
    state: State<'_, AppState>,
    command: ProjectCommand,
    expected_revision: Option<u64>,
) -> CmdResult<ProjectOutcome> {
    let mut s = state.lock();
    let result = s.editor.execute_expecting(command, expected_revision);
    #[cfg(debug_assertions)]
    eprintln!("[dotify] execute -> {:?}", result.as_ref().map(|o| (o.revision, o.label.clone(), &o.dirty)));
    let outcome = result?;
    sync_title(&app, &s);
    Ok(outcome)
}

#[tauri::command]
pub fn undo(app: AppHandle, state: State<'_, AppState>) -> Option<ProjectOutcome> {
    let mut s = state.lock();
    let outcome = s.editor.undo();
    #[cfg(debug_assertions)]
    eprintln!("[dotify] undo -> {:?}", outcome.as_ref().map(|o| (o.revision, o.label.clone(), &o.dirty)));
    sync_title(&app, &s);
    outcome
}

#[tauri::command]
pub fn redo(app: AppHandle, state: State<'_, AppState>) -> Option<ProjectOutcome> {
    let mut s = state.lock();
    let outcome = s.editor.redo();
    sync_title(&app, &s);
    outcome
}

/// Open an artboard in the editor.
#[tauri::command]
pub fn artboard_activate(app: AppHandle, state: State<'_, AppState>, artboard: u32) -> CmdResult<SessionView> {
    let mut s = state.lock();
    s.set_active(ArtboardId(artboard))?;
    sync_title(&app, &s);
    Ok(s.view())
}

/// Raw RGBA bytes of a cel region (whole canvas when `rect` is omitted).
#[tauri::command]
pub fn read_cel(
    state: State<'_, AppState>,
    artboard: u32,
    layer: u32,
    frame: u32,
    rect: Option<Rect>,
) -> CmdResult<Response> {
    let s = state.lock();
    let doc = s.editor.project().document(ArtboardId(artboard))?;
    let (layer, frame) = (LayerId(layer), FrameId(frame));
    doc.layer(layer)?;
    doc.frame(frame)?;
    let rect = rect.unwrap_or(Rect::new(0, 0, doc.width(), doc.height()));
    if rect.width == 0 || rect.height == 0 || rect.right() > doc.width() || rect.bottom() > doc.height() {
        return Err(CmdError::new("invalid_argument", "region outside the canvas"));
    }
    let bytes = match doc.cel(layer, frame) {
        Some(img) => img.read_region(rect),
        None => vec![0; rect.width as usize * rect.height as usize * 4],
    };
    Ok(Response::new(bytes))
}

/// Raw RGBA bytes of an artboard's composited frame (the first frame when
/// `frame` is omitted), for library thumbnails and the overview.
#[tauri::command]
pub fn artboard_composite(state: State<'_, AppState>, artboard: u32, frame: Option<u32>) -> CmdResult<Response> {
    let s = state.lock();
    let doc = s.editor.project().document(ArtboardId(artboard))?;
    let frame = frame.map(FrameId).unwrap_or(doc.frames()[0].id);
    Ok(Response::new(composite_frame(doc, frame)?.data().to_vec()))
}

#[tauri::command]
pub async fn export_png(
    app: AppHandle,
    state: State<'_, AppState>,
    artboard: u32,
    frame: u32,
    scale: u32,
) -> CmdResult<Option<String>> {
    let (doc, stem) = {
        let s = state.lock();
        let doc = s.editor.project().document(ArtboardId(artboard))?.clone();
        let stem = doc.name().to_string();
        (doc, stem)
    };
    let picked = app
        .dialog()
        .file()
        .set_title("Export PNG")
        .set_file_name(format!("{stem}.png"))
        .add_filter("PNG Image", &["png"])
        .blocking_save_file();
    let Some(mut path) = picked.and_then(|p| p.into_path().ok()) else { return Ok(None) };
    if path.extension().is_none_or(|e| !e.eq_ignore_ascii_case("png")) {
        path.set_extension("png");
    }
    // Encode off the UI thread; this command is async.
    dotify_export::export_png(&doc, PngExportOptions { frame: FrameId(frame), scale }, &path)?;
    Ok(Some(path.display().to_string()))
}

#[derive(Serialize)]
pub struct ExportAllResult {
    dir: String,
    files: usize,
}

/// Export every artboard as PNG into a folder the user picks, mirroring the
/// library folders as sub-directories.
#[tauri::command]
pub async fn export_all_png(
    app: AppHandle,
    state: State<'_, AppState>,
    scale: u32,
) -> CmdResult<Option<ExportAllResult>> {
    let project = state.lock().editor.project().clone();
    let picked = app.dialog().file().set_title("Export All Artboards").blocking_pick_folder();
    let Some(dir) = picked.and_then(|p| p.into_path().ok()) else { return Ok(None) };
    let written = dotify_export::project::export_project(&project, &dir, &PngArtboardExporter { scale })?;
    Ok(Some(ExportAllResult { dir: dir.display().to_string(), files: written.len() }))
}

#[tauri::command]
pub fn recovery_status(state: State<'_, AppState>) -> Option<RecoveryInfo> {
    state.recovery_info()
}

#[tauri::command]
pub fn recovery_restore(app: AppHandle, state: State<'_, AppState>) -> CmdResult<SessionView> {
    let (project, path) = state.take_recovery().map_err(|e| CmdError::new("recovery_failed", e))?;
    let mut session = Session::new(project, path);
    session.editor.mark_unsaved();
    Ok(replace_session(&app, &state, session))
}

#[tauri::command]
pub fn recovery_discard(state: State<'_, AppState>) {
    state.discard_recovery();
}
