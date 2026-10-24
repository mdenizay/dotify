//! Lossless RGBA8 PNG encoding/decoding shared by the format and exporters.

use std::io::Cursor;

use crate::image::Image;

#[derive(Debug, thiserror::Error)]
pub enum PngError {
    #[error("PNG encode failed: {0}")]
    Encode(#[from] png::EncodingError),
    #[error("PNG decode failed: {0}")]
    Decode(#[from] png::DecodingError),
    #[error("PNG is {width}x{height}, larger than the allowed {max} pixels per side")]
    TooLarge { width: u32, height: u32, max: u32 },
    #[error("unsupported PNG layout")]
    Unsupported,
}

pub fn encode_rgba(img: &Image) -> Result<Vec<u8>, PngError> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, img.width(), img.height());
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_compression(png::Compression::Balanced);
        let mut writer = enc.write_header()?;
        writer.write_image_data(img.data())?;
        writer.finish()?;
    }
    Ok(out)
}

/// Decode any 8/16-bit PNG into RGBA8. Rejects images larger than
/// `max_side` before allocating pixel memory.
pub fn decode_rgba(bytes: &[u8], max_side: u32) -> Result<Image, PngError> {
    let mut dec = png::Decoder::new(Cursor::new(bytes));
    dec.set_transformations(png::Transformations::normalize_to_color8());
    let header = dec.read_header_info()?;
    let (width, height) = (header.width, header.height);
    if width > max_side || height > max_side {
        return Err(PngError::TooLarge { width, height, max: max_side });
    }
    let mut reader = dec.read_info()?;
    let mut buf = vec![0; reader.output_buffer_size().ok_or(PngError::Unsupported)?];
    let info = reader.next_frame(&mut buf)?;
    buf.truncate(info.buffer_size());
    let px = width as usize * height as usize;
    let rgba = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => buf.chunks_exact(3).flat_map(|c| [c[0], c[1], c[2], 255]).collect(),
        png::ColorType::GrayscaleAlpha => buf.chunks_exact(2).flat_map(|c| [c[0], c[0], c[0], c[1]]).collect(),
        png::ColorType::Grayscale => buf.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => return Err(PngError::Unsupported),
    };
    if rgba.len() != px * 4 {
        return Err(PngError::Unsupported);
    }
    Image::from_raw(width, height, rgba).ok_or(PngError::Unsupported)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Rgba;

    #[test]
    fn round_trip_preserves_every_channel() {
        let mut img = Image::new(3, 2);
        img.set(0, 0, Rgba::new(1, 2, 3, 4));
        img.set(2, 1, Rgba::new(250, 128, 0, 255));
        img.set(1, 1, Rgba::new(9, 9, 9, 0));
        let bytes = encode_rgba(&img).unwrap();
        assert_eq!(decode_rgba(&bytes, 16).unwrap(), img);
    }

    #[test]
    fn rejects_oversized() {
        let bytes = encode_rgba(&Image::new(20, 1)).unwrap();
        assert!(matches!(decode_rgba(&bytes, 16), Err(PngError::TooLarge { .. })));
    }
}
