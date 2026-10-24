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
use crate::model::{AnimationTag, Document, DocumentParts, ExportSettings, Frame, FrameId, Layer, LayerId, Palette};
use crate::png_io::{self, PngError};
use crate::MAX_CANVAS_SIZE;

/// Format version written by this build.
pub const FORMAT_VERSION: u32 = 1;
/// Oldest reader version able to open files written by this build.
pub const MIN_READER_VERSION: u32 = 1;
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

#[derive(Serialize, Deserialize)]
struct DocumentFile {
    name: String,
    width: u32,
    height: u32,
    layers: Vec<Layer>,
    frames: Vec<Frame>,
    cels: Vec<CelEntry>,
    palette: Palette,
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

/// Serialize `doc` into a `.dotify` archive.
pub fn write<W: Write + Seek>(doc: &Document, writer: W, opts: WriteOptions) -> Result<(), FormatError> {
    let mut entries: Vec<(String, Vec<u8>, CompressionMethod)> = Vec::new();
    let mut cels = Vec::new();
    for (layer, frame, img) in doc.cels() {
        let Some(rect) = img.opaque_bounds() else { continue };
        let file = format!("cels/{layer}-{frame}.png");
        entries.push((file.clone(), png_io::encode_rgba(&img.crop(rect))?, CompressionMethod::Stored));
        cels.push(CelEntry { layer, frame, x: rect.x, y: rect.y, width: rect.width, height: rect.height, file });
    }
    let file = DocumentFile {
        name: doc.name.clone(),
        width: doc.width,
        height: doc.height,
        layers: doc.layers.clone(),
        frames: doc.frames.clone(),
        cels,
        palette: doc.palette.clone(),
        tags: doc.tags.clone(),
        export: doc.export.clone(),
        next_id: doc.next_id,
        extensions: doc.extensions.clone(),
    };
    let json = serde_json::to_vec_pretty(&file).map_err(|e| corrupt(e.to_string()))?;
    entries.insert(0, ("document.json".into(), json, CompressionMethod::Deflated));
    if opts.thumbnail {
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
pub fn read<R: Read + Seek>(reader: R) -> Result<Document, FormatError> {
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
    let file: DocumentFile = serde_json::from_value(json).map_err(|e| corrupt(format!("document.json: {e}")))?;
    crate::model::validate_canvas_size(file.width, file.height)?;

    let mut cels = BTreeMap::new();
    for cel in &file.cels {
        let rect = Rect::new(cel.x, cel.y, cel.width, cel.height);
        if cel.width == 0 || cel.height == 0 || rect.right() > file.width || rect.bottom() > file.height {
            return Err(corrupt(format!("cel {} is outside the canvas", cel.file)));
        }
        let img = png_io::decode_rgba(&take(&mut verified, &cel.file)?, MAX_CANVAS_SIZE)?;
        if img.width() != cel.width || img.height() != cel.height {
            return Err(corrupt(format!("cel {} has unexpected dimensions", cel.file)));
        }
        let mut full = Image::new(file.width, file.height);
        full.write_region(rect, img.data());
        if cels.insert((cel.layer, cel.frame), full).is_some() {
            return Err(corrupt(format!("duplicate cel for layer {} frame {}", cel.layer, cel.frame)));
        }
    }

    Ok(DocumentParts {
        name: file.name,
        width: file.width,
        height: file.height,
        layers: file.layers,
        frames: file.frames,
        cels,
        palette: file.palette,
        tags: file.tags,
        export: file.export,
        next_id: file.next_id,
        extensions: file.extensions,
    }
    .build()?)
}

pub fn to_bytes(doc: &Document, opts: WriteOptions) -> Result<Vec<u8>, FormatError> {
    let mut cursor = Cursor::new(Vec::new());
    write(doc, &mut cursor, opts)?;
    Ok(cursor.into_inner())
}

pub fn from_bytes(bytes: &[u8]) -> Result<Document, FormatError> {
    read(Cursor::new(bytes))
}

/// Write `doc` to `path` atomically: serialize to a temporary sibling file,
/// fsync it and rename it over the destination.
pub fn save(doc: &Document, path: &Path, opts: WriteOptions) -> Result<(), FormatError> {
    let tmp = temp_path(path);
    let result = (|| {
        let file = File::create(&tmp)?;
        let mut w = BufWriter::new(file);
        write(doc, &mut w, opts)?;
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

pub fn load(path: &Path) -> Result<Document, FormatError> {
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
    use crate::Editor;

    fn sample() -> Document {
        let doc = Document::new("Sample", 16, 12).unwrap();
        let (l, f) = (doc.layers()[0].id, doc.frames()[0].id);
        let mut e = Editor::new(doc);
        e.execute(Command::SetPixels {
            layer: l,
            frame: f,
            pixels: vec![
                PixelWrite { x: 3, y: 4, color: Rgba::new(10, 20, 30, 40) },
                PixelWrite { x: 15, y: 11, color: Rgba::WHITE },
            ],
        })
        .unwrap();
        e.execute(Command::CreateLayer { name: Some("Empty".into()), index: None }).unwrap();
        e.execute(Command::DuplicateFrame { frame: f }).unwrap();
        e.execute(Command::UpdateLayer {
            layer: l,
            name: None,
            visible: Some(false),
            locked: Some(true),
            opacity: Some(77),
        })
        .unwrap();
        e.document().clone()
    }

    #[test]
    fn round_trip_is_lossless() {
        let doc = sample();
        let bytes = to_bytes(&doc, WriteOptions::default()).unwrap();
        let back = from_bytes(&bytes).unwrap();
        assert_eq!(back, doc);
        assert_eq!(back.next_id, doc.next_id);
    }

    #[test]
    fn cels_are_trimmed_on_disk() {
        let bytes = to_bytes(&sample(), WriteOptions { thumbnail: false }).unwrap();
        let mut zip = ZipArchive::new(Cursor::new(&bytes)).unwrap();
        let json: serde_json::Value =
            serde_json::from_slice(&read_entry(&mut zip, "document.json", MAX_JSON_BYTES).unwrap()).unwrap();
        let cel = &json["cels"][0];
        assert_eq!((cel["x"].as_u64(), cel["width"].as_u64(), cel["height"].as_u64()), (Some(3), Some(13), Some(8)));
        assert_eq!(json["cels"].as_array().unwrap().len(), 2, "empty layer stores no cels");
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
        let broken = rewrite(&bytes, |name, data| (!name.starts_with("cels/")).then_some(data));
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
