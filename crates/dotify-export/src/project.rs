//! Export every artboard of a project in one go.
//!
//! [`export_project`] decides where each artboard goes (`<folder>/<name>`,
//! with file-system-safe, de-duplicated names) and hands each one to an
//! [`ArtboardExporter`]. PNG is implemented here; other formats (e.g. Godot
//! `SpriteFrames`) implement the same trait to get batch export for free.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use dotify_core::{Artboard, ArtboardId, Project};

use crate::{export_png, ExportError, PngExportOptions};

/// Writes the files for one artboard.
pub trait ArtboardExporter {
    /// Write `artboard` into `dir` (which exists), naming files after
    /// `stem`. Returns the paths written.
    fn export_artboard(&self, artboard: &Artboard, dir: &Path, stem: &str) -> Result<Vec<PathBuf>, ExportError>;
}

/// The first frame of each artboard as `<stem>.png`.
#[derive(Clone, Copy, Debug)]
pub struct PngArtboardExporter {
    pub scale: u32,
}

impl ArtboardExporter for PngArtboardExporter {
    fn export_artboard(&self, artboard: &Artboard, dir: &Path, stem: &str) -> Result<Vec<PathBuf>, ExportError> {
        let doc = artboard.document();
        let path = dir.join(format!("{stem}.png"));
        export_png(doc, PngExportOptions { frame: doc.frames()[0].id, scale: self.scale }, &path)?;
        Ok(vec![path])
    }
}

/// Where one artboard is exported, relative to the export root.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ArtboardTarget {
    pub artboard: ArtboardId,
    /// Sub-directory for the artboard's folder; empty for ungrouped ones.
    pub dir: PathBuf,
    pub stem: String,
}

/// Plan the export layout in library order. Names are unique per directory,
/// ignoring case, so they are safe on case-insensitive file systems.
pub fn plan(project: &Project) -> Vec<ArtboardTarget> {
    let mut dirs = BTreeSet::new();
    let folder_dirs: Vec<_> = project.folders().iter().map(|f| (f.id, unique(&mut dirs, &sanitize(&f.name)))).collect();
    let mut taken: Vec<(PathBuf, BTreeSet<String>)> = Vec::new();
    project
        .artboards()
        .iter()
        .map(|a| {
            let dir = a
                .folder()
                .and_then(|id| folder_dirs.iter().find(|(f, _)| *f == id))
                .map(|(_, d)| PathBuf::from(d))
                .unwrap_or_default();
            let names = match taken.iter().position(|(d, _)| *d == dir) {
                Some(i) => &mut taken[i].1,
                None => {
                    taken.push((dir.clone(), BTreeSet::new()));
                    &mut taken.last_mut().expect("just pushed").1
                }
            };
            let stem = unique(names, &sanitize(a.name()));
            ArtboardTarget { artboard: a.id(), dir, stem }
        })
        .collect()
}

/// Export every artboard under `root`. Returns all paths written.
pub fn export_project(
    project: &Project,
    root: &Path,
    exporter: &dyn ArtboardExporter,
) -> Result<Vec<PathBuf>, ExportError> {
    let mut written = Vec::new();
    for target in plan(project) {
        let dir = root.join(&target.dir);
        std::fs::create_dir_all(&dir)?;
        written.extend(exporter.export_artboard(project.artboard(target.artboard)?, &dir, &target.stem)?);
    }
    Ok(written)
}

/// A file name component safe on macOS, Windows and Linux.
fn sanitize(name: &str) -> String {
    let cleaned: String =
        name.chars()
            .map(|c| {
                if c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                    '_'
                } else {
                    c
                }
            })
            .collect();
    let cleaned = cleaned.trim().trim_matches('.').trim();
    if cleaned.is_empty() {
        "artboard".into()
    } else {
        cleaned.into()
    }
}

/// `name`, or `name-2`, `name-3`… whichever is free; records the result.
fn unique(taken: &mut BTreeSet<String>, name: &str) -> String {
    let candidate = std::iter::once(name.to_string())
        .chain((2..).map(|n| format!("{name}-{n}")))
        .find(|c| !taken.contains(&c.to_lowercase()));
    let candidate = candidate.expect("unbounded iterator");
    taken.insert(candidate.to_lowercase());
    candidate
}

#[cfg(test)]
mod tests {
    use super::*;
    use dotify_core::png_io;
    use dotify_core::project::ProjectCreated;
    use dotify_core::{Command, PixelWrite, ProjectCommand, ProjectEditor, Rgba};

    fn project() -> Project {
        let mut e = ProjectEditor::new(Project::new("Hero", 4, 4).unwrap());
        let ProjectCreated::Folder(tiles) =
            e.execute(ProjectCommand::CreateFolder { name: Some("Tiles/Ground".into()), index: None }).unwrap().created
                [0]
        else {
            panic!()
        };
        for (name, folder) in [("grass", Some(tiles)), ("Grass", Some(tiles)), ("hero", None), ("..", None)] {
            e.execute(ProjectCommand::CreateArtboard {
                name: Some(name.into()),
                width: 2,
                height: 3,
                folder,
                index: None,
            })
            .unwrap();
        }
        let hero = e.project().artboards()[0].id();
        let doc = e.project().document(hero).unwrap();
        let (layer, frame) = (doc.layers()[0].id, doc.frames()[0].id);
        e.execute(ProjectCommand::Edit {
            artboard: hero,
            command: Command::SetPixels { layer, frame, pixels: vec![PixelWrite { x: 1, y: 1, color: Rgba::WHITE }] },
        })
        .unwrap();
        e.project().clone()
    }

    #[test]
    fn plan_sanitizes_and_deduplicates_names() {
        let plan = plan(&project());
        let got: Vec<_> = plan.iter().map(|t| (t.dir.to_string_lossy().into_owned(), t.stem.as_str())).collect();
        assert_eq!(
            got,
            [
                ("".into(), "Hero"),
                ("Tiles_Ground".into(), "grass"),
                ("Tiles_Ground".into(), "Grass-2"),
                ("".into(), "hero-2"),
                ("".into(), "artboard"),
            ]
        );
    }

    #[test]
    fn exports_one_png_per_artboard() {
        let project = project();
        let dir = tempfile::tempdir().unwrap();
        let written = export_project(&project, dir.path(), &PngArtboardExporter { scale: 2 }).unwrap();
        assert_eq!(written.len(), 5);
        let hero = png_io::decode_rgba(&std::fs::read(dir.path().join("Hero.png")).unwrap(), 64).unwrap();
        assert_eq!((hero.width(), hero.height()), (8, 8));
        assert_eq!(hero.get(3, 3), Rgba::WHITE);
        let grass =
            png_io::decode_rgba(&std::fs::read(dir.path().join("Tiles_Ground/Grass-2.png")).unwrap(), 64).unwrap();
        assert_eq!((grass.width(), grass.height()), (4, 6));
    }
}
