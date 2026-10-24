//! Godot 4 export: a spritesheet PNG, a `SpriteFrames` resource (`.tres`)
//! with one animation per tag, and the generic JSON metadata.
//!
//! Godot shows each frame for `duration / speed` seconds. `speed` is chosen
//! as `1000 / gcd(durations)` so every per-frame `duration` is a whole
//! number and timing matches the document exactly. Reverse and ping-pong
//! tags are unrolled into plain frame lists, since `SpriteFrames` only plays
//! forward.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use dotify_core::Document;
use serde::{Deserialize, Serialize};

use crate::meta::{metadata_json, Pivot};
use crate::sheet::{pack, SheetOptions, Spritesheet};
use crate::{write_atomic, ExportError};

#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct GodotOptions {
    /// File stem for the written files; defaults to the document name.
    pub name: Option<String>,
    pub sheet: SheetOptions,
    /// Directory of the texture inside the Godot project, such as
    /// `res://sprites/hero`. By default the `.tres` references the PNG by a
    /// path relative to itself, so both files can be dropped anywhere in
    /// the project together.
    pub res_dir: Option<String>,
    pub pivot: Option<Pivot>,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize)]
pub struct GodotFiles {
    pub png: PathBuf,
    pub tres: PathBuf,
    pub json: PathBuf,
}

/// Write `<name>.png`, `<name>.tres` and `<name>.json` into `out_dir`.
pub fn export_godot(doc: &Document, opts: &GodotOptions, out_dir: &Path) -> Result<GodotFiles, ExportError> {
    let stem = file_stem(opts.name.as_deref().unwrap_or(doc.name()));
    let sheet = pack(doc, &opts.sheet)?;
    let png_name = format!("{stem}.png");
    let texture = match &opts.res_dir {
        Some(dir) => format!("{}/{png_name}", dir.trim_end_matches('/')),
        None => png_name.clone(),
    };
    let tres = sprite_frames_tres(&sheet, &texture);
    let json = metadata_json(doc, &sheet, &png_name, opts.pivot.unwrap_or_else(|| Pivot::center(doc)));
    let png = dotify_core::png_io::encode_rgba(&sheet.image)?;

    std::fs::create_dir_all(out_dir)?;
    let files = GodotFiles {
        png: out_dir.join(&png_name),
        tres: out_dir.join(format!("{stem}.tres")),
        json: out_dir.join(format!("{stem}.json")),
    };
    // The texture first, so the resource never points at a missing file.
    write_atomic(&files.png, &png)?;
    write_atomic(&files.tres, tres.as_bytes())?;
    write_atomic(&files.json, json.as_bytes())?;
    Ok(files)
}

/// A file stem safe on every platform and in `res://` paths.
pub fn file_stem(name: &str) -> String {
    let stem: String =
        name.trim().chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
    let stem = stem.trim_matches('_');
    if stem.is_empty() {
        "sprite".into()
    } else {
        stem.to_string()
    }
}

/// The `SpriteFrames` resource text for `sheet`, whose image Godot loads
/// from `texture_path`.
pub fn sprite_frames_tres(sheet: &Spritesheet, texture_path: &str) -> String {
    let animations = sheet.animations_or_default();
    let sequences: Vec<Vec<usize>> = animations.iter().map(|a| a.playback()).collect();
    // One AtlasTexture per used cell, numbered in first-use order.
    let mut atlas: Vec<usize> = Vec::new();
    for &cell in sequences.iter().flatten() {
        if !atlas.contains(&cell) {
            atlas.push(cell);
        }
    }

    let mut out = String::new();
    let w = &mut out;
    let _ = writeln!(w, "[gd_resource type=\"SpriteFrames\" load_steps={} format=3]\n", atlas.len() + 2);
    let _ = writeln!(w, "[ext_resource type=\"Texture2D\" path={} id=\"1_sheet\"]\n", quote(texture_path));
    for (i, &cell) in atlas.iter().enumerate() {
        let r = sheet.cells[cell].rect;
        let _ = writeln!(w, "[sub_resource type=\"AtlasTexture\" id=\"AtlasTexture_{i}\"]");
        let _ = writeln!(w, "atlas = ExtResource(\"1_sheet\")");
        let _ = writeln!(w, "region = Rect2({}, {}, {}, {})", r.x, r.y, r.width, r.height);
        let _ = writeln!(w, "filter_clip = true\n");
    }
    let _ = writeln!(w, "[resource]");
    let _ = write!(w, "animations = [");
    for (a, (anim, seq)) in animations.iter().zip(&sequences).enumerate() {
        let unit = seq.iter().map(|&c| sheet.cells[c].duration_ms).fold(0, gcd).max(1);
        let _ = write!(w, "{}{{\n\"frames\": [", if a == 0 { "" } else { ", " });
        for (i, &cell) in seq.iter().enumerate() {
            let id = atlas.iter().position(|&c| c == cell).expect("cell has an atlas texture");
            let duration = sheet.cells[cell].duration_ms / unit;
            let _ = write!(
                w,
                "{}{{\n\"duration\": {},\n\"texture\": SubResource(\"AtlasTexture_{id}\")\n}}",
                if i == 0 { "" } else { ", " },
                real(duration as f64)
            );
        }
        let _ = write!(
            w,
            "],\n\"loop\": {},\n\"name\": &{},\n\"speed\": {}\n}}",
            anim.looping,
            quote(&anim.name),
            real(1000.0 / unit as f64)
        );
    }
    let _ = writeln!(w, "]");
    out
}

fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 {
        a
    } else {
        gcd(b, a % b)
    }
}

/// A Godot string literal.
fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A Godot float literal: always has a decimal point, at most 6 decimals.
fn real(v: f64) -> String {
    let s = format!("{v:.6}");
    let s = s.trim_end_matches('0');
    if s.ends_with('.') {
        format!("{s}0")
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sheet::SheetLayout;
    use crate::tests::animated_doc;

    const SNAPSHOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/snapshots/hero.tres");

    #[test]
    fn tres_matches_snapshot() {
        let doc = animated_doc();
        let sheet = pack(&doc, &SheetOptions { padding: 1, ..Default::default() }).unwrap();
        let tres = sprite_frames_tres(&sheet, "hero.png");
        if std::env::var_os("DOTIFY_UPDATE_SNAPSHOTS").is_some() {
            std::fs::write(SNAPSHOT, &tres).unwrap();
        }
        let expected = std::fs::read_to_string(SNAPSHOT).expect("run with DOTIFY_UPDATE_SNAPSHOTS=1 to create");
        assert_eq!(tres, expected);
    }

    #[test]
    fn speed_and_durations_preserve_timing() {
        let doc = animated_doc();
        let sheet = pack(&doc, &SheetOptions::default()).unwrap();
        let tres = sprite_frames_tres(&sheet, "x.png");
        // Walk: 100, 100, 200, 100 ms → 10 fps with durations 1, 1, 2, 1.
        assert!(tres.contains("\"name\": &\"Walk\",\n\"speed\": 10.0"));
        // Hit: one 50 ms frame → 20 fps, duration 1, plays once.
        assert!(tres.contains("\"loop\": false,\n\"name\": &\"Hit\",\n\"speed\": 20.0"));
        assert_eq!(tres.matches("\"duration\": 2.0").count(), 1);
    }

    #[test]
    fn untagged_documents_get_a_default_animation() {
        let doc = Document::new("Plain", 2, 2).unwrap();
        let sheet = pack(&doc, &SheetOptions { layout: SheetLayout::RowsPerTag, ..Default::default() }).unwrap();
        let tres = sprite_frames_tres(&sheet, "res://a \"b\".png");
        assert!(tres.contains("path=\"res://a \\\"b\\\".png\""));
        assert!(tres.contains("\"name\": &\"default\""));
        assert!(tres.contains("load_steps=3"));
    }

    #[test]
    fn helpers() {
        assert_eq!(real(10.0), "10.0");
        assert_eq!(real(1000.0 / 30.0), "33.333333");
        assert_eq!(real(12.5), "12.5");
        assert_eq!(file_stem(" My Hero!/v2 "), "My_Hero__v2");
        assert_eq!(file_stem("///"), "sprite");
        assert_eq!(gcd(100, 250), 50);
    }

    #[test]
    fn writes_all_files() {
        let doc = animated_doc();
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("godot");
        let opts = GodotOptions { res_dir: Some("res://art/".into()), ..Default::default() };
        let files = export_godot(&doc, &opts, &out).unwrap();
        assert_eq!(files.tres, out.join("Hero.tres"));
        let tres = std::fs::read_to_string(&files.tres).unwrap();
        assert!(tres.contains("path=\"res://art/Hero.png\""));
        let png = dotify_core::png_io::decode_rgba(&std::fs::read(&files.png).unwrap(), 1024).unwrap();
        assert_eq!(png, pack(&doc, &opts.sheet).unwrap().image);
        let json: serde_json::Value = serde_json::from_slice(&std::fs::read(&files.json).unwrap()).unwrap();
        assert_eq!(json["image"], "Hero.png");
        assert_eq!(json["pivot"], serde_json::json!({ "x": 2.0, "y": 1.5 }));
    }
}
