//! Plane geometry in mm, KiCad's frame: Y points down, angles are degrees CCW as seen
//! on screen.

use std::ops::{Add, Mul, Sub};

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Pt {
    pub x: f64,
    pub y: f64,
}

pub const fn pt(x: f64, y: f64) -> Pt {
    Pt { x, y }
}

impl Add for Pt {
    type Output = Pt;
    fn add(self, o: Pt) -> Pt {
        pt(self.x + o.x, self.y + o.y)
    }
}
impl Sub for Pt {
    type Output = Pt;
    fn sub(self, o: Pt) -> Pt {
        pt(self.x - o.x, self.y - o.y)
    }
}
impl Mul<f64> for Pt {
    type Output = Pt;
    fn mul(self, k: f64) -> Pt {
        pt(self.x * k, self.y * k)
    }
}

impl Pt {
    pub fn norm(self) -> f64 {
        self.x.hypot(self.y)
    }
}

/// cos and sin of an angle in degrees, exact for multiples of 90.
pub fn cos_sin(deg: f64) -> (f64, f64) {
    let a = deg.rem_euclid(360.0);
    match a {
        0.0 => (1.0, 0.0),
        90.0 => (0.0, 1.0),
        180.0 => (-1.0, 0.0),
        270.0 => (0.0, -1.0),
        _ => (a.to_radians().cos(), a.to_radians().sin()),
    }
}

/// Rotate a footprint-frame point by `deg` into the board frame (CCW on screen, Y down):
/// (x·cos + y·sin, −x·sin + y·cos).
pub fn rotate(p: Pt, deg: f64) -> Pt {
    let (c, s) = cos_sin(deg);
    pt(p.x * c + p.y * s, -p.x * s + p.y * c)
}

/// Angle normalised to [0, 360).
pub fn norm360(deg: f64) -> f64 {
    let a = deg.rem_euclid(360.0);
    if (a - 360.0).abs() < 1e-9 { 0.0 } else { a }
}

/// Angle normalised to (-180, 180], as KiCad writes footprint orientations.
pub fn norm180(deg: f64) -> f64 {
    let a = norm360(deg);
    if a > 180.0 { a - 360.0 } else { a }
}

/// Distance from `p` to the segment a-b.
pub fn seg_dist(p: Pt, a: Pt, b: Pt) -> f64 {
    let d = b - a;
    let len2 = d.x * d.x + d.y * d.y;
    let t = if len2 == 0.0 { 0.0 } else { (((p - a).x * d.x + (p - a).y * d.y) / len2).clamp(0.0, 1.0) };
    (p - (a + d * t)).norm()
}

/// Even-odd point-in-polygon (the polygon is implicitly closed).
pub fn inside(p: Pt, poly: &[Pt]) -> bool {
    let mut c = false;
    let n = poly.len();
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        if (a.y > p.y) != (b.y > p.y) && p.x < a.x + (p.y - a.y) * (b.x - a.x) / (b.y - a.y) {
            c = !c;
        }
    }
    c
}

/// Distance from `p` to the nearest edge of a closed polygon.
pub fn edge_dist(p: Pt, poly: &[Pt]) -> f64 {
    let n = poly.len();
    (0..n).map(|i| seg_dist(p, poly[i], poly[(i + 1) % n])).fold(f64::INFINITY, f64::min)
}

/// Does a circle of radius `r` at `p` touch the polygon (inside, or within r of an edge)?
pub fn collides(p: Pt, r: f64, poly: &[Pt]) -> bool {
    inside(p, poly) || edge_dist(p, poly) <= r
}

pub fn area(poly: &[Pt]) -> f64 {
    let n = poly.len();
    (0..n).map(|i| poly[i].x * poly[(i + 1) % n].y - poly[(i + 1) % n].x * poly[i].y).sum::<f64>().abs() / 2.0
}

/// (min, max) corners.
pub fn bbox(poly: &[Pt]) -> (Pt, Pt) {
    let mut lo = pt(f64::INFINITY, f64::INFINITY);
    let mut hi = pt(f64::NEG_INFINITY, f64::NEG_INFINITY);
    for p in poly {
        lo = pt(lo.x.min(p.x), lo.y.min(p.y));
        hi = pt(hi.x.max(p.x), hi.y.max(p.y));
    }
    (lo, hi)
}

/// Points along the circular arc start → mid → end, at most `max_err` mm from the true
/// arc (end points included).
pub fn arc_points(start: Pt, mid: Pt, end: Pt, max_err: f64) -> Vec<Pt> {
    // circumcentre
    let (ax, ay, bx, by, cx, cy) = (start.x, start.y, mid.x, mid.y, end.x, end.y);
    let d = 2.0 * (ax * (by - cy) + bx * (cy - ay) + cx * (ay - by));
    if d.abs() < 1e-12 {
        return vec![start, end];
    }
    let sq = |x: f64, y: f64| x * x + y * y;
    let ux = (sq(ax, ay) * (by - cy) + sq(bx, by) * (cy - ay) + sq(cx, cy) * (ay - by)) / d;
    let uy = (sq(ax, ay) * (cx - bx) + sq(bx, by) * (ax - cx) + sq(cx, cy) * (bx - ax)) / d;
    let c = pt(ux, uy);
    let r = (start - c).norm();
    let ang = |p: Pt| (p.y - c.y).atan2(p.x - c.x);
    let (a0, am, a1) = (ang(start), ang(mid), ang(end));
    // sweep from a0 to a1 through am
    let tau = std::f64::consts::TAU;
    let ccw = |from: f64, to: f64| (to - from).rem_euclid(tau);
    let sweep = if ccw(a0, am) <= ccw(a0, a1) { ccw(a0, a1) } else { ccw(a0, a1) - tau };
    let step = 2.0 * (1.0 - max_err / r).clamp(-1.0, 1.0).acos();
    let n = ((sweep.abs() / step).ceil() as usize).max(1);
    (0..=n).map(|i| {
        let a = a0 + sweep * i as f64 / n as f64;
        pt(c.x + r * a.cos(), c.y + r * a.sin())
    }).collect()
}

/// Convex hull (Andrew's monotone chain), counter-clockwise in a Y-up frame; collinear
/// points dropped.
pub fn convex_hull(pts: &[Pt]) -> Vec<Pt> {
    let mut p = pts.to_vec();
    p.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    p.dedup();
    if p.len() < 3 {
        return p;
    }
    let cross = |o: Pt, a: Pt, b: Pt| (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x);
    let mut hull: Vec<Pt> = Vec::with_capacity(p.len() + 1);
    for pass in 0..2 {
        let start = hull.len();
        let iter: Box<dyn Iterator<Item = &Pt>> = if pass == 0 { Box::new(p.iter()) } else { Box::new(p.iter().rev()) };
        for &q in iter {
            while hull.len() >= start + 2 && cross(hull[hull.len() - 2], hull[hull.len() - 1], q) <= 0.0 {
                hull.pop();
            }
            hull.push(q);
        }
        hull.pop(); // the last point starts the other chain
    }
    hull
}

/// A regular polygon *around* the circle (its sides touch it), so it covers the disc, with
/// its corners at most `max_err` outside the circle.
pub fn disc(c: Pt, r: f64, max_err: f64) -> Vec<Pt> {
    let sides = ((std::f64::consts::PI / (r / (r + max_err)).acos()).ceil() as usize).max(8);
    let ro = r / (std::f64::consts::PI / sides as f64).cos();
    (0..sides)
        .map(|i| {
            let a = std::f64::consts::TAU * (i as f64 + 0.5) / sides as f64;
            c + pt(ro * a.cos(), ro * a.sin())
        })
        .collect()
}

/// Distance from `p` (in the shape's own frame, centred) to a rounded rectangle of size
/// w × h with corner radius `r` (0 = rectangle, min(w,h)/2 = oval). 0 inside.
pub fn round_rect_dist(p: Pt, w: f64, h: f64, r: f64) -> f64 {
    let qx = p.x.abs() - (w / 2.0 - r);
    let qy = p.y.abs() - (h / 2.0 - r);
    let outside = pt(qx.max(0.0), qy.max(0.0)).norm();
    (outside - r).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotation_matches_kicad() {
        // SHT40 pad 1 at local (-0.7, -0.4), footprint at 270: board offset (0.4, -0.7)
        let p = rotate(pt(-0.7, -0.4), 270.0);
        assert!((p.x - 0.4).abs() < 1e-12 && (p.y + 0.7).abs() < 1e-12, "{p:?}");
    }

    #[test]
    fn arcs() {
        let pts = arc_points(pt(1.0, 0.0), pt(0.0, 1.0), pt(-1.0, 0.0), 0.001);
        assert!(pts.iter().all(|p| (p.norm() - 1.0).abs() < 1e-9 && p.y >= -1e-9));
        assert!(pts.len() > 10);
    }

    #[test]
    fn hull() {
        let pts = [pt(0.0, 0.0), pt(2.0, 0.0), pt(1.0, 1.0), pt(2.0, 2.0), pt(0.0, 2.0), pt(1.0, 0.0), pt(0.0, 0.0)];
        let h = convex_hull(&pts);
        assert_eq!(h.len(), 4);
        assert_eq!(area(&h), 4.0);
        let d = disc(pt(1.0, 1.0), 1.0, 0.005);
        assert!(d.iter().all(|q| (1.0..=1.005 + 1e-12).contains(&(*q - pt(1.0, 1.0)).norm())));
        assert!(edge_dist(pt(1.0, 1.0), &d) >= 1.0 - 1e-12);
    }

    #[test]
    fn shapes() {
        let sq = [pt(0.0, 0.0), pt(2.0, 0.0), pt(2.0, 2.0), pt(0.0, 2.0)];
        assert!(inside(pt(1.0, 1.0), &sq) && !inside(pt(3.0, 1.0), &sq));
        assert_eq!(edge_dist(pt(1.0, 1.0), &sq), 1.0);
        assert_eq!(area(&sq), 4.0);
        assert_eq!(round_rect_dist(pt(2.0, 0.0), 2.0, 1.0, 0.0), 1.0);
        assert_eq!(round_rect_dist(pt(0.1, 0.1), 2.0, 1.0, 0.25), 0.0);
    }
}
