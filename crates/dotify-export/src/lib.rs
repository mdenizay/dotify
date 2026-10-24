//! Exporters for Dotify documents. All scaling is integer nearest-neighbour
//! and transparent pixels are preserved exactly.

pub mod gif;
pub mod godot;
pub mod meta;
pub mod project;
pub mod sheet;

use std::path::Path;

use dotify_core::composite::composite_frame;
use dotify_core::png_io::{self, PngError};
use dotify_core::{CoreError, Document, FrameId, Image};

pub const MAX_SCALE: u32 = 64;
/// Largest exported image side, in pixels.
pub const MAX_SHEET_SIDE: u32 = 16_384;

#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error(transparent)]
    Core(#[from] CoreError),
    #[error(transparent)]
    Png(#[from] PngError),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("scale must be between 1 and {MAX_SCALE}")]
    InvalidScale,
    #[error("exported image would be {0} pixels wide/high, larger than the supported maximum")]
    TooLarge(u64),
    #[error("GIF frames are limited to 255 colors plus transparency; this frame has {0}")]
    TooManyColors(usize),
    #[error("GIF encode failed: {0}")]
    Gif(#[from] ::gif::EncodingError),
    #[error("invalid argument: {0}")]
    InvalidArgument(String),
}

impl ExportError {
    pub fn code(&self) -> &'static str {
        match self {
            ExportError::Core(e) => e.code(),
            ExportError::Png(_) => "encode_failed",
            ExportError::Io(_) => "io_error",
            ExportError::InvalidScale => "invalid_scale",
            ExportError::TooLarge(_) => "too_large",
            ExportError::TooManyColors(_) => "too_many_colors",
            ExportError::Gif(_) => "encode_failed",
            ExportError::InvalidArgument(_) => "invalid_argument",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct PngExportOptions {
    pub frame: FrameId,
    /// Integer nearest-neighbour upscale factor (1 = actual size).
    pub scale: u32,
}

/// Composite one frame and scale it.
pub fn render_frame(doc: &Document, frame: FrameId, scale: u32) -> Result<Image, ExportError> {
    if scale == 0 || scale > MAX_SCALE {
        return Err(ExportError::InvalidScale);
    }
    let side = doc.width().max(doc.height()) as u64 * scale as u64;
    if side > MAX_SHEET_SIDE as u64 {
        return Err(ExportError::TooLarge(side));
    }
    Ok(composite_frame(doc, frame)?.scaled(scale))
}

pub fn png_bytes(doc: &Document, opts: PngExportOptions) -> Result<Vec<u8>, ExportError> {
    Ok(png_io::encode_rgba(&render_frame(doc, opts.frame, opts.scale)?)?)
}

/// Write a PNG atomically (temporary sibling file + rename).
pub fn export_png(doc: &Document, opts: PngExportOptions, path: &Path) -> Result<(), ExportError> {
    let bytes = png_bytes(doc, opts)?;
    write_atomic(path, &bytes)?;
    Ok(())
}

/// Write a spritesheet PNG to `png_path` and its JSON metadata next to it
/// (same name, `.json`). Returns the metadata path.
pub fn export_spritesheet(
    doc: &Document,
    opts: &sheet::SheetOptions,
    pivot: Option<meta::Pivot>,
    png_path: &Path,
) -> Result<std::path::PathBuf, ExportError> {
    let sheet = sheet::pack(doc, opts)?;
    let image = png_path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let json = meta::metadata_json(doc, &sheet, &image, pivot.unwrap_or_else(|| meta::Pivot::center(doc)));
    let json_path = png_path.with_extension("json");
    write_atomic(png_path, &png_io::encode_rgba(&sheet.image)?)?;
    write_atomic(&json_path, json.as_bytes())?;
    Ok(json_path)
}

pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = path.with_file_name(format!(".{name}.tmp-{}", std::process::id()));
    let result = (|| {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        std::fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use dotify_core::{Command, Created, Editor, PixelWrite, PlaybackDirection, Rgba};

    /// 4×3, four frames (100, 100, 200, 50 ms), one marked pixel each.
    /// Tags: "Walk" over frames 0–2 (ping-pong, looping), "Hit" on frame 3
    /// (forward, once).
    pub(crate) fn animated_doc() -> Document {
        let doc = Document::new("Hero", 4, 3).unwrap();
        let (l, f0) = (doc.layers()[0].id, doc.frames()[0].id);
        let mut e = Editor::new(doc);
        let mut frames = vec![f0];
        for ms in [100, 200, 50] {
            let out = e.execute(Command::AddFrame { index: None, duration_ms: Some(ms) }).unwrap();
            let Created::Frame(f) = out.created[0] else { panic!() };
            frames.push(f);
        }
        let marks = [
            (0, 0, Rgba::opaque(255, 0, 0)),
            (1, 1, Rgba::opaque(0, 255, 0)),
            (2, 2, Rgba::new(0, 0, 255, 100)),
            (3, 2, Rgba::WHITE),
        ];
        for (f, (x, y, color)) in frames.iter().zip(marks) {
            e.execute(Command::SetPixels { layer: l, frame: *f, pixels: vec![PixelWrite { x, y, color }] }).unwrap();
        }
        e.execute(Command::CreateTag {
            name: "Walk".into(),
            from: frames[0],
            to: frames[2],
            direction: Some(PlaybackDirection::PingPong),
            looping: None,
            color: None,
        })
        .unwrap();
        e.execute(Command::CreateTag {
            name: "Hit".into(),
            from: frames[3],
            to: frames[3],
            direction: None,
            looping: Some(false),
            color: None,
        })
        .unwrap();
        e.document().clone()
    }

    fn doc() -> Document {
        let doc = Document::new("t", 4, 3).unwrap();
        let (l, f) = (doc.layers()[0].id, doc.frames()[0].id);
        let mut e = Editor::new(doc);
        e.execute(Command::SetPixels {
            layer: l,
            frame: f,
            pixels: vec![
                PixelWrite { x: 0, y: 0, color: Rgba::new(255, 0, 0, 255) },
                PixelWrite { x: 3, y: 2, color: Rgba::new(0, 0, 255, 100) },
            ],
        })
        .unwrap();
        e.document().clone()
    }

    #[test]
    fn png_export_is_pixel_exact_and_keeps_transparency() {
        let d = doc();
        let f = d.frames()[0].id;
        let img = png_io::decode_rgba(&png_bytes(&d, PngExportOptions { frame: f, scale: 1 }).unwrap(), 64).unwrap();
        assert_eq!(img, composite_frame(&d, f).unwrap());
        assert_eq!(img.get(1, 1), Rgba::TRANSPARENT);
        assert_eq!(img.get(3, 2), Rgba::new(0, 0, 255, 100));
    }

    #[test]
    fn scaled_export_uses_nearest_neighbour() {
        let d = doc();
        let f = d.frames()[0].id;
        let img = png_io::decode_rgba(&png_bytes(&d, PngExportOptions { frame: f, scale: 4 }).unwrap(), 64).unwrap();
        assert_eq!((img.width(), img.height()), (16, 12));
        for y in 0..4 {
            for x in 0..4 {
                assert_eq!(img.get(x, y), Rgba::new(255, 0, 0, 255));
            }
        }
        assert_eq!(img.get(4, 0), Rgba::TRANSPARENT);
        assert_eq!(img.get(15, 11), Rgba::new(0, 0, 255, 100));
    }

    #[test]
    fn rejects_bad_scale_and_writes_files() {
        let d = doc();
        let f = d.frames()[0].id;
        assert!(matches!(render_frame(&d, f, 0), Err(ExportError::InvalidScale)));
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("out.png");
        export_png(&d, PngExportOptions { frame: f, scale: 2 }, &path).unwrap();
        assert_eq!(png_io::decode_rgba(&std::fs::read(&path).unwrap(), 64).unwrap().width(), 8);
    }
}
