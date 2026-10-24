//! Animated GIF export.
//!
//! Colors are written exactly (no quantization or dithering): a frame with
//! more than 255 distinct colors is an error rather than a lossy export. GIF
//! only has on/off transparency, so partially transparent pixels become
//! opaque at alpha ≥ 128 and transparent below; the report counts them.
//! Delays are stored in hundredths of a second.

use std::collections::BTreeMap;
use std::path::Path;

use ::gif::{DisposalMethod, Encoder, Frame, Repeat};
use dotify_core::{Document, Image, PlaybackDirection};
use serde::{Deserialize, Serialize};

use crate::sheet::playback_order;
use crate::{render_frame, write_atomic, ExportError};

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct GifOptions {
    pub scale: u32,
    /// Export only this tag, with its direction and loop setting. `None`
    /// exports every frame forward, looping.
    pub tag: Option<String>,
}

impl Default for GifOptions {
    fn default() -> Self {
        Self { scale: 1, tag: None }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize)]
pub struct GifReport {
    pub frames: usize,
    /// Distinct opaque colors across the animation.
    pub colors: usize,
    /// Pixels whose partial alpha was rounded to fully opaque/transparent.
    pub alpha_rounded_pixels: u64,
}

/// Shortest delay browsers honour; smaller values are commonly slowed down to 100 ms.
const MIN_DELAY_CS: u16 = 2;

pub fn gif_bytes(doc: &Document, opts: &GifOptions) -> Result<(Vec<u8>, GifReport), ExportError> {
    let frames = doc.frames();
    let (order, looping) = match &opts.tag {
        Some(name) => {
            let tag = doc.tag(name)?;
            (playback_order(doc.tag_range(tag)?, tag.direction), tag.looping)
        }
        None => (playback_order(0..=frames.len() - 1, PlaybackDirection::Forward), true),
    };
    let images = frames.iter().map(|f| render_frame(doc, f.id, opts.scale)).collect::<Result<Vec<_>, _>>()?;
    let (w, h) = (images[0].width(), images[0].height());
    let (w16, h16) = (u16::try_from(w), u16::try_from(h));
    let (Ok(w16), Ok(h16)) = (w16, h16) else { return Err(ExportError::TooLarge(w.max(h) as u64)) };

    let mut report = GifReport { frames: order.len(), ..Default::default() };
    let mut colors: BTreeMap<[u8; 3], u8> = BTreeMap::new();
    let mut global_overflow = false;
    let mut opaque: Vec<Vec<Option<[u8; 3]>>> = Vec::with_capacity(images.len());
    for img in &images {
        let px = binary_alpha(img, &mut report.alpha_rounded_pixels);
        for c in px.iter().flatten() {
            if !colors.contains_key(c) {
                global_overflow |= colors.len() >= 255;
                let next = colors.len() as u8;
                colors.entry(*c).or_insert(next);
            }
        }
        opaque.push(px);
    }
    report.colors = colors.len();

    // One global palette when everything fits, else one per frame.
    let global = (!global_overflow).then(|| palette_of(&colors));
    let mut out = Vec::new();
    {
        let mut enc = Encoder::new(&mut out, w16, h16, global.as_ref().map_or(&[][..], |(p, _)| p))?;
        if looping {
            enc.set_repeat(Repeat::Infinite)?;
        }
        for &i in &order {
            let (palette, index) = match &global {
                Some((_, index)) => (None, index.clone()),
                None => {
                    let mut local = BTreeMap::new();
                    for c in opaque[i].iter().flatten() {
                        let next = local.len();
                        local.entry(*c).or_insert(next);
                    }
                    if local.len() > 255 {
                        return Err(ExportError::TooManyColors(local.len()));
                    }
                    let local: BTreeMap<_, _> = local.into_iter().map(|(c, i)| (c, i as u8)).collect();
                    let (p, index) = palette_of(&local);
                    (Some(p), index)
                }
            };
            let transparent = (index.len()) as u8;
            let buffer: Vec<u8> = opaque[i].iter().map(|c| c.map_or(transparent, |c| index[&c])).collect();
            let delay = ((frames[i].duration_ms + 5) / 10).clamp(MIN_DELAY_CS as u32, u16::MAX as u32) as u16;
            enc.write_frame(&Frame {
                width: w16,
                height: h16,
                buffer: buffer.into(),
                palette,
                transparent: Some(transparent),
                delay,
                // Each frame is a full replacement, so clear before the next.
                dispose: DisposalMethod::Background,
                ..Frame::default()
            })?;
        }
    }
    Ok((out, report))
}

pub fn export_gif(doc: &Document, opts: &GifOptions, path: &Path) -> Result<GifReport, ExportError> {
    let (bytes, report) = gif_bytes(doc, opts)?;
    write_atomic(path, &bytes)?;
    Ok(report)
}

/// Pixels as opaque RGB or `None` for transparent, rounding partial alpha.
fn binary_alpha(img: &Image, rounded: &mut u64) -> Vec<Option<[u8; 3]>> {
    img.data()
        .chunks_exact(4)
        .map(|p| {
            if p[3] != 0 && p[3] != 255 {
                *rounded += 1;
            }
            (p[3] >= 128).then_some([p[0], p[1], p[2]])
        })
        .collect()
}

/// Flat RGB palette (in index order) plus a color → index lookup. The
/// transparent entry is appended after the colors.
fn palette_of(colors: &BTreeMap<[u8; 3], u8>) -> (Vec<u8>, BTreeMap<[u8; 3], u8>) {
    let mut by_index: Vec<_> = colors.iter().map(|(c, i)| (*i, *c)).collect();
    by_index.sort_unstable();
    let mut flat: Vec<u8> = by_index.iter().flat_map(|(_, c)| *c).collect();
    flat.extend_from_slice(&[0, 0, 0]);
    (flat, colors.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::animated_doc;
    use dotify_core::{Command, Editor, PixelWrite, Rgba};

    struct Decoded {
        frames: Vec<(u16, Vec<u8>)>,
        repeat: ::gif::Repeat,
        size: (u16, u16),
    }

    fn decode(bytes: &[u8]) -> Decoded {
        let mut opts = ::gif::DecodeOptions::new();
        opts.set_color_output(::gif::ColorOutput::RGBA);
        let mut dec = opts.read_info(bytes).unwrap();
        let size = (dec.width(), dec.height());
        let mut frames = Vec::new();
        while let Some(f) = dec.read_next_frame().unwrap() {
            frames.push((f.delay, f.buffer.to_vec()));
        }
        Decoded { frames, repeat: dec.repeat(), size }
    }

    /// The expected GIF pixels for a composite: alpha rounded to 0/255.
    fn binary(img: &Image) -> Vec<u8> {
        img.data()
            .chunks_exact(4)
            .flat_map(|p| if p[3] >= 128 { [p[0], p[1], p[2], 255] } else { [0, 0, 0, 0] })
            .collect()
    }

    #[test]
    fn all_frames_round_trip_exactly() {
        let doc = animated_doc();
        let (bytes, report) = gif_bytes(&doc, &GifOptions::default()).unwrap();
        assert_eq!(report, GifReport { frames: 4, colors: 3, alpha_rounded_pixels: 1 });
        let gif = decode(&bytes);
        assert_eq!(gif.size, (4, 3));
        assert_eq!(gif.repeat, ::gif::Repeat::Infinite);
        assert_eq!(gif.frames.iter().map(|f| f.0).collect::<Vec<_>>(), vec![10, 10, 20, 5]);
        for (i, (_, px)) in gif.frames.iter().enumerate() {
            let composite = render_frame(&doc, doc.frames()[i].id, 1).unwrap();
            let px: Vec<u8> =
                px.chunks_exact(4).flat_map(|p| if p[3] == 0 { [0; 4] } else { [p[0], p[1], p[2], p[3]] }).collect();
            assert_eq!(px, binary(&composite), "frame {i}");
        }
    }

    #[test]
    fn tags_set_order_and_looping() {
        let doc = animated_doc();
        let walk = decode(&gif_bytes(&doc, &GifOptions { tag: Some("Walk".into()), scale: 2 }).unwrap().0);
        assert_eq!(walk.size, (8, 6));
        // Ping-pong 0 1 2 1.
        assert_eq!(walk.frames.iter().map(|f| f.0).collect::<Vec<_>>(), vec![10, 10, 20, 10]);
        assert_eq!(walk.frames[1].1, walk.frames[3].1);
        assert_eq!(walk.repeat, ::gif::Repeat::Infinite);
        let hit = decode(&gif_bytes(&doc, &GifOptions { tag: Some("Hit".into()), scale: 1 }).unwrap().0);
        assert_eq!(hit.frames.len(), 1);
        assert_eq!(hit.repeat, ::gif::Repeat::Finite(0));
        assert!(matches!(
            gif_bytes(&doc, &GifOptions { tag: Some("Nope".into()), scale: 1 }),
            Err(ExportError::Core(dotify_core::CoreError::TagNotFound(_)))
        ));
    }

    #[test]
    fn many_colors_use_local_palettes_and_too_many_fail() {
        // 2 frames × 200 unique colors: 400 overall, each frame fits.
        let doc = Document::new("c", 20, 20).unwrap();
        let (l, f0) = (doc.layers()[0].id, doc.frames()[0].id);
        let mut e = Editor::new(doc);
        let f1 = match e.execute(Command::AddFrame { index: None, duration_ms: None }).unwrap().created[0] {
            dotify_core::Created::Frame(f) => f,
            _ => panic!(),
        };
        let fill = |e: &mut Editor, f, base: u32, n: u32| {
            let pixels = (0..n)
                .map(|i| {
                    let v = base + i;
                    PixelWrite {
                        x: (i % 20) as i32,
                        y: (i / 20) as i32,
                        color: Rgba::opaque(v as u8, (v >> 8) as u8, 7),
                    }
                })
                .collect();
            e.execute(Command::SetPixels { layer: l, frame: f, pixels }).unwrap();
        };
        fill(&mut e, f0, 0, 200);
        fill(&mut e, f1, 1000, 200);
        let doc = e.document().clone();
        let (bytes, report) = gif_bytes(&doc, &GifOptions::default()).unwrap();
        assert_eq!(report.colors, 400);
        let gif = decode(&bytes);
        let expect = binary(&render_frame(&doc, f1, 1).unwrap());
        let got: Vec<u8> = gif.frames[1]
            .1
            .chunks_exact(4)
            .flat_map(|p| if p[3] == 0 { [0; 4] } else { [p[0], p[1], p[2], p[3]] })
            .collect();
        assert_eq!(got, expect);

        fill(&mut e, f0, 0, 300);
        assert!(matches!(gif_bytes(e.document(), &GifOptions::default()), Err(ExportError::TooManyColors(300))));
    }
}
