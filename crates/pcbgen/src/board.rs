//! A typed, read-only view of a saved `.kicad_pcb` (or of one footprint node): what the
//! DSN writer, escape stubs, stitching and the routing report need. Coordinates are
//! KiCad page mm (the board's top-left is at `ORIGIN`).

use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::geom::{Pt, arc_points, cos_sin, pt, rotate, round_rect_dist};
use crate::sexpr::{Sexp, parse};

/// The board's top-left corner on the KiCad page, mm.
pub const ORIGIN: Pt = pt(100.0, 100.0);

/// Arc approximation error for outlines and courtyards, mm (KiCad's default max_error).
pub const MAX_ERR: f64 = 0.005;

#[derive(Clone, Debug)]
pub struct Pad {
    pub number: String,
    /// smd, thru_hole, np_thru_hole, connect
    pub kind: String,
    /// rect, roundrect, circle, oval, trapezoid, custom
    pub shape: String,
    /// Position in the footprint's own frame.
    pub local: Pt,
    /// Absolute orientation (the footprint's rotation included), degrees.
    pub angle: f64,
    /// Orientation relative to the footprint.
    pub rel_angle: f64,
    pub size: (f64, f64),
    /// Drill (x, y); equal for a round hole.
    pub drill: Option<(f64, f64)>,
    pub layers: Vec<String>,
    pub net: Option<String>,
    pub rratio: f64,
    /// Absolute position.
    pub pos: Pt,
}

impl Pad {
    pub fn on_layer(&self, layer: &str) -> bool {
        self.layers.iter().any(|l| layer_match(l, layer))
    }

    /// Corner radius of the copper shape.
    pub fn radius(&self) -> f64 {
        let m = self.size.0.min(self.size.1);
        match self.shape.as_str() {
            "circle" | "oval" => m / 2.0,
            "roundrect" => self.rratio * m,
            _ => 0.0,
        }
    }

    /// Distance from `p` to the pad's copper (0 inside). Trapezoid and custom pads are
    /// taken as their bounding rectangle (conservative).
    pub fn dist(&self, p: Pt) -> f64 {
        let q = rotate(p - self.pos, -self.angle);
        if self.shape == "circle" {
            return (q.norm() - self.size.0 / 2.0).max(0.0);
        }
        round_rect_dist(q, self.size.0, self.size.1, self.radius())
    }
}

/// Does a layer pattern from a file ("*.Cu", "F&B.Cu", "F.Cu") cover `layer`?
pub fn layer_match(pattern: &str, layer: &str) -> bool {
    match pattern {
        "*.Cu" => layer.ends_with(".Cu"),
        "F&B.Cu" => layer == "F.Cu" || layer == "B.Cu",
        "*.Mask" => layer.ends_with(".Mask"),
        p => p == layer,
    }
}

/// A rule area (keep-out), in absolute coordinates.
#[derive(Clone, Debug)]
pub struct RuleArea {
    pub name: String,
    pub layers: Vec<String>,
    pub poly: Vec<Pt>,
    pub no_tracks: bool,
    pub no_vias: bool,
}

#[derive(Clone, Debug)]
pub struct Footprint {
    pub reference: String,
    pub value: String,
    pub lib_id: String,
    pub pos: Pt,
    pub rot: f64,
    pub layer: String,
    pub pads: Vec<Pad>,
    /// Closed courtyard outlines, absolute, both sides.
    pub courtyards: Vec<Vec<Pt>>,
    pub keepouts: Vec<RuleArea>,
}

#[derive(Clone, Debug)]
pub struct Segment {
    pub start: Pt,
    pub end: Pt,
    pub width: f64,
    pub layer: String,
    pub net: String,
    pub locked: bool,
}

#[derive(Clone, Debug)]
pub struct Via {
    pub at: Pt,
    pub size: f64,
    pub drill: f64,
    pub net: String,
    pub locked: bool,
}

#[derive(Clone, Debug)]
pub struct Zone {
    pub name: String,
    pub net: String,
    pub layers: Vec<String>,
    pub outline: Vec<Pt>,
    /// (layer, piece): KiCad saves each filled piece as one outline with its holes
    /// joined in by zero-width cuts.
    pub filled: Vec<(String, Vec<Pt>)>,
}

pub struct Board {
    pub footprints: Vec<Footprint>,
    pub segments: Vec<Segment>,
    pub vias: Vec<Via>,
    /// Copper zones (pours); rule areas are in `keepouts`.
    pub zones: Vec<Zone>,
    /// Board-level rule areas.
    pub keepouts: Vec<RuleArea>,
    /// Board outline (Edge.Cuts) as one closed polygon.
    pub outline: Vec<Pt>,
    /// Widest Edge.Cuts stroke, mm.
    pub edge_width: f64,
}

pub fn xy(n: Option<&Sexp>) -> Pt {
    n.map(|n| pt(n.num(1), n.num(2))).unwrap_or_default()
}

/// Points of a `(pts (xy ..) (arc ..))` list.
pub fn pts(n: Option<&Sexp>) -> Vec<Pt> {
    let mut out = vec![];
    for p in n.map(|n| n.items()).unwrap_or_default() {
        match p.head() {
            Some("xy") => out.push(pt(p.num(1), p.num(2))),
            Some("arc") => {
                let a = arc_points(xy(p.find("start")), xy(p.find("mid")), xy(p.find("end")), MAX_ERR);
                if out.last() == a.first() {
                    out.extend(&a[1..]);
                } else {
                    out.extend(a);
                }
            }
            _ => {}
        }
    }
    out
}

fn layers_of(n: &Sexp) -> Vec<String> {
    if let Some(ls) = n.find("layers") {
        return ls.items()[1..].iter().filter_map(|l| l.atom()).map(String::from).collect();
    }
    n.get("layer").map(|l| vec![l.to_string()]).unwrap_or_default()
}

fn rule_area(z: &Sexp, poly: Vec<Pt>) -> Option<RuleArea> {
    let ko = z.find("keepout")?;
    Some(RuleArea {
        name: z.get("name").unwrap_or("").to_string(),
        layers: layers_of(z),
        poly,
        no_tracks: ko.get("tracks") == Some("not_allowed"),
        no_vias: ko.get("vias") == Some("not_allowed"),
    })
}

/// Join open strokes (lines, arcs) into closed loops by matching end points.
pub fn chain(mut strokes: Vec<Vec<Pt>>) -> Vec<Vec<Pt>> {
    let close = |a: Pt, b: Pt| (a - b).norm() < 1e-4;
    let mut loops = vec![];
    while let Some(mut cur) = strokes.pop() {
        loop {
            let end = *cur.last().unwrap();
            if cur.len() > 2 && close(end, cur[0]) {
                cur.pop();
                break;
            }
            let Some(i) = strokes.iter().position(|s| close(s[0], end) || close(*s.last().unwrap(), end)) else {
                break; // open: treated as closed
            };
            let mut s = strokes.swap_remove(i);
            if !close(s[0], end) {
                s.reverse();
            }
            cur.extend(&s[1..]);
        }
        loops.push(cur);
    }
    loops
}

/// Closed shapes on `layer` among graphic items named `prefix_line`, `prefix_rect`, ...,
/// mapped through `f` (footprint-local → absolute).
fn shapes_on(node: &Sexp, prefix: &str, layer: &str, f: &dyn Fn(Pt) -> Pt) -> Vec<Vec<Pt>> {
    let mut closed = vec![];
    let mut strokes = vec![];
    for g in node.items() {
        let Some(h) = g.head() else { continue };
        let Some(kind) = h.strip_prefix(prefix) else { continue };
        if g.get("layer") != Some(layer) {
            continue;
        }
        match kind {
            "line" => strokes.push(vec![f(xy(g.find("start"))), f(xy(g.find("end")))]),
            "arc" => strokes.push(
                arc_points(xy(g.find("start")), xy(g.find("mid")), xy(g.find("end")), MAX_ERR).into_iter().map(f).collect(),
            ),
            "rect" => {
                let (a, b) = (xy(g.find("start")), xy(g.find("end")));
                closed.push([a, pt(b.x, a.y), b, pt(a.x, b.y)].into_iter().map(f).collect());
            }
            "poly" => closed.push(pts(g.find("pts")).into_iter().map(f).collect()),
            "circle" => {
                let (c, e) = (xy(g.find("center")), xy(g.find("end")));
                let r = (e - c).norm();
                let n = 36;
                closed.push(
                    (0..n)
                        .map(|i| {
                            let (co, si) = cos_sin(i as f64 * 360.0 / n as f64);
                            f(c + pt(r * co, r * si))
                        })
                        .collect(),
                );
            }
            _ => {}
        }
    }
    closed.extend(chain(strokes));
    closed
}

impl Footprint {
    /// Parse a placed footprint node (from a board, or built by the PCB writer).
    pub fn from_node(n: &Sexp) -> Result<Footprint> {
        let at = n.find("at");
        let pos = xy(at);
        let rot = at.map_or(0.0, |a| a.num(3));
        let to_abs = |p: Pt| pos + rotate(p, rot);
        let prop = |key: &str| {
            n.find_all("property").find(|p| p.arg(1) == Some(key)).and_then(|p| p.arg(2)).unwrap_or("").to_string()
        };
        let reference = prop("Reference");
        let mut pads = vec![];
        for p in n.find_all("pad") {
            let pat = p.find("at");
            let local = xy(pat);
            let angle = pat.map_or(0.0, |a| a.num(3));
            let size = p.find("size").map_or((0.0, 0.0), |s| (s.num(1), s.num(2)));
            let drill = p.find("drill").map(|d| {
                let nums: Vec<f64> = d.items()[1..].iter().filter_map(|x| match x {
                    Sexp::Sym(s) => s.parse().ok(),
                    _ => None,
                }).collect();
                match nums.as_slice() {
                    [a] => (*a, *a),
                    [a, b, ..] => (*a, *b),
                    [] => (0.0, 0.0),
                }
            });
            pads.push(Pad {
                number: p.arg(1).unwrap_or("").to_string(),
                kind: p.arg(2).unwrap_or("").to_string(),
                shape: p.arg(3).unwrap_or("").to_string(),
                local,
                angle,
                rel_angle: angle - rot,
                size,
                drill,
                layers: layers_of(p),
                net: p.find("net").and_then(|x| x.items().last()).and_then(Sexp::atom).map(String::from),
                rratio: p.find("roundrect_rratio").map_or(0.0, |r| r.num(1)),
                pos: to_abs(local),
            });
        }
        let mut courtyards = shapes_on(n, "fp_", "F.CrtYd", &to_abs);
        courtyards.extend(shapes_on(n, "fp_", "B.CrtYd", &to_abs));
        // zones inside a footprint are stored in board coordinates already
        let keepouts = n.find_all("zone").filter_map(|z| rule_area(z, pts(z.find("polygon").and_then(|p| p.find("pts"))))).collect();
        Ok(Footprint {
            reference,
            value: prop("Value"),
            lib_id: n.arg(1).unwrap_or("").to_string(),
            pos,
            rot,
            layer: n.get("layer").unwrap_or("F.Cu").to_string(),
            pads,
            courtyards,
            keepouts,
        })
    }
}

fn net_of(n: &Sexp) -> String {
    // KiCad 10 writes (net "NAME"); older files (net 3 "NAME")
    n.find("net").and_then(|x| x.items().last()).and_then(Sexp::atom).unwrap_or("").to_string()
}

impl Board {
    pub fn load(path: &Path) -> Result<Board> {
        let text = std::fs::read_to_string(path).with_context(|| path.display().to_string())?;
        Board::from_sexp(&parse(&text)?)
    }

    pub fn from_sexp(root: &Sexp) -> Result<Board> {
        if !root.is("kicad_pcb") {
            bail!("not a kicad_pcb file");
        }
        let footprints = root.find_all("footprint").map(Footprint::from_node).collect::<Result<Vec<_>>>()?;
        let segments = root
            .find_all("segment")
            .map(|s| Segment {
                start: xy(s.find("start")),
                end: xy(s.find("end")),
                width: s.find("width").map_or(0.0, |w| w.num(1)),
                layer: s.get("layer").unwrap_or("").to_string(),
                net: net_of(s),
                locked: s.flag("locked"),
            })
            .collect();
        let vias = root
            .find_all("via")
            .map(|v| Via {
                at: xy(v.find("at")),
                size: v.find("size").map_or(0.0, |w| w.num(1)),
                drill: v.find("drill").map_or(0.0, |w| w.num(1)),
                net: net_of(v),
                locked: v.flag("locked"),
            })
            .collect();
        let mut zones = vec![];
        let mut keepouts = vec![];
        for z in root.find_all("zone") {
            let outline = pts(z.find("polygon").and_then(|p| p.find("pts")));
            if let Some(ra) = rule_area(z, outline.clone()) {
                keepouts.push(ra);
                continue;
            }
            let filled = z
                .find_all("filled_polygon")
                .map(|f| (f.get("layer").unwrap_or("").to_string(), pts(f.find("pts"))))
                .collect();
            zones.push(Zone { name: z.get("name").unwrap_or("").to_string(), net: net_of(z), layers: layers_of(z), outline, filled });
        }
        let edges = shapes_on(root, "gr_", "Edge.Cuts", &|p| p);
        let outline = edges.into_iter().max_by(|a, b| crate::geom::area(a).total_cmp(&crate::geom::area(b))).unwrap_or_default();
        let edge_width = root
            .items()
            .iter()
            .filter(|g| g.head().is_some_and(|h| h.starts_with("gr_")) && g.get("layer") == Some("Edge.Cuts"))
            .map(|g| g.find("stroke").and_then(|s| s.find("width")).map_or(0.0, |w| w.num(1)))
            .fold(0.0, f64::max);
        Ok(Board { footprints, segments, vias, zones, keepouts, outline, edge_width })
    }

    /// Bounding box of the board edges, strokes included (as KiCad's
    /// GetBoardEdgesBoundingBox).
    pub fn edge_bbox(&self) -> (Pt, Pt) {
        let (lo, hi) = crate::geom::bbox(&self.outline);
        let h = self.edge_width / 2.0;
        (lo - pt(h, h), hi + pt(h, h))
    }

    /// Every plated or unplated hole: (centre, radius).
    pub fn holes(&self) -> Vec<(Pt, f64)> {
        let mut h: Vec<(Pt, f64)> = self.vias.iter().map(|v| (v.at, v.drill / 2.0)).collect();
        for fp in &self.footprints {
            for p in &fp.pads {
                if let Some((dx, dy)) = p.drill.filter(|d| d.0 > 0.0) {
                    h.push((p.pos, dx.max(dy) / 2.0));
                }
            }
        }
        h
    }
}
