//! The palette library: built-in presets plus palettes the user imported or
//! saved, kept in `<app data>/palettes.json`. Choosing one only sets the
//! project palette (an undoable command in the UI); presets live outside
//! project files.

use std::path::PathBuf;
use std::sync::Mutex;

use dotify_core::{palette_io, Palette, Rgba};
use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use crate::commands::{CmdError, CmdResult};

#[derive(Serialize, Clone, PartialEq, Debug)]
pub struct LibraryPalette {
    #[serde(flatten)]
    pub palette: Palette,
    pub builtin: bool,
}

fn builtins() -> Vec<Palette> {
    let hex = |name: &str, colors: &[u32]| Palette {
        name: name.into(),
        colors: colors.iter().map(|h| Rgba::opaque((h >> 16) as u8, (h >> 8) as u8, *h as u8)).collect(),
    };
    vec![
        Palette::default(),
        hex("Game Boy", &[0x0f380f, 0x306230, 0x8bac0f, 0x9bbc0f]),
        hex("1-bit", &[0x000000, 0xffffff]),
    ]
}

/// User palettes in library order. Names are unique and never shadow a
/// built-in.
#[derive(Default, Debug, PartialEq)]
pub struct Library {
    user: Vec<Palette>,
}

impl Library {
    pub fn all(&self) -> Vec<LibraryPalette> {
        builtins()
            .into_iter()
            .map(|palette| LibraryPalette { palette, builtin: true })
            .chain(self.user.iter().cloned().map(|palette| LibraryPalette { palette, builtin: false }))
            .collect()
    }

    fn taken(&self, name: &str) -> bool {
        builtins().iter().chain(&self.user).any(|p| p.name.eq_ignore_ascii_case(name))
    }

    /// Add an imported palette, renaming it ("Name 2", …) if the name is taken.
    pub fn add(&mut self, mut palette: Palette) -> Palette {
        let base = palette.name.clone();
        let mut n = 2;
        while self.taken(&palette.name) {
            palette.name = format!("{base} {n}");
            n += 1;
        }
        self.user.push(palette.clone());
        palette
    }

    /// Save under the palette's name, replacing a user palette of that name.
    pub fn save(&mut self, palette: Palette) -> Result<(), CmdError> {
        if palette.name.trim().is_empty() {
            return Err(CmdError::new("invalid_argument", "a palette needs a name"));
        }
        if builtins().iter().any(|p| p.name.eq_ignore_ascii_case(&palette.name)) {
            return Err(CmdError::new(
                "builtin_palette",
                format!("“{}” is a built-in palette; pick another name", palette.name),
            ));
        }
        match self.user.iter_mut().find(|p| p.name.eq_ignore_ascii_case(&palette.name)) {
            Some(existing) => *existing = palette,
            None => self.user.push(palette),
        }
        Ok(())
    }

    pub fn delete(&mut self, name: &str) -> Result<(), CmdError> {
        let before = self.user.len();
        self.user.retain(|p| p.name != name);
        if self.user.len() == before {
            return Err(CmdError::new("palette_not_found", format!("no saved palette named “{name}”")));
        }
        Ok(())
    }
}

pub struct PaletteStore {
    path: PathBuf,
    lock: Mutex<()>,
}

impl PaletteStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path, lock: Mutex::new(()) }
    }

    fn load(&self) -> Library {
        // A missing or unreadable file means "no saved palettes yet".
        let user = std::fs::read(&self.path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        Library { user }
    }

    fn store(&self, lib: &Library) -> CmdResult<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| CmdError::new("io_error", e.to_string()))?;
        }
        let json = serde_json::to_vec_pretty(&lib.user).map_err(|e| CmdError::new("internal", e.to_string()))?;
        dotify_export::write_atomic(&self.path, &json).map_err(|e| CmdError::new("io_error", e.to_string()))
    }

    /// Load, change and save the library under one lock.
    fn update<T>(&self, f: impl FnOnce(&mut Library) -> CmdResult<T>) -> CmdResult<(T, Vec<LibraryPalette>)> {
        let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let mut lib = self.load();
        let out = f(&mut lib)?;
        self.store(&lib)?;
        Ok((out, lib.all()))
    }
}

#[tauri::command]
pub fn palette_library(store: State<'_, PaletteStore>) -> Vec<LibraryPalette> {
    store.load().all()
}

#[derive(Serialize)]
pub struct Imported {
    pub palette: Palette,
    pub library: Vec<LibraryPalette>,
}

/// Pick a palette file, add it to the library and return it.
#[tauri::command]
pub async fn palette_import(app: AppHandle, store: State<'_, PaletteStore>) -> CmdResult<Option<Imported>> {
    let picked = app
        .dialog()
        .file()
        .set_title("Import Palette")
        .add_filter("Palette", &["gpl", "hex", "pal", "txt", "png"])
        .blocking_pick_file();
    let Some(path) = picked.and_then(|p| p.into_path().ok()) else { return Ok(None) };
    let bytes = std::fs::read(&path).map_err(|e| CmdError::new("io_error", e.to_string()))?;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let palette = palette_io::parse(&name, &bytes).map_err(|e| CmdError::new(e.code(), e.to_string()))?;
    let (palette, library) = store.update(|lib| Ok(lib.add(palette)))?;
    Ok(Some(Imported { palette, library }))
}

#[tauri::command]
pub fn palette_save(store: State<'_, PaletteStore>, palette: Palette) -> CmdResult<Vec<LibraryPalette>> {
    Ok(store.update(|lib| lib.save(palette))?.1)
}

#[tauri::command]
pub fn palette_delete(store: State<'_, PaletteStore>, name: String) -> CmdResult<Vec<LibraryPalette>> {
    Ok(store.update(|lib| lib.delete(&name))?.1)
}

/// Write a palette as a GIMP `.gpl` file the user picks.
#[tauri::command]
pub async fn palette_export(app: AppHandle, palette: Palette) -> CmdResult<Option<String>> {
    let picked = app
        .dialog()
        .file()
        .set_title("Export Palette")
        .set_file_name(format!("{}.gpl", palette.name))
        .add_filter("GIMP Palette", &["gpl"])
        .blocking_save_file();
    let Some(mut path) = picked.and_then(|p| p.into_path().ok()) else { return Ok(None) };
    if path.extension().is_none_or(|e| !e.eq_ignore_ascii_case("gpl")) {
        path.set_extension("gpl");
    }
    dotify_export::write_atomic(&path, palette_io::to_gpl(&palette).as_bytes())
        .map_err(|e| CmdError::new("io_error", e.to_string()))?;
    Ok(Some(path.display().to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pal(name: &str) -> Palette {
        Palette { name: name.into(), colors: vec![Rgba::WHITE] }
    }

    #[test]
    fn builtins_come_first_and_are_protected() {
        let mut lib = Library::default();
        assert!(lib.all().iter().take(3).all(|p| p.builtin));
        assert!(lib.save(pal("pico-8")).is_err());
        assert!(lib.delete("PICO-8").is_err());
    }

    #[test]
    fn imports_get_unique_names() {
        let mut lib = Library::default();
        assert_eq!(lib.add(pal("Dusk")).name, "Dusk");
        assert_eq!(lib.add(pal("Dusk")).name, "Dusk 2");
        assert_eq!(lib.add(pal("PICO-8")).name, "PICO-8 2");
    }

    #[test]
    fn save_replaces_by_name_and_delete_removes() {
        let mut lib = Library::default();
        lib.save(pal("Dusk")).unwrap();
        let mut changed = pal("dusk");
        changed.colors.push(Rgba::BLACK);
        lib.save(changed).unwrap();
        assert_eq!(lib.user.len(), 1);
        assert_eq!(lib.user[0].colors.len(), 2);
        lib.delete("dusk").unwrap();
        assert!(lib.user.is_empty());
    }

    #[test]
    fn store_round_trips_through_disk() {
        let dir = std::env::temp_dir().join(format!("dotify-palettes-{}", std::process::id()));
        let store = PaletteStore::new(dir.join("palettes.json"));
        assert!(store.load().user.is_empty());
        store.update(|lib| lib.save(pal("Dusk"))).unwrap();
        assert_eq!(store.load().user, vec![pal("Dusk")]);
        let _ = std::fs::remove_dir_all(dir);
    }
}
