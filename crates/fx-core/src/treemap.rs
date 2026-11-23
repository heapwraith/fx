//! Squarified treemap (Bruls, Huizing and van Wijk, 2000) for the disk map
//! (T6.1, D-068): each item gets a rectangle with an area proportional to its
//! size, as close to a square as the algorithm can make it, so sizes compare
//! at a glance.

/// A rectangle, in the units of the area given.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// Rectangles for `sizes` (largest first gives the best layout) inside
/// `area`, in the same order. Sizes of zero or less get an empty rectangle.
pub fn squarify(sizes: &[f64], area: Rect) -> Vec<Rect> {
    let mut out = vec![Rect::default(); sizes.len()];
    let total: f64 = sizes.iter().map(|s| s.max(0.0)).sum();
    if total <= 0.0 || area.w <= 0.0 || area.h <= 0.0 {
        return out;
    }
    let scale = f64::from(area.w) * f64::from(area.h) / total;
    let areas: Vec<(usize, f64)> = sizes
        .iter()
        .enumerate()
        .filter(|(_, s)| **s > 0.0)
        .map(|(k, s)| (k, s * scale))
        .collect();
    let mut r = area;
    let mut i = 0;
    while i < areas.len() {
        let short = f64::from(r.w.min(r.h));
        if short <= 0.0 {
            break;
        }
        // Grow the row while it makes the worst aspect ratio no worse.
        let mut end = i + 1;
        let mut best = worst(&areas[i..end], short);
        while end < areas.len() {
            let next = worst(&areas[i..=end], short);
            if next > best {
                break;
            }
            best = next;
            end += 1;
        }
        let row = &areas[i..end];
        let sum: f64 = row.iter().map(|(_, a)| a).sum();
        if r.w >= r.h {
            // A column along the left side.
            let width = (sum / f64::from(r.h)) as f32;
            let mut y = r.y;
            for &(k, a) in row {
                let h = (a / f64::from(width)) as f32;
                out[k] = Rect {
                    x: r.x,
                    y,
                    w: width,
                    h,
                };
                y += h;
            }
            r = Rect {
                x: r.x + width,
                y: r.y,
                w: (r.w - width).max(0.0),
                h: r.h,
            };
        } else {
            // A row along the top.
            let height = (sum / f64::from(r.w)) as f32;
            let mut x = r.x;
            for &(k, a) in row {
                let w = (a / f64::from(height)) as f32;
                out[k] = Rect {
                    x,
                    y: r.y,
                    w,
                    h: height,
                };
                x += w;
            }
            r = Rect {
                x: r.x,
                y: r.y + height,
                w: r.w,
                h: (r.h - height).max(0.0),
            };
        }
        i = end;
    }
    out
}

/// The worst aspect ratio of a row laid along a side of length `short`.
fn worst(row: &[(usize, f64)], short: f64) -> f64 {
    let sum: f64 = row.iter().map(|(_, a)| a).sum();
    let max = row.iter().map(|(_, a)| *a).fold(0.0, f64::max);
    let min = row
        .iter()
        .map(|(_, a)| *a)
        .fold(f64::INFINITY, f64::min)
        .max(f64::MIN_POSITIVE);
    let (s2, w2) = (sum * sum, short * short);
    f64::max(w2 * max / s2, s2 / (w2 * min))
}

#[cfg(test)]
mod tests {
    use super::*;

    const AREA: Rect = Rect {
        x: 0.0,
        y: 0.0,
        w: 600.0,
        h: 400.0,
    };

    fn inside(r: &Rect) -> bool {
        r.x >= -0.01 && r.y >= -0.01 && r.x + r.w <= 600.01 && r.y + r.h <= 400.01
    }

    fn overlap(a: &Rect, b: &Rect) -> bool {
        let eps = 0.01;
        a.x + eps < b.x + b.w
            && b.x + eps < a.x + a.w
            && a.y + eps < b.y + b.h
            && b.y + eps < a.y + a.h
    }

    #[test]
    fn areas_follow_sizes_and_fill_the_space() {
        let sizes = [6.0, 6.0, 4.0, 3.0, 2.0, 2.0, 1.0];
        let rects = squarify(&sizes, AREA);
        let total: f64 = sizes.iter().sum();
        for (s, r) in sizes.iter().zip(&rects) {
            let expected = s / total * 240_000.0;
            let got = f64::from(r.w * r.h);
            assert!((got - expected).abs() < 1.0, "{got} vs {expected}");
            assert!(inside(r), "{r:?}");
        }
        for (k, a) in rects.iter().enumerate() {
            for b in &rects[k + 1..] {
                assert!(!overlap(a, b), "{a:?} {b:?}");
            }
        }
    }

    #[test]
    fn equal_sizes_make_near_squares() {
        let rects = squarify(
            &[1.0; 4],
            Rect {
                x: 0.0,
                y: 0.0,
                w: 400.0,
                h: 400.0,
            },
        );
        for r in rects {
            let ratio = r.w.max(r.h) / r.w.min(r.h);
            assert!(ratio < 1.01, "{r:?}");
        }
    }

    #[test]
    fn nothing_to_lay_out() {
        assert_eq!(squarify(&[], AREA), vec![]);
        assert_eq!(squarify(&[0.0, 0.0], AREA), vec![Rect::default(); 2]);
        let rects = squarify(&[5.0, 0.0], AREA);
        assert_eq!(rects[1], Rect::default());
        assert!((rects[0].w * rects[0].h - 240_000.0).abs() < 1.0);
    }
}
