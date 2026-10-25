//! Reference images shown behind (or over) the canvas. The user picks a file
//! in a native dialog; only paths picked that way can be read back, so the
//! webview still cannot read arbitrary files. Picked paths are remembered in
//! `<app data>/references.json` so references come back after a restart.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use tauri::ipc::Response;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use crate::commands::{CmdError, CmdResult};

const MAX_REFERENCE_BYTES: u64 = 64 * 1024 * 1024;
const EXTENSIONS: [&str; 6] = ["png", "jpg", "jpeg", "gif", "webp", "bmp"];

pub struct ReferenceStore {
    path: PathBuf,
    allowed: Mutex<BTreeSet<PathBuf>>,
}

impl ReferenceStore {
    pub fn new(path: PathBuf) -> Self {
        let allowed = std::fs::read(&path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        Self { path, allowed: Mutex::new(allowed) }
    }

    fn allow(&self, file: &Path) -> CmdResult<()> {
        let mut allowed = self.allowed.lock().unwrap_or_else(|e| e.into_inner());
        if allowed.insert(file.to_path_buf()) {
            if let Some(dir) = self.path.parent() {
                std::fs::create_dir_all(dir).map_err(|e| CmdError::new("io_error", e.to_string()))?;
            }
            let json = serde_json::to_vec(&*allowed).map_err(|e| CmdError::new("internal", e.to_string()))?;
            dotify_export::write_atomic(&self.path, &json).map_err(|e| CmdError::new("io_error", e.to_string()))?;
        }
        Ok(())
    }

    fn is_allowed(&self, file: &Path) -> bool {
        self.allowed.lock().unwrap_or_else(|e| e.into_inner()).contains(file)
    }
}

fn read_image(file: &Path) -> CmdResult<Vec<u8>> {
    let meta = std::fs::metadata(file).map_err(|e| CmdError::new("io_error", e.to_string()))?;
    if meta.len() > MAX_REFERENCE_BYTES {
        return Err(CmdError::new("too_large", "reference images are limited to 64 MB"));
    }
    std::fs::read(file).map_err(|e| CmdError::new("io_error", e.to_string()))
}

/// Pick an image; returns its path (the UI keeps it per project).
#[tauri::command]
pub async fn reference_pick(app: AppHandle, store: State<'_, ReferenceStore>) -> CmdResult<Option<String>> {
    let picked =
        app.dialog().file().set_title("Choose Reference Image").add_filter("Image", &EXTENSIONS).blocking_pick_file();
    let Some(path) = picked.and_then(|p| p.into_path().ok()) else { return Ok(None) };
    let path = path.canonicalize().map_err(|e| CmdError::new("io_error", e.to_string()))?;
    store.allow(&path)?;
    Ok(Some(path.display().to_string()))
}

/// The bytes of a previously picked reference image.
#[tauri::command]
pub fn reference_load(store: State<'_, ReferenceStore>, path: String) -> CmdResult<Response> {
    let file = PathBuf::from(&path);
    if !store.is_allowed(&file) {
        return Err(CmdError::new("not_allowed", "choose the reference image again"));
    }
    Ok(Response::new(read_image(&file)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_picked_paths_are_readable_and_survive_restart() {
        let dir = std::env::temp_dir().join(format!("dotify-refs-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let picked = dir.join("ref.png");
        std::fs::write(&picked, b"png").unwrap();
        let other = dir.join("secret.txt");
        std::fs::write(&other, b"no").unwrap();

        let store = ReferenceStore::new(dir.join("references.json"));
        assert!(!store.is_allowed(&picked));
        store.allow(&picked).unwrap();
        assert!(store.is_allowed(&picked));
        assert!(!store.is_allowed(&other));

        let reopened = ReferenceStore::new(dir.join("references.json"));
        assert!(reopened.is_allowed(&picked));
        assert_eq!(read_image(&picked).unwrap(), b"png");
        let _ = std::fs::remove_dir_all(dir);
    }
}
