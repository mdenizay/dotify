use std::fmt;

use crate::color::Rgba;
use crate::geometry::{Bounds, Rect};

/// A `width × height` RGBA8 bitmap, straight alpha, row-major.
#[derive(Clone, PartialEq, Eq)]
pub struct Image {
    width: u32,
    height: u32,
    data: Vec<u8>,
}

impl Image {
    /// A fully transparent image.
    pub fn new(width: u32, height: u32) -> Self {
        Self { width, height, data: vec![0; width as usize * height as usize * 4] }
    }

    pub fn from_raw(width: u32, height: u32, data: Vec<u8>) -> Option<Self> {
        (data.len() == width as usize * height as usize * 4).then_some(Self { width, height, data })
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn data(&self) -> &[u8] {
        &self.data
    }

    pub fn into_raw(self) -> Vec<u8> {
        self.data
    }

    pub fn bounds(&self) -> Rect {
        Rect::new(0, 0, self.width, self.height)
    }

    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && (x as u32) < self.width && (y as u32) < self.height
    }

    fn offset(&self, x: u32, y: u32) -> usize {
        (y as usize * self.width as usize + x as usize) * 4
    }

    /// Panics if out of bounds.
    pub fn get(&self, x: u32, y: u32) -> Rgba {
        assert!(x < self.width && y < self.height, "pixel ({x}, {y}) out of bounds");
        let i = self.offset(x, y);
        Rgba::from_array([self.data[i], self.data[i + 1], self.data[i + 2], self.data[i + 3]])
    }

    /// Panics if out of bounds.
    pub fn set(&mut self, x: u32, y: u32, color: Rgba) {
        assert!(x < self.width && y < self.height, "pixel ({x}, {y}) out of bounds");
        let i = self.offset(x, y);
        self.data[i..i + 4].copy_from_slice(&color.to_array());
    }

    /// Copy the pixels of `rect` (must be inside the image) into a new buffer.
    pub fn read_region(&self, rect: Rect) -> Vec<u8> {
        assert!(rect.right() <= self.width && rect.bottom() <= self.height, "region out of bounds");
        let row = rect.width as usize * 4;
        let mut out = Vec::with_capacity(row * rect.height as usize);
        for y in rect.y..rect.bottom() {
            let start = self.offset(rect.x, y);
            out.extend_from_slice(&self.data[start..start + row]);
        }
        out
    }

    /// Overwrite the pixels of `rect` (must be inside the image).
    pub fn write_region(&mut self, rect: Rect, pixels: &[u8]) {
        assert!(rect.right() <= self.width && rect.bottom() <= self.height, "region out of bounds");
        let row = rect.width as usize * 4;
        assert_eq!(pixels.len(), row * rect.height as usize, "region buffer size mismatch");
        for (i, y) in (rect.y..rect.bottom()).enumerate() {
            let start = self.offset(rect.x, y);
            self.data[start..start + row].copy_from_slice(&pixels[i * row..(i + 1) * row]);
        }
    }

    pub fn crop(&self, rect: Rect) -> Image {
        Image { width: rect.width, height: rect.height, data: self.read_region(rect) }
    }

    /// True when every pixel is fully transparent.
    pub fn is_transparent(&self) -> bool {
        self.data.chunks_exact(4).all(|p| p[3] == 0)
    }

    /// Bounding box of pixels with non-zero alpha.
    pub fn opaque_bounds(&self) -> Option<Rect> {
        let mut bounds = Bounds::default();
        for y in 0..self.height {
            for x in 0..self.width {
                if self.data[self.offset(x, y) + 3] != 0 {
                    bounds.add(x, y);
                }
            }
        }
        bounds.rect()
    }

    /// Nearest-neighbour integer upscale.
    pub fn scaled(&self, factor: u32) -> Image {
        assert!(factor >= 1);
        if factor == 1 {
            return self.clone();
        }
        let mut out = Image::new(self.width * factor, self.height * factor);
        for y in 0..out.height {
            for x in 0..out.width {
                out.set(x, y, self.get(x / factor, y / factor));
            }
        }
        out
    }
}

impl fmt::Debug for Image {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Image({}x{})", self.width, self.height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn region_round_trip() {
        let mut img = Image::new(4, 4);
        img.set(1, 1, Rgba::WHITE);
        img.set(2, 2, Rgba::BLACK);
        let rect = Rect::new(1, 1, 2, 2);
        let region = img.read_region(rect);
        let mut other = Image::new(4, 4);
        other.write_region(rect, &region);
        assert_eq!(img, other);
    }

    #[test]
    fn opaque_bounds_and_transparency() {
        let mut img = Image::new(8, 8);
        assert!(img.is_transparent());
        assert_eq!(img.opaque_bounds(), None);
        img.set(2, 3, Rgba::WHITE);
        img.set(5, 6, Rgba::new(0, 0, 0, 1));
        assert_eq!(img.opaque_bounds(), Some(Rect::new(2, 3, 4, 4)));
    }

    #[test]
    fn nearest_neighbour_scale() {
        let mut img = Image::new(2, 1);
        img.set(0, 0, Rgba::WHITE);
        let big = img.scaled(3);
        assert_eq!((big.width(), big.height()), (6, 3));
        assert_eq!(big.get(2, 2), Rgba::WHITE);
        assert_eq!(big.get(3, 0), Rgba::TRANSPARENT);
    }
}
