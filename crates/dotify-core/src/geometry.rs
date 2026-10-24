use serde::{Deserialize, Serialize};

/// A pixel coordinate. May lie outside the canvas; drawing clips.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

impl Point {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

/// A non-empty axis-aligned rectangle in canvas pixel space.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    pub const fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self { x, y, width, height }
    }

    pub fn right(&self) -> u32 {
        self.x + self.width
    }

    pub fn bottom(&self) -> u32 {
        self.y + self.height
    }

    pub fn union(&self, other: &Rect) -> Rect {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        Rect::new(x, y, self.right().max(other.right()) - x, self.bottom().max(other.bottom()) - y)
    }

    /// Clip a signed rectangle to `0..width × 0..height`. Returns `None` if
    /// nothing remains.
    pub fn clip(x: i64, y: i64, w: i64, h: i64, width: u32, height: u32) -> Option<Rect> {
        let x0 = x.max(0);
        let y0 = y.max(0);
        let x1 = (x + w).min(width as i64);
        let y1 = (y + h).min(height as i64);
        if x1 <= x0 || y1 <= y0 {
            return None;
        }
        Some(Rect::new(x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32))
    }
}

/// Incrementally computes the bounding box of a set of points.
#[derive(Default, Clone, Copy, Debug)]
pub(crate) struct Bounds {
    min: Option<(u32, u32)>,
    max: (u32, u32),
}

impl Bounds {
    pub fn add(&mut self, x: u32, y: u32) {
        match self.min {
            None => {
                self.min = Some((x, y));
                self.max = (x, y);
            }
            Some((mx, my)) => {
                self.min = Some((mx.min(x), my.min(y)));
                self.max = (self.max.0.max(x), self.max.1.max(y));
            }
        }
    }

    pub fn rect(&self) -> Option<Rect> {
        self.min.map(|(x, y)| Rect::new(x, y, self.max.0 - x + 1, self.max.1 - y + 1))
    }
}
