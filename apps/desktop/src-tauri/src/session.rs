//! The open project, its active artboard, its file path, and crash-recovery
//! autosave.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use dotify_core::format::{self, WriteOptions};
use dotify_core::{ArtboardId, DocumentSummary, Project, ProjectEditor, ProjectSummary};
use serde::{Deserialize, Serialize};

pub const AUTOSAVE_INTERVAL: Duration = Duration::from_secs(30);

pub struct Session {
    pub editor: ProjectEditor,
    pub path: Option<PathBuf>,
    /// The artboard open in the editor. Use [`Session::active`], which falls
    /// back to the first artboard if this one was deleted (e.g. by undo).
    active: ArtboardId,
    /// Revision captured by the last recovery write.
    pub autosaved_revision: Option<u64>,
}

impl Session {
    pub fn new(project: Project, path: Option<PathBuf>) -> Self {
        let active = project.artboards()[0].id();
        Self { editor: ProjectEditor::new(project), path, active, autosaved_revision: None }
    }

    pub fn active(&self) -> ArtboardId {
        let project = self.editor.project();
        match project.artboard(self.active) {
            Ok(a) => a.id(),
            Err(_) => project.artboards()[0].id(),
        }
    }

    pub fn set_active(&mut self, artboard: ArtboardId) -> Result<(), dotify_core::CoreError> {
        self.active = self.editor.project().artboard(artboard)?.id();
        Ok(())
    }

    pub fn file_name(&self) -> String {
        match &self.path {
            Some(p) => p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
            None => format!("{}.{}", self.editor.project().name(), format::FILE_EXTENSION),
        }
    }

    pub fn view(&self) -> SessionView {
        let active = self.active();
        SessionView {
            project: self.editor.summary(),
            active_artboard: active,
            summary: self.editor.document_summary(active).expect("active artboard exists"),
            path: self.path.as_ref().map(|p| p.display().to_string()),
            file_name: self.file_name(),
        }
    }
}

#[derive(Serialize)]
pub struct SessionView {
    pub project: ProjectSummary,
    pub active_artboard: ArtboardId,
    /// The active artboard, with the project's history state.
    pub summary: DocumentSummary,
    pub path: Option<String>,
    pub file_name: String,
}

#[derive(Serialize, Deserialize)]
pub struct RecoveryInfo {
    pub name: String,
    pub original_path: Option<String>,
    /// Unix time in milliseconds.
    pub saved_at: u64,
}

pub struct AppState {
    session: Mutex<Session>,
    recovery_dir: PathBuf,
}

impl AppState {
    pub fn new(recovery_dir: PathBuf) -> Self {
        let project = Project::new("Untitled", 32, 32).expect("default canvas size is valid");
        Self { session: Mutex::new(Session::new(project, None)), recovery_dir }
    }

    pub fn lock(&self) -> MutexGuard<'_, Session> {
        // A panic while holding the lock leaves the editor in a consistent
        // state (ops are applied atomically), so recover from poisoning.
        self.session.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn recovery_file(&self) -> PathBuf {
        self.recovery_dir.join("recovery.dotify")
    }

    fn recovery_meta(&self) -> PathBuf {
        self.recovery_dir.join("recovery.json")
    }

    /// Write a recovery copy if the project changed since the last one.
    pub fn autosave(&self) -> Result<bool, String> {
        let (project, info, revision) = {
            let s = self.lock();
            if !s.editor.is_dirty() || s.autosaved_revision == Some(s.editor.revision()) {
                return Ok(false);
            }
            let info = RecoveryInfo {
                name: s.editor.project().name().to_string(),
                original_path: s.path.as_ref().map(|p| p.display().to_string()),
                saved_at: SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0),
            };
            (s.editor.project().clone(), info, s.editor.revision())
        };
        std::fs::create_dir_all(&self.recovery_dir).map_err(|e| e.to_string())?;
        format::save(&project, &self.recovery_file(), WriteOptions { thumbnail: false }).map_err(|e| e.to_string())?;
        let meta = serde_json::to_vec(&info).map_err(|e| e.to_string())?;
        dotify_export::write_atomic(&self.recovery_meta(), &meta).map_err(|e| e.to_string())?;
        self.lock().autosaved_revision = Some(revision);
        Ok(true)
    }

    pub fn recovery_info(&self) -> Option<RecoveryInfo> {
        if !self.recovery_file().exists() {
            return None;
        }
        let meta = std::fs::read(self.recovery_meta()).ok()?;
        serde_json::from_slice(&meta).ok()
    }

    pub fn take_recovery(&self) -> Result<(Project, Option<PathBuf>), String> {
        let project = format::load(&self.recovery_file()).map_err(|e| e.to_string())?;
        let path = self.recovery_info().and_then(|i| i.original_path).map(PathBuf::from);
        Ok((project, path))
    }

    pub fn discard_recovery(&self) {
        let _ = std::fs::remove_file(self.recovery_file());
        let _ = std::fs::remove_file(self.recovery_meta());
    }
}

pub fn ensure_extension(path: &Path) -> PathBuf {
    if path.extension().is_some_and(|e| e == format::FILE_EXTENSION) {
        path.to_path_buf()
    } else {
        path.with_extension(format::FILE_EXTENSION)
    }
}
