//! Specctra DSN for Freerouting, written from a saved board (kicad-cli can't export it,
//! and KiCad 11 drops the SWIG module that could).
//!
//! Conventions follow KiCad's own exporter: µm, Y pointing up, `(resolution um 10)`; one
//! image per footprint shape with its pins, padstacks and keep-outs in the footprint's
//! frame; NPTH holes as keep-outs of drill + 2 × hole clearance; one class per net
//! class with its via; locked tracks as `(type fix)` wires. Copper pours are never
//! written: as planes they make Freerouting treat every pad of their net as connected,
//! so it routes none of them (D-012).

use std::collections::HashMap;
use std::fmt::Write as _;

use anyhow::{Result, bail};

use crate::board::{Board, Footprint, Pad, RuleArea};
use crate::geom::{Pt, cos_sin, norm180, norm360, pt, rotate};
use crate::project::BoardRules;
use crate::sexpr::fmt_num;

const LAYERS: [&str; 2] = ["F.Cu", "B.Cu"];

/// mm → µm text.
fn um(v: f64) -> String {
    fmt_num(v * 1000.0)
}

/// A board point as DSN "x y" (µm, Y up).
fn xy(p: Pt) -> String {
    format!("{} {}", um(p.x), um(-p.y))
}

/// Quote a name the way KiCad's exporter does: when it holds a separator or a '-'.
fn q(s: &str) -> String {
    if s.is_empty() || s.chars().any(|c| c.is_whitespace() || matches!(c, '(' | ')' | '-')) {
        format!("\"{s}\"")
    } else {
        s.to_string()
    }
}

/// Pin references ("U1-41@1") are split on their first '-' by the reader, so they are
/// never quoted for a '-'.
fn q_pin(s: &str) -> String {
    if s.chars().any(|c| c.is_whitespace() || matches!(c, '(' | ')')) { format!("\"{s}\"") } else { s.to_string() }
}

pub fn via_name(dia: f64, drill: f64) -> String {
    format!("Via[0-1]_{}:{}_um", um(dia), um(drill))
}

fn closed(pts: &[Pt]) -> Vec<Pt> {
    let mut v = pts.to_vec();
    if v.len() > 1 && v.first() != v.last() {
        v.push(v[0]);
    }
    v
}

fn coords(pts: &[Pt]) -> String {
    pts.iter().map(|p| xy(*p)).collect::<Vec<_>>().join("  ")
}

/// Padstack name and body for a pad, or None for a pad with no copper.
fn padstack(pad: &Pad) -> Result<Option<(String, String)>> {
    let layers: Vec<&str> = LAYERS.iter().copied().filter(|l| pad.on_layer(l)).collect();
    let tag = match layers.as_slice() {
        [] => return Ok(None),
        ["F.Cu"] => "T",
        ["B.Cu"] => "B",
        _ => "A",
    };
    let (w, h) = pad.size;
    let shape = match pad.shape.as_str() {
        "oval" if w == h => "circle",
        "roundrect" if pad.radius() <= 0.0 => "rect",
        s => s,
    };
    // the shape with "{L}" standing for the layer
    let (name, body): (String, String) = match shape {
        "circle" => (format!("Round[{tag}]Pad_{}_um", um(w)), format!("(circle {{L}} {})", um(w))),
        "rect" => (
            format!("Rect[{tag}]Pad_{}x{}_um", um(w), um(h)),
            format!("(rect {{L}} {} {} {} {})", um(-w / 2.0), um(-h / 2.0), um(w / 2.0), um(h / 2.0)),
        ),
        "oval" => {
            let (d, half) = (w.min(h), (w - h).abs() / 2.0);
            let (a, b) = if w > h { (pt(-half, 0.0), pt(half, 0.0)) } else { (pt(0.0, -half), pt(0.0, half)) };
            (
                format!("Oval[{tag}]Pad_{}x{}_um", um(w), um(h)),
                format!("(path {{L}} {}  {} {}  {} {})", um(d), um(a.x), um(a.y), um(b.x), um(b.y)),
            )
        }
        "roundrect" => {
            let r = pad.radius();
            // each corner arc as edges tangent to it, at most 1 µm outside: the polygon
            // covers all the copper (as KiCad's does) and its sides stay at the pad size
            let err = 0.001;
            let steps = ((std::f64::consts::FRAC_PI_2 / (2.0 * (1.0 - err / r).clamp(-1.0, 1.0).acos())).ceil() as usize).max(1);
            let step = 90.0 / steps as f64;
            let ro = r / (step / 2.0).to_radians().cos();
            let mut pts = vec![];
            for (k, (sx, sy)) in [(1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)].into_iter().enumerate() {
                let c = pt(sx * (w / 2.0 - r), sy * (h / 2.0 - r));
                let a0 = 90.0 * k as f64;
                let at = |a: f64, rad: f64| {
                    let (co, si) = cos_sin(a);
                    c + pt(rad * co, rad * si)
                };
                pts.push(at(a0, r));
                for i in 0..steps {
                    pts.push(at(a0 + (i as f64 + 0.5) * step, ro));
                }
                pts.push(at(a0 + 90.0, r));
            }
            pts.push(pts[0]);
            let text = pts.iter().map(|p| format!("{} {}", um(p.x), um(p.y))).collect::<Vec<_>>().join("  ");
            (
                format!("RoundRect[{tag}]Pad_{}x{}_{}_um", um(w), um(h), um(r)),
                format!("(polygon {{L}} 0  {text})"),
            )
        }
        s => bail!("pad shape {s:?} is not supported by the DSN writer yet"),
    };
    let mut out = String::new();
    for l in &layers {
        writeln!(out, "      (shape {})", body.replace("{L}", l))?;
    }
    out.push_str("      (attach off)\n");
    Ok(Some((name, out)))
}

fn keepout_kind(k: &RuleArea) -> Option<&'static str> {
    match (k.no_tracks, k.no_vias) {
        (true, true) => Some("keepout"),
        (true, false) => Some("wire_keepout"),
        (false, true) => Some("via_keepout"),
        (false, false) => None,
    }
}

fn keepout_layers(k: &RuleArea) -> Vec<&'static str> {
    LAYERS.iter().copied().filter(|l| k.layers.iter().any(|p| crate::board::layer_match(p, l))).collect()
}

/// Pin names per footprint pad: repeated pad numbers get "@1", "@2", ... in file order,
/// as KiCad's exporter names them. None for pads that aren't pins (NPTH, no copper).
pub fn pin_names(fp: &Footprint) -> Vec<Option<String>> {
    let mut seen: HashMap<&str, usize> = HashMap::new();
    fp.pads
        .iter()
        .map(|p| {
            if p.kind == "np_thru_hole" || !LAYERS.iter().any(|l| p.on_layer(l)) {
                return None;
            }
            let n = seen.entry(&p.number).or_insert(0);
            let name = if *n == 0 { p.number.clone() } else { format!("{}@{}", p.number, n) };
            *n += 1;
            Some(name)
        })
        .collect()
}

/// Footprint order for the DSN: file order for `seed` 0, else a shuffle seeded by it.
///
/// Freerouting is deterministic: the same DSN gives the same routing, and the order of
/// footprints (hence of images, nets and pins) steers it. The Python pipeline's random
/// footprint UUIDs reshuffled that order on every run, which is why its results
/// "varied run to run"; here a retry asks for a different order on purpose.
fn footprint_order(n: usize, seed: u64) -> Vec<usize> {
    let mut order: Vec<usize> = (0..n).collect();
    if seed == 0 {
        return order;
    }
    let mut x = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1; // xorshift64
    for i in (1..n).rev() {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        order.swap(i, (x % (i as u64 + 1)) as usize);
    }
    order
}

/// Write the DSN. Returns via padstack name → (diameter, drill) mm, to read the SES back.
/// `seed` 0 keeps the board's footprint order; any other value shuffles it (retries).
pub fn write(board: &Board, rules: &BoardRules, hole_clearance: f64, seed: u64, path: &std::path::Path) -> Result<HashMap<String, (f64, f64)>> {
    let footprints: Vec<&Footprint> = footprint_order(board.footprints.len(), seed).into_iter().map(|i| &board.footprints[i]).collect();
    let mut o = String::new();
    writeln!(o, "(pcb {}", q(&path.to_string_lossy()))?;
    o.push_str("  (parser\n    (string_quote \")\n    (space_in_quoted_tokens on)\n    (host_cad \"pcbgen\")\n    (host_version \"0.2\")\n  )\n");
    o.push_str("  (resolution um 10)\n  (unit um)\n  (structure\n");
    for (i, l) in LAYERS.iter().enumerate() {
        writeln!(o, "    (layer {l}\n      (type signal)\n      (property\n        (index {i})\n      )\n    )")?;
    }
    if board.outline.len() < 3 {
        bail!("the board has no closed Edge.Cuts outline");
    }
    writeln!(o, "    (boundary\n      (path pcb 0  {})\n    )", coords(&closed(&board.outline)))?;
    for k in &board.keepouts {
        let Some(kind) = keepout_kind(k) else { continue };
        for l in keepout_layers(k) {
            writeln!(o, "    ({kind} {} (polygon {l} 0  {}))", q(&k.name), coords(&closed(&k.poly)))?;
        }
    }

    // vias: the default class's first, then each other class's
    let default = rules.classes.iter().find(|c| c.name == "Default").unwrap_or(&rules.classes[0]);
    let mut vias: Vec<(String, f64, f64)> = vec![];
    for c in std::iter::once(default).chain(rules.classes.iter()) {
        let n = via_name(c.via_diameter, c.via_drill);
        if !vias.iter().any(|v| v.0 == n) {
            vias.push((n, c.via_diameter, c.via_drill));
        }
    }
    writeln!(o, "    (via {})", vias.iter().map(|v| q(&v.0)).collect::<Vec<_>>().join(" "))?;
    writeln!(
        o,
        "    (rule\n      (width {})\n      (clearance {})\n      (clearance {} (type smd_smd))\n    )\n  )",
        um(default.track),
        um(default.clearance),
        um(default.clearance / 4.0)
    )?;

    // images: one per distinct footprint shape
    let mut padstacks: Vec<(String, String)> = vec![];
    let mut images: Vec<(String, String)> = vec![]; // (name, body)
    let mut image_of: Vec<String> = vec![];
    for fp in &footprints {
        if fp.layer != "F.Cu" {
            bail!("{}: bottom-side footprints are not supported by the DSN writer yet", fp.reference);
        }
        let mut body = String::new();
        for (pad, name) in fp.pads.iter().zip(pin_names(fp)) {
            let Some(name) = name else { continue };
            let Some((ps, ps_body)) = padstack(pad)? else { continue };
            if !padstacks.iter().any(|p| p.0 == ps) {
                padstacks.push((ps.clone(), ps_body));
            }
            let rot = norm360(pad.rel_angle);
            let rot = if rot == 0.0 { String::new() } else { format!(" (rotate {})", fmt_num(rot)) };
            writeln!(body, "      (pin {}{rot} {} {} {})", q(&ps), q_pin(&name), um(pad.local.x), um(-pad.local.y))?;
        }
        let local = |p: Pt| rotate(p - fp.pos, -fp.rot);
        for k in &fp.keepouts {
            let Some(kind) = keepout_kind(k) else { continue };
            let poly: Vec<Pt> = closed(&k.poly).into_iter().map(local).collect();
            for l in keepout_layers(k) {
                writeln!(body, "      ({kind} \"\" (polygon {l} 0  {}))", coords(&poly))?;
            }
        }
        for pad in fp.pads.iter().filter(|p| p.kind == "np_thru_hole") {
            let d = pad.drill.map_or(pad.size.0, |d| d.0.max(d.1)) + 2.0 * hole_clearance;
            let at = if pad.local == pt(0.0, 0.0) { String::new() } else { format!(" {} {}", um(pad.local.x), um(-pad.local.y)) };
            for l in LAYERS {
                writeln!(body, "      (keepout \"\" (circle {l} {}{at}))", um(d))?;
            }
        }
        let name = match images.iter().find(|(n, b)| b == &body && (n == &fp.lib_id || n.starts_with(&format!("{}::", fp.lib_id)))) {
            Some((n, _)) => n.clone(),
            None => {
                let k = images.iter().filter(|(n, _)| n == &fp.lib_id || n.starts_with(&format!("{}::", fp.lib_id))).count();
                let n = if k == 0 { fp.lib_id.clone() } else { format!("{}::{k}", fp.lib_id) };
                images.push((n.clone(), body));
                n
            }
        };
        image_of.push(name);
    }

    o.push_str("  (placement\n");
    for (img, _) in &images {
        writeln!(o, "    (component {}", q(img))?;
        for (fp, _) in footprints.iter().zip(&image_of).filter(|(_, i)| *i == img) {
            let p = fp.pos;
            writeln!(o, "      (place {} {} front {} (PN {}))", q(&fp.reference), xy(p), fmt_num(norm180(fp.rot)), q(&fp.value))?;
        }
        o.push_str("    )\n");
    }
    o.push_str("  )\n  (library\n");
    for (img, body) in &images {
        write!(o, "    (image {}\n{body}    )\n", q(img))?;
    }
    for (ps, body) in &padstacks {
        write!(o, "    (padstack {}\n{body}    )\n", q(ps))?;
    }
    let mut via_table = HashMap::new();
    for (n, d, h) in &vias {
        writeln!(o, "    (padstack {}\n      (shape (circle F.Cu {}))\n      (shape (circle B.Cu {}))\n      (attach off)\n    )", q(n), um(*d), um(*d))?;
        via_table.insert(n.clone(), (*d, *h));
    }
    o.push_str("  )\n  (network\n");

    // nets in the order their first pad appears
    let mut nets: Vec<(String, Vec<String>)> = vec![];
    for fp in &footprints {
        for (pad, name) in fp.pads.iter().zip(pin_names(fp)) {
            let (Some(name), Some(net)) = (name, pad.net.as_ref()) else { continue };
            let pin = format!("{}-{name}", fp.reference);
            match nets.iter_mut().find(|n| &n.0 == net) {
                Some(n) => n.1.push(pin),
                None => nets.push((net.clone(), vec![pin])),
            }
        }
    }
    for (net, pins) in &nets {
        writeln!(o, "    (net {}\n      (pins {})\n    )", q(net), pins.iter().map(|p| q_pin(p)).collect::<Vec<_>>().join(" "))?;
    }
    for c in &rules.classes {
        let mut members: Vec<&str> = nets.iter().map(|n| n.0.as_str()).filter(|n| rules.class_of(n).name == c.name).collect();
        if members.is_empty() {
            continue;
        }
        members.sort();
        let cname = if c.name == "Default" { "kicad_default".to_string() } else { c.name.clone() };
        writeln!(
            o,
            "    (class {} {}\n      (circuit\n        (use_via {})\n      )\n      (rule\n        (width {})\n        (clearance {})\n      )\n    )",
            q(&cname),
            members.iter().map(|n| q(n)).collect::<Vec<_>>().join(" "),
            q(&via_name(c.via_diameter, c.via_drill)),
            um(c.track),
            um(c.clearance)
        )?;
    }
    o.push_str("  )\n  (wiring\n");
    for s in board.segments.iter().filter(|s| s.locked) {
        writeln!(o, "    (wire (path {} {}  {})(net {})(type fix))", s.layer, um(s.width), coords(&[s.start, s.end]), q(&s.net))?;
    }
    for v in board.vias.iter().filter(|v| v.locked) {
        let n = via_name(v.size, v.drill);
        if !via_table.contains_key(&n) {
            bail!("locked via at {:?} has no matching via padstack ({n})", v.at);
        }
        writeln!(o, "    (via {} {} (net {})(type fix))", q(&n), xy(v.at), q(&v.net))?;
    }
    o.push_str("  )\n)\n");
    std::fs::write(path, o)?;
    Ok(via_table)
}
