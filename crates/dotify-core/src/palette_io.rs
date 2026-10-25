//! Reading and writing palette files: GIMP `.gpl`, JASC `.pal`, Lospec-style
//! `.hex`, Paint.NET `.txt` and PNG swatch images.

use crate::color::Rgba;
use crate::command::MAX_PALETTE_COLORS;
use crate::model::Palette;
use crate::png_io;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PaletteError {
    #[error("unrecognized palette format")]
    UnknownFormat,
    #[error("line {line}: {message}")]
    Syntax { line: usize, message: String },
    #[error("the palette has no colors")]
    Empty,
    #[error("the palette has {0} colors; at most {MAX_PALETTE_COLORS} are supported")]
    TooManyColors(usize),
    #[error("could not read the image: {0}")]
    Image(String),
}

impl PaletteError {
    pub fn code(&self) -> &'static str {
        match self {
            PaletteError::UnknownFormat => "unknown_palette_format",
            PaletteError::Syntax { .. } => "palette_syntax",
            PaletteError::Empty => "empty_palette",
            PaletteError::TooManyColors(_) => "too_many_colors",
            PaletteError::Image(_) => "palette_image",
        }
    }
}

/// Largest swatch image accepted, per side.
const MAX_IMAGE_SIDE: u32 = 1024;

/// Parse a palette file. The format is detected from the contents, falling
/// back to `file_name`'s extension; the palette name comes from the file
/// (GIMP `Name:`) or the file name without extension.
pub fn parse(file_name: &str, bytes: &[u8]) -> Result<Palette, PaletteError> {
    let stem = file_name.rsplit(['/', '\\']).next().unwrap_or(file_name);
    let stem = stem.rsplit_once('.').map_or(stem, |(s, _)| s).trim();
    let fallback_name = if stem.is_empty() { "Imported" } else { stem };

    let (name, colors) = if bytes.starts_with(b"\x89PNG") {
        (None, from_png(bytes)?)
    } else {
        let text = String::from_utf8_lossy(bytes);
        let text = text.trim_start_matches('\u{feff}');
        let first = text.lines().next().unwrap_or("").trim();
        if first.eq_ignore_ascii_case("GIMP Palette") {
            parse_gpl(text)?
        } else if first.eq_ignore_ascii_case("JASC-PAL") {
            (None, parse_jasc(text)?)
        } else {
            (None, parse_hex_lines(text)?)
        }
    };
    let colors = dedupe(colors);
    if colors.is_empty() {
        return Err(PaletteError::Empty);
    }
    if colors.len() > MAX_PALETTE_COLORS {
        return Err(PaletteError::TooManyColors(colors.len()));
    }
    let name = name.filter(|n| !n.trim().is_empty()).unwrap_or_else(|| fallback_name.to_string());
    Ok(Palette { name: name.trim().chars().take(crate::command::MAX_NAME_LEN).collect(), colors })
}

/// Serialize as a GIMP palette, readable by most pixel art tools.
pub fn to_gpl(palette: &Palette) -> String {
    let mut out = format!("GIMP Palette\nName: {}\nColumns: 8\n#\n", palette.name);
    for c in &palette.colors {
        out.push_str(&format!("{:3} {:3} {:3}\t{}\n", c.r, c.g, c.b, &c.to_hex()[1..7]));
    }
    out
}

fn syntax(line: usize, message: impl Into<String>) -> PaletteError {
    PaletteError::Syntax { line: line + 1, message: message.into() }
}

fn byte(s: &str, line: usize) -> Result<u8, PaletteError> {
    s.parse::<u8>().map_err(|_| syntax(line, format!("{s:?} is not a number from 0 to 255")))
}

fn parse_gpl(text: &str) -> Result<(Option<String>, Vec<Rgba>), PaletteError> {
    let mut name = None;
    let mut colors = Vec::new();
    for (i, raw) in text.lines().enumerate().skip(1) {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(n) = line.strip_prefix("Name:") {
            name = Some(n.trim().to_string());
            continue;
        }
        if line.starts_with("Columns:") {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 3 {
            return Err(syntax(i, "expected \"R G B [name]\""));
        }
        colors.push(Rgba::opaque(byte(parts[0], i)?, byte(parts[1], i)?, byte(parts[2], i)?));
    }
    Ok((name, colors))
}

fn parse_jasc(text: &str) -> Result<Vec<Rgba>, PaletteError> {
    let lines: Vec<&str> = text.lines().map(str::trim).collect();
    let count: usize =
        lines.get(2).and_then(|l| l.parse().ok()).ok_or_else(|| syntax(2, "expected the color count"))?;
    let mut colors = Vec::with_capacity(count);
    for (i, line) in lines.iter().enumerate().skip(3).filter(|(_, l)| !l.is_empty()).take(count) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 3 {
            return Err(syntax(i, "expected \"R G B\""));
        }
        colors.push(Rgba::opaque(byte(parts[0], i)?, byte(parts[1], i)?, byte(parts[2], i)?));
    }
    if colors.len() != count {
        return Err(syntax(lines.len(), format!("expected {count} colors, found {}", colors.len())));
    }
    Ok(colors)
}

/// One color per line: `rrggbb`, `#rrggbb`, `rrggbbaa` or Paint.NET
/// `AARRGGBB`; `;` and `//` start comments.
fn parse_hex_lines(text: &str) -> Result<Vec<Rgba>, PaletteError> {
    let mut colors = Vec::new();
    let mut paint_net = false;
    for (i, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.starts_with(';') {
            paint_net = true;
            continue;
        }
        if line.is_empty() || line.starts_with("//") {
            continue;
        }
        let hex = line.trim_start_matches('#');
        if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return if colors.is_empty() {
                Err(PaletteError::UnknownFormat)
            } else {
                Err(syntax(i, "not a hex color"))
            };
        }
        let color = match hex.len() {
            6 => format!("#{hex}").parse::<Rgba>().ok(),
            // Paint.NET writes AARRGGBB; elsewhere 8 digits are RRGGBBAA.
            8 if paint_net => format!("#{}{}", &hex[2..], &hex[..2]).parse::<Rgba>().ok(),
            8 => format!("#{hex}").parse::<Rgba>().ok(),
            _ => None,
        };
        match color {
            Some(c) => colors.push(c),
            None => return Err(syntax(i, "expected 6 or 8 hex digits")),
        }
    }
    Ok(colors)
}

/// Every distinct non-transparent color of a swatch image, in reading order.
fn from_png(bytes: &[u8]) -> Result<Vec<Rgba>, PaletteError> {
    let img = png_io::decode_rgba(bytes, MAX_IMAGE_SIDE).map_err(|e| PaletteError::Image(e.to_string()))?;
    let mut colors = Vec::new();
    for y in 0..img.height() {
        for x in 0..img.width() {
            let c = img.get(x, y);
            if c.a != 0 {
                colors.push(c);
            }
        }
    }
    Ok(colors)
}

fn dedupe(colors: Vec<Rgba>) -> Vec<Rgba> {
    let mut seen = std::collections::HashSet::new();
    colors.into_iter().filter(|c| seen.insert(*c)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::Image;

    const RED: Rgba = Rgba::opaque(255, 0, 0);
    const TEAL: Rgba = Rgba::opaque(0x12, 0x80, 0x7f);

    #[test]
    fn reads_gimp_palettes_with_name_and_comments() {
        let gpl = "GIMP Palette\nName: Sunset 8\nColumns: 4\n# comment\n255   0   0\tRed\n 18 128 127 Teal\n";
        let p = parse("whatever.gpl", gpl.as_bytes()).unwrap();
        assert_eq!(p.name, "Sunset 8");
        assert_eq!(p.colors, vec![RED, TEAL]);
    }

    #[test]
    fn gimp_round_trip() {
        let p = Palette { name: "Mine".into(), colors: vec![RED, TEAL] };
        assert_eq!(parse("x.gpl", to_gpl(&p).as_bytes()).unwrap(), p);
    }

    #[test]
    fn reads_jasc_palettes() {
        let pal = "JASC-PAL\r\n0100\r\n2\r\n255 0 0\r\n18 128 127\r\n";
        let p = parse("retro.pal", pal.as_bytes()).unwrap();
        assert_eq!((p.name.as_str(), p.colors.clone()), ("retro", vec![RED, TEAL]));
        assert!(matches!(parse("bad.pal", b"JASC-PAL\n0100\n3\n1 2 3\n"), Err(PaletteError::Syntax { .. })));
    }

    #[test]
    fn reads_lospec_hex_and_paint_net() {
        let p = parse("palettes/endless.hex", b"ff0000\n#12807f\n\n").unwrap();
        assert_eq!((p.name.as_str(), p.colors.clone()), ("endless", vec![RED, TEAL]));
        let pdn = parse("p.txt", b"; paint.net Palette File\n;Colors: 2\nFFFF0000\n8012807F\n").unwrap();
        assert_eq!(pdn.colors, vec![RED, Rgba::new(0x12, 0x80, 0x7f, 0x80)]);
    }

    #[test]
    fn reads_png_swatches_in_reading_order_without_duplicates() {
        let mut img = Image::new(3, 2);
        img.set(0, 0, TEAL);
        img.set(1, 0, RED);
        img.set(2, 0, TEAL);
        img.set(0, 1, RED);
        let png = png_io::encode_rgba(&img).unwrap();
        let p = parse("Swatches.png", &png).unwrap();
        assert_eq!((p.name.as_str(), p.colors.clone()), ("Swatches", vec![TEAL, RED]));
    }

    #[test]
    fn rejects_garbage_empty_and_oversized_input() {
        assert_eq!(parse("notes.txt", b"hello world").unwrap_err(), PaletteError::UnknownFormat);
        assert_eq!(parse("empty.hex", b"\n\n").unwrap_err(), PaletteError::Empty);
        let many: String = (0..300).map(|i| format!("{:06x}\n", i * 37)).collect();
        assert_eq!(parse("big.hex", many.as_bytes()).unwrap_err(), PaletteError::TooManyColors(300));
        assert!(matches!(parse("x.gpl", b"GIMP Palette\n300 0 0\n"), Err(PaletteError::Syntax { line: 2, .. })));
    }
}
