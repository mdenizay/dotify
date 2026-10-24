use crate::error::CoreResult;
use crate::image::Image;
use crate::model::{Document, FrameId};

/// Integer source-over of `src` onto `dst` with an extra layer opacity.
/// Straight alpha in and out; exact and platform independent.
pub fn blend_over(dst: [u8; 4], src: [u8; 4], opacity: u8) -> [u8; 4] {
    let sa = (src[3] as u32 * opacity as u32 + 127) / 255;
    if sa == 0 {
        return dst;
    }
    if sa == 255 {
        return [src[0], src[1], src[2], 255];
    }
    let da = dst[3] as u32;
    let dst_weight = da * (255 - sa);
    let out_a = sa + (dst_weight + 127) / 255;
    let denom = out_a * 255;
    let mut out = [0u8; 4];
    for c in 0..3 {
        let num = src[c] as u32 * sa * 255 + dst[c] as u32 * dst_weight;
        out[c] = ((num + denom / 2) / denom).min(255) as u8;
    }
    out[3] = out_a.min(255) as u8;
    out
}

/// Flatten all visible layers of `frame` into one image.
pub fn composite_frame(doc: &Document, frame: FrameId) -> CoreResult<Image> {
    doc.frame(frame)?;
    let mut out = Image::new(doc.width(), doc.height());
    let mut buf = out.clone().into_raw();
    for layer in doc.layers().iter().filter(|l| l.visible && l.opacity > 0) {
        let Some(cel) = doc.cel(layer.id, frame) else { continue };
        for (d, s) in buf.chunks_exact_mut(4).zip(cel.data().chunks_exact(4)) {
            if s[3] != 0 {
                let r = blend_over([d[0], d[1], d[2], d[3]], [s[0], s[1], s[2], s[3]], layer.opacity);
                d.copy_from_slice(&r);
            }
        }
    }
    out = Image::from_raw(doc.width(), doc.height(), buf).expect("buffer size matches canvas");
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opaque_source_replaces() {
        assert_eq!(blend_over([1, 2, 3, 255], [9, 8, 7, 255], 255), [9, 8, 7, 255]);
    }

    #[test]
    fn transparent_source_or_zero_opacity_keeps_destination() {
        assert_eq!(blend_over([1, 2, 3, 200], [9, 8, 7, 0], 255), [1, 2, 3, 200]);
        assert_eq!(blend_over([1, 2, 3, 200], [9, 8, 7, 255], 0), [1, 2, 3, 200]);
    }

    #[test]
    fn onto_transparent_keeps_source_color() {
        assert_eq!(blend_over([0, 0, 0, 0], [200, 100, 50, 128], 255), [200, 100, 50, 128]);
    }

    #[test]
    fn half_white_over_black() {
        assert_eq!(blend_over([0, 0, 0, 255], [255, 255, 255, 255], 128), [128, 128, 128, 255]);
        assert_eq!(blend_over([0, 0, 0, 255], [255, 255, 255, 128], 255), [128, 128, 128, 255]);
    }

    #[test]
    fn semi_over_semi() {
        // 50% red over 50% blue → alpha 75%, red weighted 2:1.
        let r = blend_over([0, 0, 255, 128], [255, 0, 0, 128], 255);
        assert_eq!(r[3], 192);
        assert_eq!(r, [170, 0, 85, 192]);
    }
}
