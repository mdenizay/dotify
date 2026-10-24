//! Deterministic pixel rasterisation algorithms.
//!
//! These are mirrored in `apps/desktop/src/canvas/raster.ts` for live
//! previews; keep both implementations in sync.

use crate::color::Rgba;
use crate::geometry::Point;
use crate::image::Image;

/// Bresenham line from `a` to `b`, both endpoints included.
pub fn line(a: Point, b: Point, mut plot: impl FnMut(i32, i32)) {
    let (mut x, mut y) = (a.x, a.y);
    let dx = (b.x - a.x).abs();
    let dy = -(b.y - a.y).abs();
    let sx = if a.x < b.x { 1 } else { -1 };
    let sy = if a.y < b.y { 1 } else { -1 };
    let mut err = dx + dy;
    loop {
        plot(x, y);
        if x == b.x && y == b.y {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
}

/// Rectangle with top-left `(x, y)` and size `w × h`.
pub fn rectangle(x: i32, y: i32, w: u32, h: u32, filled: bool, mut plot: impl FnMut(i32, i32)) {
    if w == 0 || h == 0 {
        return;
    }
    let (w, h) = (w as i32, h as i32);
    for j in 0..h {
        for i in 0..w {
            if filled || i == 0 || j == 0 || i == w - 1 || j == h - 1 {
                plot(x + i, y + j);
            }
        }
    }
}

/// Horizontal extent `[left, right]` of an ellipse spanning `major` pixels
/// across row `j` of `minor` rows.
fn ellipse_span(major: u32, minor: u32, j: u32) -> Option<(u32, u32)> {
    let half_major = major as f64 / 2.0;
    let t = (2.0 * j as f64 + 1.0 - minor as f64) / minor as f64;
    let half = half_major * (1.0 - t * t).max(0.0).sqrt();
    let left = (half_major - half).round();
    let right = (half_major + half).round() - 1.0;
    (right >= left).then_some((left as u32, right as u32))
}

/// Ellipse inscribed in the box `(x, y, w, h)`.
///
/// Computed as the union of row spans and column spans, which gives gap-free
/// outlines at every aspect ratio. The outline is each span's endpoints.
pub fn ellipse(x: i32, y: i32, w: u32, h: u32, filled: bool, mut plot: impl FnMut(i32, i32)) {
    if w == 0 || h == 0 {
        return;
    }
    let mut mask = vec![false; w as usize * h as usize];
    let idx = |i: u32, j: u32| (j * w + i) as usize;
    for j in 0..h {
        if let Some((l, r)) = ellipse_span(w, h, j) {
            if filled {
                (l..=r).for_each(|i| mask[idx(i, j)] = true);
            } else {
                mask[idx(l, j)] = true;
                mask[idx(r, j)] = true;
            }
        }
    }
    for i in 0..w {
        if let Some((t, b)) = ellipse_span(h, w, i) {
            if filled {
                (t..=b).for_each(|j| mask[idx(i, j)] = true);
            } else {
                mask[idx(i, t)] = true;
                mask[idx(i, b)] = true;
            }
        }
    }
    for j in 0..h {
        for i in 0..w {
            if mask[idx(i, j)] {
                plot(x + i as i32, y + j as i32);
            }
        }
    }
}

/// Pixels that a bucket fill starting at `(x, y)` would change.
///
/// `contiguous` uses 4-connectivity; otherwise every pixel matching the seed
/// color is selected. Returns an empty list if the seed is outside the image
/// or already has `color`.
pub fn flood_fill_pixels(img: &Image, x: i32, y: i32, color: Rgba, contiguous: bool) -> Vec<(u32, u32)> {
    if !img.contains(x, y) {
        return Vec::new();
    }
    let (x, y) = (x as u32, y as u32);
    let target = img.get(x, y);
    if target == color {
        return Vec::new();
    }
    let (w, h) = (img.width(), img.height());
    let mut out = Vec::new();
    if !contiguous {
        for py in 0..h {
            for px in 0..w {
                if img.get(px, py).matches(target) {
                    out.push((px, py));
                }
            }
        }
        return out;
    }

    let mut visited = vec![false; w as usize * h as usize];
    let matches =
        |px: u32, py: u32, visited: &[bool]| !visited[(py * w + px) as usize] && img.get(px, py).matches(target);
    let mut stack = vec![(x, y)];
    while let Some((sx, sy)) = stack.pop() {
        if !matches(sx, sy, &visited) {
            continue;
        }
        let mut left = sx;
        while left > 0 && matches(left - 1, sy, &visited) {
            left -= 1;
        }
        let mut right = sx;
        while right + 1 < w && matches(right + 1, sy, &visited) {
            right += 1;
        }
        for px in left..=right {
            visited[(sy * w + px) as usize] = true;
            out.push((px, sy));
        }
        for ny in [sy.checked_sub(1), (sy + 1 < h).then_some(sy + 1)].into_iter().flatten() {
            let mut px = left;
            while px <= right {
                if matches(px, ny, &visited) {
                    stack.push((px, ny));
                    // Skip the rest of this run; it is reached from the seed.
                    while px <= right && matches(px, ny, &visited) {
                        px += 1;
                    }
                } else {
                    px += 1;
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collect(f: impl FnOnce(&mut dyn FnMut(i32, i32))) -> Vec<(i32, i32)> {
        let mut v = Vec::new();
        f(&mut |x, y| v.push((x, y)));
        v
    }

    #[test]
    fn line_includes_endpoints_and_is_connected() {
        let pts = collect(|p| line(Point::new(0, 0), Point::new(5, 2), p));
        assert_eq!(pts.first(), Some(&(0, 0)));
        assert_eq!(pts.last(), Some(&(5, 2)));
        assert_eq!(pts.len(), 6);
        for w in pts.windows(2) {
            assert!((w[0].0 - w[1].0).abs() <= 1 && (w[0].1 - w[1].1).abs() <= 1);
        }
    }

    #[test]
    fn line_single_point_and_reverse_symmetry() {
        assert_eq!(collect(|p| line(Point::new(3, 3), Point::new(3, 3), p)), vec![(3, 3)]);
        let fwd = collect(|p| line(Point::new(0, 0), Point::new(-4, 4), p));
        assert_eq!(fwd, vec![(0, 0), (-1, 1), (-2, 2), (-3, 3), (-4, 4)]);
    }

    #[test]
    fn rectangle_outline_and_fill_counts() {
        assert_eq!(collect(|p| rectangle(0, 0, 4, 3, false, p)).len(), 10);
        assert_eq!(collect(|p| rectangle(0, 0, 4, 3, true, p)).len(), 12);
        assert_eq!(collect(|p| rectangle(0, 0, 1, 1, false, p)), vec![(0, 0)]);
    }

    #[test]
    fn ellipse_is_symmetric_and_inside_box() {
        for (w, h) in [(1, 1), (2, 2), (5, 3), (8, 8), (13, 6), (1, 9)] {
            for filled in [false, true] {
                let pts = collect(|p| ellipse(0, 0, w, h, filled, p));
                assert!(!pts.is_empty());
                for &(x, y) in &pts {
                    assert!(x >= 0 && y >= 0 && x < w as i32 && y < h as i32);
                    assert!(pts.contains(&(w as i32 - 1 - x, y)), "{w}x{h} not mirror symmetric");
                    assert!(pts.contains(&(x, h as i32 - 1 - y)), "{w}x{h} not flip symmetric");
                }
            }
        }
    }

    #[test]
    fn ellipse_8x8_outline_shape() {
        let mut grid = [[' '; 8]; 8];
        ellipse(0, 0, 8, 8, false, |x, y| grid[y as usize][x as usize] = '#');
        let rows: Vec<String> = grid.iter().map(|r| r.iter().collect()).collect();
        assert_eq!(
            rows,
            ["  ####  ", " #    # ", "#      #", "#      #", "#      #", "#      #", " #    # ", "  ####  "]
        );
    }

    #[test]
    fn flood_fill_respects_walls() {
        let mut img = Image::new(5, 5);
        for y in 0..5 {
            img.set(2, y, Rgba::BLACK);
        }
        let px = flood_fill_pixels(&img, 0, 0, Rgba::WHITE, true);
        assert_eq!(px.len(), 10);
        assert!(px.iter().all(|&(x, _)| x < 2));
    }

    #[test]
    fn flood_fill_global_and_noop() {
        let mut img = Image::new(3, 1);
        img.set(0, 0, Rgba::BLACK);
        img.set(2, 0, Rgba::BLACK);
        assert_eq!(flood_fill_pixels(&img, 0, 0, Rgba::WHITE, true).len(), 1);
        assert_eq!(flood_fill_pixels(&img, 0, 0, Rgba::WHITE, false).len(), 2);
        assert!(flood_fill_pixels(&img, 0, 0, Rgba::BLACK, true).is_empty());
        assert!(flood_fill_pixels(&img, 9, 0, Rgba::BLACK, true).is_empty());
    }

    #[test]
    fn flood_fill_large_area_without_recursion() {
        let img = Image::new(1024, 1024);
        assert_eq!(flood_fill_pixels(&img, 512, 512, Rgba::WHITE, true).len(), 1024 * 1024);
    }

    #[test]
    fn flood_fill_spiral_maze() {
        // A wall pattern with a single winding corridor.
        let rows = ["#.#####", "#.#...#", "#.#.#.#", "#...#.#", "#####.#"];
        let mut img = Image::new(7, 5);
        for (y, r) in rows.iter().enumerate() {
            for (x, c) in r.chars().enumerate() {
                if c == '#' {
                    img.set(x as u32, y as u32, Rgba::BLACK);
                }
            }
        }
        let open = rows.iter().flat_map(|r| r.chars()).filter(|&c| c == '.').count();
        assert_eq!(flood_fill_pixels(&img, 1, 0, Rgba::WHITE, true).len(), open);
    }
}
