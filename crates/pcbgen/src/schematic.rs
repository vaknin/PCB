//! Emit a KiCad schematic from a Circuit.
//!
//! Style: every connected pin gets a short wire stub ending in a net label; unused
//! pins get a no-connect flag. Parts are grouped by `block` into titled columns.
//! The file is written in KiCad 9 format and then rewritten by
//! `kicad-cli sch upgrade`, so the final file is always saved by KiCad itself.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Result, bail};

use crate::circuit::{Circuit, Part};
use crate::node;
use crate::sexpr::{Kw, Sexp, dumps};
use crate::symlib::{self, Symbol};
use crate::{kicad_cli, uid};

const FORMAT_VERSION: i64 = 20250114; // KiCad 9; upgraded to the running KiCad's format afterwards
const STUB: f64 = 2.54;
const GRID: f64 = 2.54;
const CHAR_W: f64 = 1.0; // approx width of one 1.27 mm label character

fn font() -> Sexp {
    node!("effects", node!("font", node!("size", 1.27, 1.27)))
}

/// Python's round(v, 4).
fn r(v: f64) -> f64 {
    format!("{v:.4}").parse().unwrap()
}

fn snap(v: f64) -> f64 {
    (v / GRID).round_ties_even() * GRID
}

/// Library point (Y up) -> schematic offset (Y down) for a symbol rotated `rot` deg CCW.
fn xform(px: f64, py: f64, rot: i32) -> (f64, f64) {
    let (x, y) = (px, -py);
    let t = (rot as f64).to_radians();
    let (c, s) = (t.cos().round(), t.sin().round());
    (x * c + y * s, -x * s + y * c)
}

/// Unit vector (schematic space) for a library pin angle after symbol rotation.
fn dir(angle: i32, rot: i32) -> (i32, i32) {
    let a = (angle as f64).to_radians();
    let (dx, dy) = xform(a.cos().round(), a.sin().round(), rot);
    (dx as i32, dy as i32)
}

fn label_angle(d: (i32, i32)) -> i32 {
    match d {
        (1, 0) => 0,
        (0, -1) => 90,
        (-1, 0) => 180,
        (0, 1) => 270,
        _ => unreachable!(),
    }
}

/// Bounding box (schematic offsets) including stubs and label text.
fn extent(c: &Circuit, idx: usize) -> (f64, f64, f64, f64) {
    let part = &c.parts[idx];
    let sym = &part.symbol;
    let (x0, y0, x1, y1) = sym.bbox;
    let pts: Vec<(f64, f64)> =
        [x0, x1].iter().flat_map(|&x| [y0, y1].map(|y| xform(x, y, part.rot))).collect();
    let mut xs: Vec<f64> = pts.iter().map(|p| p.0).collect();
    let mut ys: Vec<f64> = pts.iter().map(|p| p.1).collect();
    for pin in &sym.pins {
        let net = c.net_of(idx, pin);
        let (px, py) = xform(pin.x, pin.y, part.rot);
        let d = dir(pin.angle, part.rot);
        let (ox, oy) = (-d.0 as f64, -d.1 as f64);
        let reach = STUB + net.map_or(1.0, |n| n.name.chars().count() as f64 * CHAR_W + 1.5);
        xs.push(px + ox * reach);
        ys.push(py + oy * reach);
        if oy == 0.0 {
            ys.extend([py - 1.5, py + 1.5]);
        }
    }
    let min = |v: &[f64]| v.iter().copied().fold(f64::INFINITY, f64::min);
    let max = |v: &[f64]| v.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    (min(&xs), min(&ys), max(&xs), max(&ys) + 5.0) // room for ref/value text
}

type Titles = Vec<(String, f64, f64)>;

/// Place blocks as columns left to right, wrapping into rows. Returns part positions,
/// block-title texts, and the used width/height.
fn layout(c: &Circuit) -> (Vec<(f64, f64)>, Titles, (f64, f64)) {
    let mut blocks: Vec<(String, Vec<usize>)> = vec![];
    for (i, p) in c.parts.iter().enumerate() {
        match blocks.iter_mut().find(|b| b.0 == p.block) {
            Some(b) => b.1.push(i),
            None => blocks.push((p.block.clone(), vec![i])),
        }
    }
    let mut pos = vec![(0.0, 0.0); c.parts.len()];
    let mut titles = vec![];
    let (margin, gap, max_w) = (25.4, 12.7, 380.0);
    let (mut x, mut y) = (margin, margin);
    let mut row_h: f64 = 0.0;
    for (name, parts) in &blocks {
        let ext: Vec<_> = parts.iter().map(|&i| extent(c, i)).collect();
        let widest = ext.iter().map(|e| e.2 - e.0).fold(f64::NEG_INFINITY, f64::max);
        let width = widest.max(name.chars().count() as f64 * 2.3);
        let height = ext.iter().map(|e| e.3 - e.1 + 5.08).sum::<f64>() + 7.62;
        if x + width > max_w && x > margin {
            (x, y, row_h) = (margin, y + row_h + gap, 0.0);
        }
        titles.push((name.clone(), x, y));
        let mut cy = y + 7.62;
        for (&i, e) in parts.iter().zip(&ext) {
            pos[i] = (snap(x - e.0 + (width - (e.2 - e.0)) / 2.0), snap(cy - e.1));
            cy += e.3 - e.1 + 5.08;
        }
        x += width + gap;
        row_h = row_h.max(height);
    }
    (pos, titles, (max_w + margin, y + row_h + margin))
}

fn paper(w: f64, h: f64) -> Sexp {
    for (name, pw, ph) in [("A4", 297.0, 210.0), ("A3", 420.0, 297.0), ("A2", 594.0, 420.0)] {
        if w <= pw && h <= ph {
            return node!("paper", name);
        }
    }
    node!("paper", "User", r(w), r(h))
}

fn prop(key: &str, value: &str, x: f64, y: f64, hide: bool, justify: Option<&str>) -> Sexp {
    let mut eff = font();
    if let Some(j) = justify {
        eff.push(node!("justify", Kw(j)));
    }
    if hide {
        eff.push(node!("hide", Kw("yes")));
    }
    node!("property", key, value, node!("at", r(x), r(y), 0), eff)
}

fn symbol_instance(c: &Circuit, part: &Part, x: f64, y: f64, root: &str) -> Sexp {
    let sym = &part.symbol;
    let (x0, y0, x1, y1) = sym.bbox;
    let pts: Vec<(f64, f64)> =
        [x0, x1].iter().flat_map(|&a| [y0, y1].map(|b| xform(a, b, part.rot))).collect();
    let top = pts.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
    let bot = pts.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max);
    let right = pts.iter().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max);
    let has_vertical_pins = sym.pins.iter().any(|p| dir(p.angle, part.rot).0 == 0);
    let (fx, j, ref_y, val_y) = if has_vertical_pins && !sym.is_power {
        // keep text clear of labels hanging off top/bottom pins
        (x + right + 1.27, Some("left"), y + top - 1.27, y + top + 1.27)
    } else {
        (x, None, y + top - 1.27, y + bot + 1.905)
    };
    let refr = &part.reference;
    let mut n = node!(
        "symbol",
        node!("lib_id", &part.lib_id),
        node!("at", r(x), r(y), part.rot),
        node!("unit", 1),
        node!("exclude_from_sim", Kw("no")),
        node!("in_bom", part.in_bom),
        node!("on_board", part.on_board),
        node!("dnp", part.dnp),
        node!("uuid", uid(&c.name, &["sym", refr]))
    );
    n.push(prop("Reference", refr, fx, ref_y, sym.is_power, j));
    n.push(prop("Value", &part.value, fx, val_y, false, j));
    n.push(prop("Footprint", &part.footprint, x, y, true, None));
    n.push(prop("Datasheet", sym.prop("Datasheet").unwrap_or("~"), x, y, true, None));
    for (k, v) in &part.fields {
        n.push(prop(k, v, x, y, true, None));
    }
    for pin in &sym.pins {
        n.push(node!("pin", &pin.number, node!("uuid", uid(&c.name, &["pin", refr, &pin.number]))));
    }
    n.push(node!(
        "instances",
        node!("project", &c.name, node!("path", format!("/{root}"), node!("reference", refr), node!("unit", 1)))
    ));
    n
}

/// A net named like a KiCad power symbol (GND, +3V3, VBUS, ...) is drawn with that symbol.
fn power_symbol(net_name: &str) -> Option<Arc<Symbol>> {
    let sym = symlib::load(&format!("power:{net_name}")).ok()?;
    (sym.is_power && net_name != "PWR_FLAG").then_some(sym)
}

/// Rotation that makes the symbol's body point along `outward` (away from the pin).
fn power_rot(sym: &Symbol, outward: (i32, i32)) -> i32 {
    let (_, y0, _, y1) = sym.bbox;
    // screen space: body below or above the pin
    let natural: (f64, f64) = if (y0 + y1) < 0.0 { (0.0, 1.0) } else { (0.0, -1.0) };
    for rot in [0, 90, 180, 270] {
        let (a, b) = xform(natural.0, -natural.1, rot);
        if (a as i32, b as i32) == outward {
            return rot;
        }
    }
    unreachable!()
}

#[allow(clippy::too_many_arguments)]
fn power_instance(c: &Circuit, sym: &Symbol, reference: &str, x: f64, y: f64, rot: i32, root: &str, key: &[&str]) -> Sexp {
    let (tx, ty) = xform(0.0, if (sym.bbox.1 + sym.bbox.3) > 0.0 { 3.2 } else { -4.0 }, rot);
    let mut n = node!(
        "symbol",
        node!("lib_id", &sym.lib_id),
        node!("at", x, y, rot),
        node!("unit", 1),
        node!("exclude_from_sim", Kw("no")),
        node!("in_bom", Kw("no")),
        node!("on_board", Kw("yes")),
        node!("dnp", Kw("no")),
        node!("uuid", uid(&c.name, &[&["pwr"], key].concat()))
    );
    n.push(prop("Reference", reference, x, y, true, None));
    n.push(prop("Value", sym.prop("Value").unwrap_or_default(), r(x + tx), r(y + ty), false, None));
    n.push(prop("Footprint", "", x, y, true, None));
    n.push(prop("Datasheet", "", x, y, true, None));
    for pin in &sym.pins {
        n.push(node!("pin", &pin.number, node!("uuid", uid(&c.name, &[&["pwrpin"], key].concat()))));
    }
    n.push(node!(
        "instances",
        node!("project", &c.name, node!("path", format!("/{root}"), node!("reference", reference), node!("unit", 1)))
    ));
    n
}

pub fn today() -> String {
    let out = std::process::Command::new("date").arg("+%F").output().expect("date");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

pub fn build(c: &mut Circuit) -> Result<Sexp> {
    let problems = c.check();
    if !problems.is_empty() {
        bail!("circuit check failed:\n  {}", problems.join("\n  "));
    }
    let root = uid(&c.name, &["root"]);

    // PWR_FLAG symbols become ordinary parts in their own block
    for (i, net_name) in c.pwr_flags.clone().iter().enumerate() {
        let reference = format!("#FLG{:02}", i + 1);
        if c.find_part(&reference).is_none() {
            let flag = c.part(&reference, "power:PWR_FLAG", "PWR_FLAG", "").block("Power flags").not_in_bom().id();
            c.parts[flag.0].on_board = false;
            let net = c.net(net_name);
            c.connect(net, flag, &["1"]);
        }
    }

    let (pos, titles, (w, h)) = layout(c);
    let mut items: Vec<Sexp> = vec![];
    let mut lib_ids: Vec<String> = vec![];
    let mut n_pwr = 0;
    for (idx, part) in c.parts.iter().enumerate() {
        if !lib_ids.contains(&part.lib_id) {
            lib_ids.push(part.lib_id.clone());
        }
        let (x, y) = pos[idx];
        items.push(symbol_instance(c, part, x, y, &root));
        let mut seen: Vec<(f64, f64)> = vec![];
        for pin in &part.symbol.pins {
            let (dx, dy) = xform(pin.x, pin.y, part.rot);
            let (px, py) = (r(x + dx), r(y + dy));
            if seen.contains(&(px, py)) {
                continue; // stacked pins share one connection
            }
            seen.push((px, py));
            let key: [&str; 2] = [&part.reference, &pin.number];
            let Some(net) = c.net_of(idx, pin) else {
                if c.is_nc(idx, pin) {
                    items.push(node!("no_connect", node!("at", px, py), node!("uuid", uid(&c.name, &[&["nc"], &key[..]].concat()))));
                }
                continue;
            };
            let d = dir(pin.angle, part.rot);
            let (ox, oy) = (-d.0, -d.1);
            let (ex, ey) = (r(px + ox as f64 * STUB), r(py + oy as f64 * STUB));
            items.push(node!(
                "wire",
                node!("pts", node!("xy", px, py), node!("xy", ex, ey)),
                node!("stroke", node!("width", 0), node!("type", Kw("default"))),
                node!("uuid", uid(&c.name, &[&["wire"], &key[..]].concat()))
            ));
            if let Some(psym) = power_symbol(&net.name) {
                if !lib_ids.contains(&psym.lib_id) {
                    lib_ids.push(psym.lib_id.clone());
                }
                n_pwr += 1;
                let rot = power_rot(&psym, (ox, oy));
                items.push(power_instance(c, &psym, &format!("#PWR{n_pwr:03}"), ex, ey, rot, &root, &key));
                continue;
            }
            let angle = label_angle((ox, oy));
            let mut eff = font();
            eff.push(node!("justify", Kw(if angle == 180 || angle == 270 { "right" } else { "left" }), Kw("bottom")));
            items.push(node!(
                "label",
                &net.name,
                node!("at", ex, ey, angle),
                node!("fields_autoplaced", Kw("yes")),
                eff,
                node!("uuid", uid(&c.name, &[&["label"], &key[..]].concat()))
            ));
        }
    }
    for (name, tx, ty) in &titles {
        let eff = node!(
            "effects",
            node!("font", node!("size", 2.54, 2.54), node!("bold", Kw("yes"))),
            node!("justify", Kw("left"), Kw("bottom"))
        );
        items.push(node!(
            "text",
            name,
            node!("exclude_from_sim", Kw("no")),
            node!("at", r(*tx), r(*ty), 0),
            eff,
            node!("uuid", uid(&c.name, &["title", name]))
        ));
    }

    let mut lib_symbols = node!("lib_symbols");
    for id in &lib_ids {
        lib_symbols.push(symlib::load(id)?.node.clone());
    }
    let mut sch = node!(
        "kicad_sch",
        node!("version", FORMAT_VERSION),
        node!("generator", "pcbgen"),
        node!("generator_version", "0.2"),
        node!("uuid", &root),
        paper(w, h),
        node!(
            "title_block",
            node!("title", &c.title),
            node!("date", today()),
            node!("rev", &c.rev),
            node!("company", &c.company),
            node!("comment", 1, "Generated from code by pcbgen; do not edit by hand")
        ),
        lib_symbols
    );
    for it in items {
        sch.push(it);
    }
    sch.push(node!("sheet_instances", node!("path", "/", node!("page", "1"))));
    Ok(sch)
}

pub fn write(c: &mut Circuit, out_dir: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(out_dir)?;
    let path = out_dir.join(format!("{}.kicad_sch", c.name));
    std::fs::write(&path, dumps(&build(c)?) + "\n")?;
    kicad_cli(&["sch", "upgrade", &path.to_string_lossy()])?;
    write_lib_tables(c, out_dir)?;
    Ok(path)
}

/// Project-local library tables, so ERC/DRC don't depend on the user's global KiCad config.
fn write_lib_tables(c: &Circuit, out_dir: &Path) -> Result<()> {
    let mut sym_libs: Vec<&str> = c.parts.iter().map(|p| p.lib_id.split(':').next().unwrap()).collect();
    sym_libs.sort();
    sym_libs.dedup();
    let mut fp_libs: Vec<&str> =
        c.parts.iter().filter(|p| !p.footprint.is_empty()).map(|p| p.footprint.split(':').next().unwrap()).collect();
    fp_libs.sort();
    fp_libs.dedup();
    let local = crate::repo_root().join("lib");
    let sym_uri = |lib: &str| {
        let own = local.join("symbols").join(format!("{lib}.kicad_sym"));
        if own.exists() { own.to_string_lossy().into_owned() } else { format!("${{KICAD10_SYMBOL_DIR}}/{lib}.kicad_sym") }
    };
    let fp_uri = |lib: &str| {
        let own = local.join("footprints").join(format!("{lib}.pretty"));
        if own.exists() { own.to_string_lossy().into_owned() } else { format!("${{KICAD10_FOOTPRINT_DIR}}/{lib}.pretty") }
    };
    let table = |kind: &str, libs: &[&str], uri: &dyn Fn(&str) -> String| {
        let mut t = node!(kind, node!("version", 7));
        for n in libs {
            t.push(node!("lib", node!("name", *n), node!("type", "KiCad"), node!("uri", uri(n)), node!("options", ""), node!("descr", "")));
        }
        dumps(&t) + "\n"
    };
    std::fs::write(out_dir.join("sym-lib-table"), table("sym_lib_table", &sym_libs, &sym_uri))?;
    std::fs::write(out_dir.join("fp-lib-table"), table("fp_lib_table", &fp_libs, &fp_uri))?;
    Ok(())
}
