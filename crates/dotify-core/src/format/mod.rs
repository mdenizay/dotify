//! Reading and writing `.dotify` project files. See `docs/FORMAT.md`.

mod migrate;

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufWriter, Cursor, Read, Seek, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::composite::composite_frame;
use crate::geometry::Rect;
use crate::image::Image;
use crate::model::{AnimationTag, DocumentParts, ExportSettings, Frame, FrameId, Layer, LayerId, Palette};
use crate::png_io::{self, PngError};
use crate::project::{ArtboardId, Folder, FolderId, Project, ProjectParts};
use crate::MAX_CANVAS_SIZE;

/// Format version written by this build.
pub const FORMAT_VERSION: u32 = 3;
/// Oldest reader version able to open files written by this build.
pub const MIN_READER_VERSION: u32 = 3;
pub const MIMETYPE: &str = "application/x-dotify";
pub const FILE_EXTENSION: &str = "dotify";

const MAX_JSON_BYTES: u64 = 64 * 1024 * 1024;
const MAX_ENTRY_BYTES: u64 = 256 * 1024 * 1024;
const MAX_ENTRIES: usize = 100_000;

#[derive(Debug, thiserror::Error)]
pub enum FormatError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("not a Dotify project: {0}")]
    NotDotify(String),
    #[error("this file requires Dotify format version {required}, but this build supports up to {supported}")]
    UnsupportedVersion { required: u32, supported: u32 },
    #[error("the project file is corrupt: {0}")]
    Corrupt(String),
    #[error(transparent)]
    Png(#[from] PngError),
    #[error("invalid project data: {0}")]
    Invalid(#[from] crate::CoreError),
}

impl FormatError {
    pub fn code(&self) -> &'static str {
        match self {
            FormatError::Io(_) => "io_error",
            FormatError::NotDotify(_) => "not_dotify",
            FormatError::UnsupportedVersion { .. } => "unsupported_version",
            FormatError::Corrupt(_) | FormatError::Png(_) => "corrupt_file",
            FormatError::Invalid(_) => "invalid_project",
        }
    }
}

impl From<zip::result::ZipError> for FormatError {
    fn from(e: zip::result::ZipError) -> Self {
        match e {
            zip::result::ZipError::Io(io) => FormatError::Io(io),
            other => FormatError::Corrupt(other.to_string()),
        }
    }
}

fn corrupt(msg: impl Into<String>) -> FormatError {
    FormatError::Corrupt(msg.into())
}

#[derive(Serialize, Deserialize)]
struct Manifest {
    format: String,
    format_version: u32,
    min_reader_version: u32,
    #[serde(default)]
    generator: String,
    entries: BTreeMap<String, String>,
}

#[derive(Serialize, Deserialize)]
struct CelEntry {
    layer: LayerId,
    frame: FrameId,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    file: String,
}

/// `document.json`: the project.
#[derive(Serialize, Deserialize)]
struct ProjectFile {
    name: String,
    palette: Palette,
    #[serde(default)]
    folders: Vec<Folder>,
    artboards: Vec<ArtboardFile>,
    #[serde(default)]
    next_id: u32,
    #[serde(default)]
    extensions: serde_json::Map<String, serde_json::Value>,
}

/// One artboard: everything a v1 document stored, minus the palette.
#[derive(Serialize, Deserialize)]
struct ArtboardFile {
    id: ArtboardId,
    #[serde(default)]
    folder: Option<FolderId>,
    name: String,
    width: u32,
    height: u32,
    layers: Vec<Layer>,
    frames: Vec<Frame>,
    cels: Vec<CelEntry>,
    #[serde(default)]
    tags: Vec<AnimationTag>,
    #[serde(default)]
    export: ExportSettings,
    #[serde(default)]
    next_id: u32,
    #[serde(default)]
    extensions: serde_json::Map<String, serde_json::Value>,
}

fn sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut s = String::with_capacity(7 + 64);
    s.push_str("sha256:");
    for b in digest {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

#[derive(Clone, Copy, Debug)]
pub struct WriteOptions {
    pub thumbnail: bool,
}

impl Default for WriteOptions {
    fn default() -> Self {
        Self { thumbnail: true }
    }
}

/// Serialize `project` into a `.dotify` archive.
pub fn write<W: Write + Seek>(project: &Project, writer: W, opts: WriteOptions) -> Result<(), FormatError> {
    let mut entries: Vec<(String, Vec<u8>, CompressionMethod)> = Vec::new();
    let mut artboards = Vec::new();
    for artboard in project.artboards() {
        let doc = artboard.document();
        let mut cels = Vec::new();
        for (layer, frame, img) in doc.cels() {
            let Some(rect) = img.opaque_bounds() else { continue };
            let file = format!("artboards/{}/cels/{layer}-{frame}.png", artboard.id());
            entries.push((file.clone(), png_io::encode_rgba(&img.crop(rect))?, CompressionMethod::Stored));
            cels.push(CelEntry { layer, frame, x: rect.x, y: rect.y, width: rect.width, height: rect.height, file });
        }
        artboards.push(ArtboardFile {
            id: artboard.id(),
            folder: artboard.folder(),
            name: doc.name.clone(),
            width: doc.width,
            height: doc.height,
            layers: doc.layers.clone(),
            frames: doc.frames.clone(),
            cels,
            tags: doc.tags.clone(),
            export: doc.export.clone(),
            next_id: doc.next_id,
            extensions: doc.extensions.clone(),
        });
    }
    let file = ProjectFile {
        name: project.name.clone(),
        palette: project.palette.clone(),
        folders: project.folders.clone(),
        artboards,
        next_id: project.next_id,
        extensions: project.extensions.clone(),
    };
    let json = serde_json::to_vec_pretty(&file).map_err(|e| corrupt(e.to_string()))?;
    entries.insert(0, ("document.json".into(), json, CompressionMethod::Deflated));
    if opts.thumbnail {
        let doc = project.artboards()[0].document();
        let thumb = composite_frame(doc, doc.frames[0].id)?;
        entries.push(("thumbnail.png".into(), png_io::encode_rgba(&thumb)?, CompressionMethod::Stored));
    }

    let manifest = Manifest {
        format: "dotify".into(),
        format_version: FORMAT_VERSION,
        min_reader_version: MIN_READER_VERSION,
        generator: format!("dotify {}", env!("CARGO_PKG_VERSION")),
        entries: entries.iter().map(|(name, bytes, _)| (name.clone(), sha256(bytes))).collect(),
    };
    let manifest = serde_json::to_vec_pretty(&manifest).map_err(|e| corrupt(e.to_string()))?;

    let mut zip = ZipWriter::new(writer);
    let opts = |m: CompressionMethod| SimpleFileOptions::default().compression_method(m).unix_permissions(0o644);
    zip.start_file("mimetype", opts(CompressionMethod::Stored))?;
    zip.write_all(MIMETYPE.as_bytes())?;
    zip.start_file("manifest.json", opts(CompressionMethod::Deflated))?;
    zip.write_all(&manifest)?;
    for (name, bytes, method) in entries {
        zip.start_file(name, opts(method))?;
        zip.write_all(&bytes)?;
    }
    zip.finish()?;
    Ok(())
}

fn read_entry<R: Read + Seek>(zip: &mut ZipArchive<R>, name: &str, limit: u64) -> Result<Vec<u8>, FormatError> {
    let entry = zip.by_name(name).map_err(|_| corrupt(format!("missing entry {name}")))?;
    if entry.size() > limit {
        return Err(corrupt(format!("entry {name} is too large")));
    }
    let mut buf = Vec::with_capacity(entry.size() as usize);
    entry.take(limit + 1).read_to_end(&mut buf)?;
    if buf.len() as u64 > limit {
        return Err(corrupt(format!("entry {name} is too large")));
    }
    Ok(buf)
}

/// Parse a `.dotify` archive.
pub fn read<R: Read + Seek>(reader: R) -> Result<Project, FormatError> {
    let mut zip = ZipArchive::new(reader).map_err(|e| FormatError::NotDotify(e.to_string()))?;
    if zip.len() > MAX_ENTRIES {
        return Err(corrupt("too many entries"));
    }
    {
        let first = zip.by_index(0).map_err(|e| FormatError::NotDotify(e.to_string()))?;
        if first.name().ok().as_deref() != Some("mimetype") {
            return Err(FormatError::NotDotify("missing mimetype entry".into()));
        }
    }
    if read_entry(&mut zip, "mimetype", 256)? != MIMETYPE.as_bytes() {
        return Err(FormatError::NotDotify("unexpected mimetype".into()));
    }
    let manifest: Manifest = serde_json::from_slice(&read_entry(&mut zip, "manifest.json", MAX_JSON_BYTES)?)
        .map_err(|e| corrupt(format!("manifest.json: {e}")))?;
    if manifest.format != "dotify" {
        return Err(FormatError::NotDotify(format!("unknown format {:?}", manifest.format)));
    }
    if manifest.min_reader_version > FORMAT_VERSION {
        return Err(FormatError::UnsupportedVersion {
            required: manifest.min_reader_version,
            supported: FORMAT_VERSION,
        });
    }

    let mut verified: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for (name, hash) in &manifest.entries {
        let limit = if name.ends_with(".json") { MAX_JSON_BYTES } else { MAX_ENTRY_BYTES };
        let bytes = read_entry(&mut zip, name, limit)?;
        if &sha256(&bytes) != hash {
            return Err(corrupt(format!("checksum mismatch for {name}")));
        }
        verified.insert(name.clone(), bytes);
    }
    let take = |verified: &mut BTreeMap<String, Vec<u8>>, name: &str| {
        verified.remove(name).ok_or_else(|| corrupt(format!("{name} is not listed in the manifest")))
    };

    let json: serde_json::Value = serde_json::from_slice(&take(&mut verified, "document.json")?)
        .map_err(|e| corrupt(format!("document.json: {e}")))?;
    let json = migrate::migrate(json, manifest.format_version.min(FORMAT_VERSION))?;
    let file: ProjectFile = serde_json::from_value(json).map_err(|e| corrupt(format!("document.json: {e}")))?;

    let mut artboards = Vec::with_capacity(file.artboards.len());
    for artboard in file.artboards {
        crate::model::validate_canvas_size(artboard.width, artboard.height)?;
        let mut cels = BTreeMap::new();
        for cel in &artboard.cels {
            let rect = Rect::new(cel.x, cel.y, cel.width, cel.height);
            if cel.width == 0 || cel.height == 0 || rect.right() > artboard.width || rect.bottom() > artboard.height {
                return Err(corrupt(format!("cel {} is outside the canvas", cel.file)));
            }
            let img = png_io::decode_rgba(&take(&mut verified, &cel.file)?, MAX_CANVAS_SIZE)?;
            if img.width() != cel.width || img.height() != cel.height {
                return Err(corrupt(format!("cel {} has unexpected dimensions", cel.file)));
            }
            let mut full = Image::new(artboard.width, artboard.height);
            full.write_region(rect, img.data());
            if cels.insert((cel.layer, cel.frame), full).is_some() {
                return Err(corrupt(format!("duplicate cel for layer {} frame {}", cel.layer, cel.frame)));
            }
        }
        let document = DocumentParts {
            name: artboard.name,
            width: artboard.width,
            height: artboard.height,
            layers: artboard.layers,
            frames: artboard.frames,
            cels,
            palette: file.palette.clone(),
            tags: artboard.tags,
            export: artboard.export,
            next_id: artboard.next_id,
            extensions: artboard.extensions,
        }
        .build()?;
        artboards.push((artboard.id, artboard.folder, document));
    }

    Ok(ProjectParts {
        name: file.name,
        palette: file.palette,
        folders: file.folders,
        artboards,
        next_id: file.next_id,
        extensions: file.extensions,
    }
    .build()?)
}

pub fn to_bytes(project: &Project, opts: WriteOptions) -> Result<Vec<u8>, FormatError> {
    let mut cursor = Cursor::new(Vec::new());
    write(project, &mut cursor, opts)?;
    Ok(cursor.into_inner())
}

pub fn from_bytes(bytes: &[u8]) -> Result<Project, FormatError> {
    read(Cursor::new(bytes))
}

/// Write `project` to `path` atomically: serialize to a temporary sibling file,
/// fsync it and rename it over the destination.
pub fn save(project: &Project, path: &Path, opts: WriteOptions) -> Result<(), FormatError> {
    let tmp = temp_path(path);
    let result = (|| {
        let file = File::create(&tmp)?;
        let mut w = BufWriter::new(file);
        write(project, &mut w, opts)?;
        let file = w.into_inner().map_err(|e| e.into_error())?;
        file.sync_all()?;
        std::fs::rename(&tmp, path)?;
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            // Persist the rename itself; not supported on every platform.
            if let Ok(d) = File::open(dir) {
                let _ = d.sync_all();
            }
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

pub fn load(path: &Path) -> Result<Project, FormatError> {
    read(std::io::BufReader::new(File::open(path)?))
}

fn temp_path(path: &Path) -> PathBuf {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    path.with_file_name(format!(".{name}.tmp-{}", std::process::id()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Rgba;
    use crate::command::{Command, PixelWrite};
    use crate::project::{ProjectCommand, ProjectCreated, ProjectEditor};

    /// Two artboards (one in a folder), a shared palette, layers, frames.
    fn sample() -> Project {
        let project = Project::new("Sample", 16, 12).unwrap();
        let a = project.artboards()[0].id();
        let doc = project.document(a).unwrap();
        let (l, f) = (doc.layers()[0].id, doc.frames()[0].id);
        let mut e = ProjectEditor::new(project);
        let edit = |command| ProjectCommand::Edit { artboard: a, command };
        e.execute(edit(Command::SetPixels {
            layer: l,
            frame: f,
            pixels: vec![
                PixelWrite { x: 3, y: 4, color: Rgba::new(10, 20, 30, 40) },
                PixelWrite { x: 15, y: 11, color: Rgba::WHITE },
            ],
        }))
        .unwrap();
        e.execute(edit(Command::CreateLayer { name: Some("Empty".into()), index: None })).unwrap();
        e.execute(edit(Command::DuplicateFrame { frame: f })).unwrap();
        e.execute(edit(Command::CreateTag {
            name: "Blink".into(),
            from: f,
            to: f,
            direction: Some(crate::model::PlaybackDirection::PingPong),
            looping: Some(false),
            color: None,
        }))
        .unwrap();
        e.execute(edit(Command::UpdateLayer {
            layer: l,
            name: None,
            visible: Some(false),
            locked: Some(true),
            opacity: Some(77),
        }))
        .unwrap();
        let out = e.execute(ProjectCommand::CreateFolder { name: Some("Tiles".into()), index: None }).unwrap();
        let ProjectCreated::Folder(folder) = out.created[0] else { panic!() };
        let out = e
            .execute(ProjectCommand::CreateArtboard {
                name: Some("Grass".into()),
                width: 8,
                height: 8,
                folder: Some(folder),
                index: None,
            })
            .unwrap();
        let ProjectCreated::Artboard(grass) = out.created[0] else { panic!() };
        let gdoc = e.project().document(grass).unwrap();
        let (gl, gf) = (gdoc.layers()[0].id, gdoc.frames()[0].id);
        e.execute(ProjectCommand::Edit {
            artboard: grass,
            command: Command::FillRegion {
                layer: gl,
                frame: gf,
                x: 0,
                y: 0,
                color: Rgba::opaque(0, 200, 0),
                contiguous: true,
            },
        })
        .unwrap();
        e.execute(ProjectCommand::SetPalette {
            palette: Palette { name: "Game".into(), colors: vec![Rgba::BLACK, Rgba::opaque(0, 200, 0)] },
        })
        .unwrap();
        e.project().clone()
    }

    #[test]
    fn round_trip_is_lossless() {
        let project = sample();
        let bytes = to_bytes(&project, WriteOptions::default()).unwrap();
        let back = from_bytes(&bytes).unwrap();
        assert_eq!(back, project);
        assert_eq!(back.next_id, project.next_id);
        assert_eq!(back.artboards()[1].document().palette(), project.palette());
    }

    #[test]
    fn opens_v1_fixture_as_single_artboard_project() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/v1-hero.dotify");
        let project = load(&path).unwrap();
        assert_eq!(project.name(), "Hero");
        assert_eq!(project.palette().name, "Mini");
        assert!(project.folders().is_empty());
        let [artboard] = project.artboards() else { panic!("expected one artboard") };
        assert_eq!((artboard.id(), artboard.folder(), artboard.name()), (ArtboardId(1), None, "Hero"));
        let doc = artboard.document();
        assert_eq!((doc.width(), doc.height(), doc.palette()), (8, 6, project.palette()));
        assert_eq!(
            doc.layers().iter().map(|l| (l.id.0, l.name.as_str())).collect::<Vec<_>>(),
            [(1, "Layer 1"), (3, "Ink")]
        );
        assert_eq!(doc.frames().iter().map(|f| (f.id.0, f.duration_ms)).collect::<Vec<_>>(), [(2, 100), (4, 150)]);
        assert_eq!(doc.cel_image(LayerId(1), FrameId(4)).get(2, 2), Rgba::opaque(255, 0, 0));
        assert_eq!(doc.cel_image(LayerId(1), FrameId(4)).get(0, 0), Rgba::TRANSPARENT);
        assert_eq!(doc.cel_image(LayerId(3), FrameId(2)).get(7, 5), Rgba::new(0, 255, 0, 128));
        assert_eq!(doc.next_id, 5);
        // Re-saving writes v2 and stays lossless.
        let back = from_bytes(&to_bytes(&project, WriteOptions::default()).unwrap()).unwrap();
        assert_eq!(back, project);
    }

    #[test]
    fn cels_are_trimmed_on_disk() {
        let bytes = to_bytes(&sample(), WriteOptions { thumbnail: false }).unwrap();
        let mut zip = ZipArchive::new(Cursor::new(&bytes)).unwrap();
        let json: serde_json::Value =
            serde_json::from_slice(&read_entry(&mut zip, "document.json", MAX_JSON_BYTES).unwrap()).unwrap();
        let cel = &json["artboards"][0]["cels"][0];
        assert_eq!(cel["file"], "artboards/1/cels/1-2.png");
        assert_eq!((cel["x"].as_u64(), cel["width"].as_u64(), cel["height"].as_u64()), (Some(3), Some(13), Some(8)));
        assert_eq!(json["artboards"][0]["cels"].as_array().unwrap().len(), 2, "empty layer stores no cels");
        assert!(json["artboards"][0].get("palette").is_none(), "the palette is stored once per project");
    }

    #[test]
    fn mimetype_is_first_and_stored() {
        let bytes = to_bytes(&sample(), WriteOptions::default()).unwrap();
        let mut zip = ZipArchive::new(Cursor::new(&bytes)).unwrap();
        let first = zip.by_index(0).unwrap();
        assert_eq!(first.name().unwrap(), "mimetype");
        assert_eq!(first.compression(), CompressionMethod::Stored);
    }

    /// Rebuild an archive, transforming one entry.
    fn rewrite(bytes: &[u8], f: impl Fn(&str, Vec<u8>) -> Option<Vec<u8>>) -> Vec<u8> {
        let mut zip = ZipArchive::new(Cursor::new(bytes)).unwrap();
        let mut out = ZipWriter::new(Cursor::new(Vec::new()));
        for i in 0..zip.len() {
            let mut e = zip.by_index(i).unwrap();
            let name = e.name().unwrap().to_string();
            let mut data = Vec::new();
            e.read_to_end(&mut data).unwrap();
            if let Some(data) = f(&name, data) {
                out.start_file(name, SimpleFileOptions::default()).unwrap();
                out.write_all(&data).unwrap();
            }
        }
        out.finish().unwrap().into_inner()
    }

    #[test]
    fn detects_tampered_entries() {
        let bytes = to_bytes(&sample(), WriteOptions::default()).unwrap();
        let tampered = rewrite(&bytes, |name, mut data| {
            if name == "document.json" {
                data.extend_from_slice(b" ");
            }
            Some(data)
        });
        assert!(matches!(from_bytes(&tampered), Err(FormatError::Corrupt(m)) if m.contains("checksum")));
    }

    #[test]
    fn detects_missing_cel() {
        let bytes = to_bytes(&sample(), WriteOptions::default()).unwrap();
        let broken = rewrite(&bytes, |name, data| (!name.starts_with("artboards/1/cels/")).then_some(data));
        assert!(matches!(from_bytes(&broken), Err(FormatError::Corrupt(_))));
    }

    #[test]
    fn rejects_files_from_the_future() {
        let bytes = to_bytes(&sample(), WriteOptions::default()).unwrap();
        let future = rewrite(&bytes, |name, data| {
            if name != "manifest.json" {
                return Some(data);
            }
            let mut m: serde_json::Value = serde_json::from_slice(&data).unwrap();
            m["format_version"] = 7.into();
            m["min_reader_version"] = 7.into();
            Some(serde_json::to_vec(&m).unwrap())
        });
        assert!(matches!(from_bytes(&future), Err(FormatError::UnsupportedVersion { required: 7, .. })));
    }

    #[test]
    fn newer_but_compatible_files_open() {
        let bytes = to_bytes(&sample(), WriteOptions::default()).unwrap();
        let newer = rewrite(&bytes, |name, data| {
            if name != "manifest.json" {
                return Some(data);
            }
            let mut m: serde_json::Value = serde_json::from_slice(&data).unwrap();
            m["format_version"] = (FORMAT_VERSION + 1).into();
            Some(serde_json::to_vec(&m).unwrap())
        });
        assert_eq!(from_bytes(&newer).unwrap(), sample());
    }

    #[test]
    fn opens_v2_fixture_and_keeps_tag_looping() {
        // Written by the format-v2 CLI (tags with `loop`, single document).
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/v2-walk.dotify");
        let project = load(&path).unwrap();
        let [artboard] = project.artboards() else { panic!("expected one artboard") };
        let doc = artboard.document();
        assert_eq!((project.name(), artboard.name(), doc.width(), doc.height()), ("Walker", "Walker", 6, 4));
        assert_eq!(project.palette().name, "Two");
        let tags: Vec<_> = doc.tags().iter().map(|t| (t.name.as_str(), t.from.0, t.to.0, t.looping)).collect();
        assert_eq!(tags, [("Walk", 2, 3, true), ("Die", 4, 4, false)]);
        assert_eq!(doc.cel_image(LayerId(1), FrameId(3)).get(5, 3), Rgba::opaque(0, 0xaa, 0xff));
        let back = from_bytes(&to_bytes(&project, WriteOptions::default()).unwrap()).unwrap();
        assert_eq!(back, project);
        assert!(!back.artboards()[0].document().tags()[1].looping);
    }

    #[test]
    fn rejects_non_dotify_input() {
        assert!(matches!(from_bytes(b"not a zip"), Err(FormatError::NotDotify(_))));
    }

    #[test]
    fn atomic_save_and_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hero.dotify");
        let doc = sample();
        save(&doc, &path, WriteOptions::default()).unwrap();
        save(&doc, &path, WriteOptions::default()).unwrap();
        assert_eq!(load(&path).unwrap(), doc);
        let leftovers: Vec<_> = std::fs::read_dir(dir.path()).unwrap().collect();
        assert_eq!(leftovers.len(), 1, "temporary files must be cleaned up");
    }
}
