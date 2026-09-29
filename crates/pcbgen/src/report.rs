//! Objective routing report, read straight from the saved .kicad_pcb.
//!
//! Numbers a reviewer would otherwise eyeball from a render: how much is routed, vias,
//! track length, power tracks thinner than their net class, copper inside keep-outs
//! (the antenna area), and how whole the pours are. Two items are gates: any unrouted
//! connection, and any track or via inside a keep-out. The rest is reported for
//! comparison between runs (reports/routing.json).

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::Result;
use serde_json::{Value, json};

use crate::board::{Board, ORIGIN};
use crate::geom::{Pt, area, inside, pt, seg_dist};
use crate::project::BoardRules;

fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

/// Does a round-ended segment a-b of half-width r touch polygon `poly`?
fn hits(a: Pt, b: Pt, r: f64, poly: &[Pt]) -> bool {
    if inside(a, poly) || inside(b, poly) {
        return true;
    }
    let n_edges = poly.len();
    // close enough to an edge, or crossing it: sample the segment at 0.05 mm steps
    let n = ((b - a).norm() / 0.05) as usize;
    let n = n.max(1);
    (0..=n).any(|i| {
        let p = a + (b - a) * (i as f64 / n as f64);
        inside(p, poly) || (0..n_edges).any(|k| seg_dist(p, poly[k], poly[(k + 1) % n_edges]) < r)
    })
}

pub fn build(project_dir: &Path, name: &str, rules: &BoardRules, gnd_net: &str) -> Result<Value> {
    let board = Board::load(&project_dir.join(format!("{name}.kicad_pcb")))?;
    let drc: Value = serde_json::from_str(&std::fs::read_to_string(project_dir.join("reports/drc.json"))?)?;
    let rel = |p: Pt| pt(p.x - ORIGIN.x, p.y - ORIGIN.y);

    // keep-outs: board-level rule areas and those inside footprints (antenna, sensor)
    let mut keepouts: Vec<(String, &[Pt])> =
        board.keepouts.iter().filter(|k| k.no_tracks).map(|k| (k.name.clone(), k.poly.as_slice())).collect();
    for fp in &board.footprints {
        keepouts.extend(fp.keepouts.iter().filter(|k| k.no_tracks).map(|k| ("(footprint)".to_string(), k.poly.as_slice())));
    }
    let mut in_keepout = vec![];
    for s in &board.segments {
        for (kname, poly) in &keepouts {
            if hits(s.start, s.end, s.width / 2.0, poly) {
                let a = rel(s.start);
                in_keepout.push(format!("track {} on {} at ({:.2}, {:.2}) in {kname}", s.net, s.layer, a.x, a.y));
            }
        }
    }
    for v in &board.vias {
        for (kname, poly) in &keepouts {
            if hits(v.at, v.at, v.size / 2.0, poly) {
                let a = rel(v.at);
                in_keepout.push(format!("via {} at ({:.2}, {:.2}) in {kname}", v.net, a.x, a.y));
            }
        }
    }

    // tracks thinner than their net class asks for (the router may neck down)
    let mut thin: BTreeMap<String, f64> = BTreeMap::new();
    for s in &board.segments {
        if s.width < rules.class_of(&s.net).track - 1e-6 {
            *thin.entry(s.net.clone()).or_default() += (s.end - s.start).norm();
        }
    }

    // pours (GND and any local copper zones): pieces and filled fraction per net and layer
    let mut pours = serde_json::Map::new();
    for z in &board.zones {
        let layer = z.layers.first().cloned().unwrap_or_default();
        let outline = area(&z.outline);
        let mut pieces: Vec<f64> = z.filled.iter().map(|(_, p)| area(p)).collect();
        pieces.sort_by(|a, b| b.total_cmp(a));
        let total: f64 = pieces.iter().sum();
        pours.insert(
            format!("{} {layer}", z.net),
            json!({
                "pieces": pieces.len(),
                "filled_pct": round1(100.0 * total / outline),
                "largest_piece_pct_of_fill": if pieces.is_empty() { 0.0 } else { round1(100.0 * pieces[0] / total) },
            }),
        );
    }

    let unconnected = drc["unconnected_items"].as_array().map_or(0, Vec::len);
    let mut length: BTreeMap<String, f64> = BTreeMap::new();
    for s in &board.segments {
        *length.entry(s.layer.clone()).or_default() += (s.end - s.start).norm();
    }
    let mut by_net: BTreeMap<String, usize> = BTreeMap::new();
    for v in &board.vias {
        *by_net.entry(v.net.clone()).or_default() += 1;
    }
    let n_gnd = board.vias.iter().filter(|v| v.net == gnd_net).count();
    Ok(json!({
        "unrouted_connections": unconnected,
        "track_segments": board.segments.len(),
        "track_length_mm": length.into_iter().map(|(k, v)| (k, json!(round1(v)))).collect::<serde_json::Map<_, _>>(),
        "vias_total": board.vias.len(),
        "vias_gnd": n_gnd,
        "vias_signal": board.vias.len() - n_gnd,
        "vias_by_net": by_net,
        "copper_in_keepouts": in_keepout,
        "thinner_than_class_mm": thin.into_iter().map(|(k, v)| (k, json!(round1(v)))).collect::<serde_json::Map<_, _>>(),
        "pours": pours,
    }))
}

pub fn run(project_dir: &Path, name: &str, rules: &BoardRules) -> Result<bool> {
    let r = build(project_dir, name, rules, "GND")?;
    std::fs::write(project_dir.join("reports/routing.json"), serde_json::to_string_pretty(&r)? + "\n")?;
    let lengths: Vec<String> =
        r["track_length_mm"].as_object().unwrap().iter().map(|(k, v)| format!("{k}: {v}")).collect();
    println!(
        "== ROUTING: {} unrouted, {} non-GND vias + {} GND vias, track length {{{}}} mm",
        r["unrouted_connections"],
        r["vias_signal"],
        r["vias_gnd"],
        lengths.join(", ")
    );
    for (key, p) in r["pours"].as_object().unwrap() {
        println!(
            "   pour {key}: {}% of its outline filled, {} piece(s), largest {}% of it",
            p["filled_pct"], p["pieces"], p["largest_piece_pct_of_fill"]
        );
    }
    for (net, mm) in r["thinner_than_class_mm"].as_object().unwrap() {
        println!("   note: {mm} mm of {net} track is thinner than its net class");
    }
    for item in r["copper_in_keepouts"].as_array().unwrap() {
        println!("   OPEN:   copper in keep-out: {}", item.as_str().unwrap_or(""));
    }
    let ok = r["unrouted_connections"] == 0 && r["copper_in_keepouts"].as_array().unwrap().is_empty();
    println!("== ROUTING: {} (report: reports/routing.json)", if ok { "PASS" } else { "FAIL" });
    Ok(ok)
}
