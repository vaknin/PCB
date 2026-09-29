//! Stitching vias for copper pours, placed after routing on the filled board.
//!
//! Freerouting doesn't stitch, and a pour cut in two by tracks leaves an isolated piece
//! (DRC: unconnected zone). A via goes on a grid wherever it fits entirely inside the
//! net's *filled* copper on both outer layers (so the fill has already kept it clear of
//! other nets), outside every courtyard (so it stays out of pads) and clear of other
//! holes; then one via per remaining pour piece.

use crate::board::Board;
use crate::geom::{Pt, bbox, edge_dist, inside, pt};
use crate::layout::RouteOptions;

/// Integer nm grid points in [lo, hi), starting half a step in (as the Python pipeline).
fn grid(lo: Pt, hi: Pt, step: f64) -> Vec<Pt> {
    let nm = |v: f64| (v * 1e6).round() as i64;
    let s = nm(step);
    let mut out = vec![];
    let mut x = nm(lo.x) + s / 2;
    while x < nm(hi.x) {
        let mut y = nm(lo.y) + s / 2;
        while y < nm(hi.y) {
            out.push(pt(x as f64 / 1e6, y as f64 / 1e6));
            y += s;
        }
        x += s;
    }
    out
}

/// Via positions for `net` (not yet added to the board).
pub fn stitch(board: &Board, opts: &RouteOptions, net: &str, pitch: f64) -> Vec<Pt> {
    let (dia, drill) = opts.stitch_via;
    let zones: Vec<_> = board.zones.iter().filter(|z| z.net == net).collect();
    // filled pieces per layer; a via must sit inside one on every layer the net pours on
    let mut per_layer: Vec<(String, Vec<&[Pt]>)> = vec![];
    for z in &zones {
        let layer = z.layers.first().cloned().unwrap_or_default();
        let pieces: Vec<&[Pt]> = z.filled.iter().filter(|(l, _)| *l == layer).map(|(_, p)| p.as_slice()).collect();
        match per_layer.iter_mut().find(|(l, _)| *l == layer) {
            Some((_, v)) => v.extend(pieces),
            None => per_layer.push((layer, pieces)),
        }
    }
    let has = |l: &str| per_layer.iter().any(|(x, _)| x == l);
    if !(has("F.Cu") && has("B.Cu")) {
        return vec![];
    }
    let inset = dia / 2.0 + 0.05;
    let courtyards: Vec<&[Pt]> = board.footprints.iter().flat_map(|f| f.courtyards.iter().map(Vec::as_slice)).collect();
    let pads: Vec<_> = board.footprints.iter().flat_map(|f| &f.pads).collect();
    let pad_gap = dia / 2.0 + 0.2;
    // the fill alone once didn't keep a via clear of a track (0.185 mm to /EN, rule 0.2):
    // check other-net copper directly, at the largest class clearance plus a margin
    let track_gap = dia / 2.0 + opts.stitch_clearance;
    let others: Vec<_> = board.segments.iter().filter(|s| s.net != net).collect();
    let other_vias: Vec<_> = board.vias.iter().filter(|v| v.net != net).collect();
    let mut holes = board.holes();
    let hole_gap = 0.5;

    let fits = |p: Pt, holes: &[(Pt, f64)], in_courtyard: bool| -> bool {
        per_layer.iter().all(|(_, pieces)| pieces.iter().any(|poly| inside(p, poly) && edge_dist(p, poly) >= inset))
            && if in_courtyard {
                // allowed inside a part's courtyard if clear of all its pads (a tented
                // via under a part body is fine; a via in a pad wicks solder)
                !pads.iter().any(|pad| pad.dist(p) <= pad_gap)
            } else {
                !courtyards.iter().any(|c| inside(p, c) || edge_dist(p, c) <= dia / 2.0)
            }
            && !holes.iter().any(|&(hp, hr)| (p - hp).norm() < hr + drill / 2.0 + hole_gap)
            && !others.iter().any(|t| crate::geom::seg_dist(p, t.start, t.end) <= t.width / 2.0 + track_gap)
            && !other_vias.iter().any(|v| (p - v.at).norm() <= v.size / 2.0 + track_gap)
    };

    let mut added = vec![];
    let (lo, hi) = board.edge_bbox();
    for p in grid(lo, hi, pitch) {
        if fits(p, &holes, false) {
            holes.push((p, drill / 2.0));
            added.push(p);
        }
    }

    // Pour pieces the grid missed (narrow slivers between tracks): one via each, found
    // by a fine search inside that piece, nearest its centre.
    for z in &zones {
        let layer = z.layers.first().cloned().unwrap_or_default();
        for (_, piece) in z.filled.iter().filter(|(l, _)| *l == layer) {
            if holes.iter().any(|(hp, _)| inside(*hp, piece)) {
                continue;
            }
            let (plo, phi) = bbox(piece);
            let centre = pt((plo.x + phi.x) / 2.0, (plo.y + phi.y) / 2.0);
            let mut spots: Vec<Pt> = grid(plo, phi, 0.25).into_iter().filter(|p| inside(*p, piece)).collect();
            spots.sort_by(|a, b| (*a - centre).norm().total_cmp(&(*b - centre).norm()));
            let spot = spots.iter().find(|p| fits(**p, &holes, false)).or_else(|| spots.iter().find(|p| fits(**p, &holes, true)));
            match spot {
                Some(&p) => {
                    holes.push((p, drill / 2.0));
                    added.push(p);
                }
                None => println!(
                    "stitching: no room for a via in a {} piece near ({:.1}, {:.1}) mm",
                    z.name,
                    centre.x - crate::board::ORIGIN.x,
                    centre.y - crate::board::ORIGIN.y
                ),
            }
        }
    }
    added
}
