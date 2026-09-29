//! Build a .kicad_pcb from the schematic's netlist, with placement given in code.
//!
//! The netlist is exported from the schematic by kicad-cli, so the PCB is always
//! derived from exactly what ERC checked, and every footprint carries its symbol's
//! UUID path (DRC's schematic-parity check relies on that). The file is written here
//! and then re-saved by `kicad-cli pcb upgrade --force`, so KiCad parses it at once.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow, bail};

use crate::board::{Footprint, ORIGIN};
use crate::gates::netlist_nets;
use crate::geom::{Pt, collides, pt, rotate};
use crate::layout::{BoardSpec, Side};
use crate::sexpr::{Kw, Sexp, dumps};
use crate::{footprint, kicad_cli, node, uid};

/// The board file format this writer targets (KiCad 10.0).
const FORMAT_VERSION: i64 = 20260206;

fn board_pt(x: f64, y: f64) -> Pt {
    pt(ORIGIN.x + x, ORIGIN.y + y)
}

fn xy(p: Pt) -> Sexp {
    node!("xy", p.x, p.y)
}

fn layers_table() -> Sexp {
    let mut t = node!("layers");
    let rows: [(i32, &str, &str, Option<&str>); 24] = [
        (0, "F.Cu", "signal", None),
        (2, "B.Cu", "signal", None),
        (9, "F.Adhes", "user", Some("F.Adhesive")),
        (11, "B.Adhes", "user", Some("B.Adhesive")),
        (13, "F.Paste", "user", None),
        (15, "B.Paste", "user", None),
        (5, "F.SilkS", "user", Some("F.Silkscreen")),
        (7, "B.SilkS", "user", Some("B.Silkscreen")),
        (1, "F.Mask", "user", None),
        (3, "B.Mask", "user", None),
        (17, "Dwgs.User", "user", Some("User.Drawings")),
        (19, "Cmts.User", "user", Some("User.Comments")),
        (21, "Eco1.User", "user", Some("User.Eco1")),
        (23, "Eco2.User", "user", Some("User.Eco2")),
        (25, "Edge.Cuts", "user", None),
        (27, "Margin", "user", None),
        (31, "F.CrtYd", "user", Some("F.Courtyard")),
        (29, "B.CrtYd", "user", Some("B.Courtyard")),
        (35, "F.Fab", "user", None),
        (33, "B.Fab", "user", None),
        (39, "User.1", "user", None),
        (41, "User.2", "user", None),
        (43, "User.3", "user", None),
        (45, "User.4", "user", None),
    ];
    for (id, name, kind, user) in rows {
        let mut row = Sexp::List(vec![Sexp::Sym(id.to_string()), name.into(), Kw(kind).into()]);
        if let Some(u) = user {
            row.push(u.into());
        }
        t.push(row);
    }
    t
}

fn outline(items: &mut Vec<Sexp>, board: &str, w: f64, h: f64, r: f64) {
    let stroke = || node!("stroke", node!("width", 0.1), node!("type", Kw("default")));
    let mut n = 0;
    let mut id = || {
        n += 1;
        node!("uuid", uid(board, &["edge", &n.to_string()]))
    };
    let seg = |a: (f64, f64), b: (f64, f64), id: Sexp| {
        let (a, b) = (board_pt(a.0, a.1), board_pt(b.0, b.1));
        node!("gr_line", node!("start", a.x, a.y), node!("end", b.x, b.y), stroke(), node!("layer", "Edge.Cuts"), id)
    };
    items.push(seg((r, 0.0), (w - r, 0.0), id()));
    items.push(seg((w, r), (w, h - r), id()));
    items.push(seg((w - r, h), (r, h), id()));
    items.push(seg((0.0, h - r), (0.0, r), id()));
    if r > 0.0 {
        let k = r * (1.0 - std::f64::consts::FRAC_1_SQRT_2);
        let arcs = [
            ((w - r, 0.0), (w - k, k), (w, r)),
            ((w, h - r), (w - k, h - k), (w - r, h)),
            ((r, h), (k, h - k), (0.0, h - r)),
            ((0.0, r), (k, k), (r, 0.0)),
        ];
        for (s, m, e) in arcs {
            let (s, m, e) = (board_pt(s.0, s.1), board_pt(m.0, m.1), board_pt(e.0, e.1));
            items.push(node!(
                "gr_arc",
                node!("start", s.x, s.y),
                node!("mid", m.x, m.y),
                node!("end", e.x, e.y),
                stroke(),
                node!("layer", "Edge.Cuts"),
                id()
            ));
        }
    }
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Sexp {
    node!(
        "polygon",
        node!("pts", xy(board_pt(x0, y0)), xy(board_pt(x1, y0)), xy(board_pt(x1, y1)), xy(board_pt(x0, y1)))
    )
}

/// A copper pour: SMD pads solid (reflowed by the fab), thermal reliefs only on
/// through-hole pads (with thermals everywhere, small pads failed the two-spoke minimum).
fn pour(board: &str, net: &str, layer: &str, name: &str, priority: u32, poly: Sexp) -> Sexp {
    let mut z = node!("zone", node!("net", net), node!("layer", layer), node!("uuid", uid(board, &["zone", name])), node!("name", name), node!("hatch", Kw("edge"), 0.5));
    if priority > 0 {
        z.push(node!("priority", priority));
    }
    z.push(node!("connect_pads", Kw("thru_hole_only"), node!("clearance", 0.3)));
    z.push(node!("min_thickness", 0.25));
    z.push(node!("fill", node!("thermal_gap", 0.3), node!("thermal_bridge_width", 0.4), node!("island_removal_mode", 0)));
    z.push(poly);
    z
}

/// Short locked tracks leading connected pads out of the footprint's own keep-out.
///
/// Some footprints (e.g. Sensirion's DFN, "no copper under the sensor") carry a keep-out
/// that leaves only a pad-sized notch. Freerouting always aims for pad centres and can't
/// fit a track plus clearance into the notch, so it gives up. The stub runs from the pad
/// centre along the pad's long axis, away from the footprint centre, to `reach` mm past
/// the pad's outer end; the router then connects to its free end. Stubs are locked so
/// the route stage keeps them.
fn escape_stubs(fp: &Footprint, board: &str, reach: f64) -> Vec<Sexp> {
    // the copper side the part sits on, and keep-outs on that side
    let layer = fp.layer.as_str();
    let keepouts: Vec<_> =
        fp.keepouts.iter().filter(|k| k.no_tracks && k.layers.iter().any(|l| crate::board::layer_match(l, layer))).collect();
    let mut out = vec![];
    for pad in &fp.pads {
        let Some(net) = pad.net.as_deref() else { continue };
        if !pad.on_layer(layer) || net.starts_with("unconnected-") {
            continue;
        }
        if !keepouts.iter().any(|k| collides(pad.pos, 0.2, &k.poly)) {
            continue;
        }
        // pad geometry in the footprint's own frame (pads rotated by 90 deg swap axes)
        let (mut sx, mut sy) = pad.size;
        if (pad.rel_angle.round() as i64).rem_euclid(180) == 90 {
            (sx, sy) = (sy, sx);
        }
        let l = pad.local;
        let (d, length, short) = if sx >= sy {
            (pt(if l.x > 0.0 { 1.0 } else { -1.0 }, 0.0), sx, sy)
        } else {
            (pt(0.0, if l.y > 0.0 { 1.0 } else { -1.0 }), sy, sx)
        };
        let e = l + d * (length / 2.0 + reach);
        let end = fp.pos + rotate(e, fp.rot);
        // as pcbnew: two thirds of the pad's short side, truncated to 1 nm, at least 0.15 mm
        let width = (((short * 1e6).round() * 2.0 / 3.0).trunc() / 1e6).max(0.15);
        out.push(node!(
            "segment",
            node!("start", pad.pos.x, pad.pos.y),
            node!("end", end.x, end.y),
            node!("width", width),
            node!("locked", true),
            node!("layer", layer),
            node!("net", net),
            node!("uuid", uid(board, &["stub", &fp.reference, &pad.number]))
        ));
    }
    out
}

/// Build the board from KiCad's netlist of the schematic (`gates::export_netlist`).
pub fn build(project_dir: &Path, name: &str, spec: &BoardSpec, net_tree: &Sexp) -> Result<PathBuf> {
    let sch = project_dir.join(format!("{name}.kicad_sch"));
    let pcb_path = project_dir.join(format!("{name}.kicad_pcb"));
    let libs = footprint::lib_paths(project_dir)?;

    // "unconnected-(...)" nets are kept: schematic parity expects no-connect pads to carry them
    let mut pad_net: HashMap<(String, String), String> = HashMap::new();
    let mut nets: Vec<String> = vec![];
    for (nname, pins) in netlist_nets(net_tree) {
        for key in pins {
            pad_net.insert(key, nname.clone());
        }
        nets.push(nname);
    }

    let mut items: Vec<Sexp> = vec![];
    let mut stubs: Vec<Sexp> = vec![];
    let mut missing = vec![];
    for comp in net_tree.find("components").map(|c| c.find_all("comp").collect::<Vec<_>>()).unwrap_or_default() {
        let reference = comp.get("ref").unwrap_or("").to_string();
        let fpid = comp.get("footprint").filter(|f| !f.is_empty()).ok_or_else(|| anyhow!("{reference} has no footprint"))?;
        let module = footprint::load(&libs, fpid).map_err(|e| anyhow!("{reference}: {e:#}"))?;
        let Some(place) = spec.place(&reference) else {
            missing.push(reference);
            continue;
        };
        let rot = place.rot;
        let mut fp = footprint::place(&module, fpid, board_pt(place.x, place.y), rot, place.side, name, &reference)
            .map_err(|e| anyhow!("{reference}: {e:#}"))?;

        // fields: Reference, Value, then the symbol's fields (LCSC, MPN, ...) as pcbnew
        // adds them, hidden; the empty ones and Footprint are skipped
        let mut fields: Vec<(String, String)> = vec![
            ("Reference".into(), reference.clone()),
            ("Value".into(), comp.get("value").unwrap_or("").to_string()),
        ];
        for f in comp.find("fields").map(|f| f.find_all("field").collect::<Vec<_>>()).unwrap_or_default() {
            let fname = f.find("name").and_then(|n| n.arg(1)).unwrap_or("");
            let fval = match f.items().get(2) {
                Some(Sexp::Str(v)) => v.as_str(),
                _ => "",
            };
            if fname != "Footprint" && !fval.is_empty() {
                fields.push((fname.to_string(), fval.to_string()));
            }
        }
        for (k, v) in fields {
            let existing = fp.items_mut().iter_mut().find(|p| p.is("property") && p.arg(1) == Some(&k));
            match existing {
                Some(p) => p.items_mut()[2] = Sexp::Str(v),
                None => {
                    let at = fp.items().iter().rposition(|c| c.is("property")).map_or(2, |i| i + 1);
                    fp.items_mut().insert(at, footprint::field(&k, &v, rot, place.side, name, &reference));
                }
            }
        }
        if !spec.silk_refs.contains(&reference)
            && let Some(p) = fp.items_mut().iter_mut().find(|p| p.is("property") && p.arg(1) == Some("Reference"))
        {
            p.set(node!("layer", if place.side == Side::Bottom { "B.Fab" } else { "F.Fab" }));
        }
        // schematic link, sheet and BOM/DNP flags from the netlist
        let props: HashMap<&str, &str> = comp
            .find_all("property")
            .map(|p| (p.get("name").unwrap_or(""), p.get("value").unwrap_or("")))
            .collect();
        let after_props = fp.items().iter().rposition(|c| c.is("property")).map_or(2, |i| i + 1);
        fp.items_mut().splice(
            after_props..after_props,
            [
                node!("path", format!("/{}", comp.get("tstamps").unwrap_or(""))),
                node!("sheetname", *props.get("Sheetname").unwrap_or(&"")),
                node!("sheetfile", props.get("Sheetfile").map_or(sch.file_name().unwrap().to_string_lossy().into_owned(), |s| s.to_string())),
            ],
        );
        let mut flags: Vec<String> = vec![];
        if props.contains_key("exclude_from_bom") {
            flags.push("exclude_from_bom".into());
        }
        if props.contains_key("dnp") {
            flags.push("dnp".into());
        }
        if !flags.is_empty() {
            match fp.find_mut("attr") {
                Some(attr) => {
                    for f in flags {
                        if !attr.items().iter().any(|a| a.atom() == Some(&f)) {
                            attr.push(Sexp::Sym(f));
                        }
                    }
                }
                None => {
                    let mut attr = node!("attr");
                    for f in flags {
                        attr.push(Sexp::Sym(f));
                    }
                    fp.push(attr);
                }
            }
        }
        for pad in fp.items_mut().iter_mut().filter(|c| c.is("pad")) {
            let key = (reference.clone(), pad.arg(1).unwrap_or("").to_string());
            if let Some(net) = pad_net.get(&key) {
                pad.push(node!("net", net));
            }
        }
        stubs.extend(escape_stubs(&Footprint::from_node(&fp)?, name, 0.5));
        items.push(fp);
    }
    if !missing.is_empty() {
        missing.sort();
        bail!("no placement given for: {}", missing.join(", "));
    }

    outline(&mut items, name, spec.width, spec.height, spec.corner_radius);

    for ko in &spec.keepouts {
        items.push(node!(
            "zone",
            node!("layers", "F.Cu", "B.Cu"),
            node!("uuid", uid(name, &["keepout", &ko.name])),
            node!("name", &ko.name),
            node!("hatch", Kw("edge"), 0.5),
            node!("connect_pads", node!("clearance", 0)),
            node!("min_thickness", 0.25),
            node!(
                "keepout",
                node!("tracks", Kw("not_allowed")),
                node!("vias", Kw("not_allowed")),
                node!("pads", Kw("allowed")),
                node!("copperpour", Kw("not_allowed")),
                node!("footprints", Kw("allowed"))
            ),
            node!("placement", node!("enabled", false), node!("sheetname", "")),
            node!("fill", node!("thermal_gap", 0.5), node!("thermal_bridge_width", 0.5)),
            rect(ko.x0, ko.y0, ko.x1, ko.y1)
        ));
    }

    if nets.contains(&spec.gnd_net) {
        for (i, layer) in spec.gnd_layers.iter().enumerate() {
            let zname = format!("{}_{layer}", spec.gnd_net);
            items.push(pour(name, &spec.gnd_net, layer, &zname, i as u32, rect(0.0, 0.0, spec.width, spec.height)));
        }
    }
    for cz in &spec.zones {
        if !nets.contains(&cz.net) {
            bail!("copper zone net {} is not in the netlist", cz.net);
        }
        for layer in &cz.layers {
            let zname = format!("{}_{layer}", cz.net);
            items.push(pour(name, &cz.net, layer, &zname, cz.priority, rect(cz.x0, cz.y0, cz.x1, cz.y1)));
        }
    }

    for (i, t) in spec.texts.iter().enumerate() {
        let p = board_pt(t.x, t.y);
        let mut eff = node!("effects", node!("font", node!("size", t.size, t.size), node!("thickness", (t.size * 0.15).max(0.15))));
        if t.layer.starts_with("B.") {
            eff.push(node!("justify", Kw("mirror")));
        }
        items.push(node!(
            "gr_text",
            &t.text,
            node!("at", p.x, p.y, t.rot),
            node!("layer", &t.layer),
            node!("uuid", uid(name, &["text", &i.to_string()])),
            eff
        ));
    }
    items.extend(stubs);

    let aux = board_pt(0.0, spec.height); // drill/pos files measured from board bottom-left
    let mut pcb = node!(
        "kicad_pcb",
        node!("version", FORMAT_VERSION),
        node!("generator", "pcbgen"),
        node!("generator_version", "0.2"),
        node!("general", node!("thickness", 1.6), node!("legacy_teardrops", false)),
        node!("paper", "A4"),
        layers_table(),
        node!(
            "setup",
            node!("pad_to_mask_clearance", 0),
            node!("allow_soldermask_bridges_in_footprints", false),
            node!("aux_axis_origin", aux.x, aux.y),
            node!("grid_origin", ORIGIN.x, ORIGIN.y)
        )
    );
    for it in items {
        pcb.push(it);
    }
    std::fs::write(&pcb_path, dumps(&pcb) + "\n")?;
    kicad_cli(&["pcb", "upgrade", "--force", &pcb_path.to_string_lossy()])?;
    Ok(pcb_path)
}
