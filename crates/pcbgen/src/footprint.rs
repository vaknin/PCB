//! Load footprints from `.kicad_mod` files and place them in a board.
//!
//! KiCad's board-file conventions (read from boards KiCad saved): pad and graphic
//! positions stay in the footprint's own frame; pad, property and text *angles* include
//! the footprint's rotation; zones inside a footprint (keep-outs) are stored in board
//! coordinates.
//!
//! A bottom-side part is the library footprint flipped first (`flip`, as pcbnew's flip
//! of a footprint at angle 0) and then placed like a top-side one, so its `rot` is the
//! angle KiCad shows and saves (as seen from the top).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};

use crate::geom::{Pt, norm180, norm360, rotate};
use crate::layout::Side;
use crate::sexpr::{Kw, Sexp, parse};
use crate::{node, uid};

/// Library name → directory, from the project's fp-lib-table.
pub fn lib_paths(project_dir: &Path) -> Result<HashMap<String, PathBuf>> {
    let fp_dir = std::env::var("KICAD10_FOOTPRINT_DIR").unwrap_or("/usr/share/kicad/footprints".into());
    let table = parse(&std::fs::read_to_string(project_dir.join("fp-lib-table"))?)?;
    Ok(table
        .find_all("lib")
        .map(|lib| {
            let uri = lib
                .get("uri")
                .unwrap_or("")
                .replace("${KICAD10_FOOTPRINT_DIR}", &fp_dir)
                .replace("${KIPRJMOD}", &project_dir.to_string_lossy());
            (lib.get("name").unwrap_or("").to_string(), PathBuf::from(uri))
        })
        .collect())
}

/// Parse `Lib:Name` from the library table.
pub fn load(libs: &HashMap<String, PathBuf>, fpid: &str) -> Result<Sexp> {
    let (lib, name) = fpid.split_once(':').ok_or_else(|| anyhow!("footprint id {fpid:?} has no library"))?;
    let dir = libs.get(lib).ok_or_else(|| anyhow!("footprint library {lib} is not in fp-lib-table"))?;
    let path = dir.join(format!("{name}.kicad_mod"));
    let text = std::fs::read_to_string(&path).with_context(|| format!("footprint {fpid} not found in {}", dir.display()))?;
    parse(&text).with_context(|| path.display().to_string())
}

fn rotate_at(n: &mut Sexp, rot: f64) {
    let Some(at) = n.find_mut("at") else { return };
    let a = norm360(at.num(3) + rot);
    let items = at.items_mut();
    items.truncate(3);
    items.push(a.into());
}

/// F.* ↔ B.*; other layers ("*.Cu", "F&B.Cu", "Edge.Cuts") unchanged.
pub fn flip_layer(l: &str) -> String {
    match (l.strip_prefix("F."), l.strip_prefix("B.")) {
        (Some(rest), _) => format!("B.{rest}"),
        (_, Some(rest)) => format!("F.{rest}"),
        _ => l.to_string(),
    }
}

fn neg(n: &mut Sexp, i: usize) {
    if let Some(Sexp::Sym(v)) = n.items_mut().get_mut(i)
        && let Ok(x) = v.parse::<f64>()
    {
        *v = crate::sexpr::fmt_num(-x);
    }
}

fn set_angle(at: &mut Sexp, a: f64) {
    let items = at.items_mut();
    items.truncate(3);
    if a != 0.0 {
        items.push(a.into());
    }
}

/// A text's angle and justification flipped as pcbnew does (verified against pcbnew-saved
/// boards, 2026-09-29). The angle a (relative to the footprint) becomes -a; if that is
/// under 180 (mod 360) the text turns a further 180° and keeps its justification,
/// otherwise left/right and top/bottom swap. An `unlocked` text becomes 180 - a and
/// keeps its justification. Mirroring toggles only on F.*/B.* layers (a Cmts.User text
/// stays unmirrored).
fn flip_text(t: &mut Sexp) {
    let sided = t.get("layer").is_some_and(|l| l.starts_with("F.") || l.starts_with("B."));
    let unlocked = t.flag("unlocked");
    let Some(at) = t.find_mut("at") else { return };
    let a = at.num(3);
    let (r, keep) = if unlocked {
        (norm360(180.0 - a), true)
    } else {
        let r = norm360(-a);
        if r < 180.0 { (r + 180.0, true) } else { (r, false) }
    };
    set_angle(at, r);
    let Some(eff) = t.find_mut("effects") else { return };
    let mut words: Vec<String> = eff.find("justify").map(|j| j.items()[1..].iter().filter_map(|w| w.atom()).map(String::from).collect()).unwrap_or_default();
    if !keep {
        for w in words.iter_mut() {
            let swapped = match w.as_str() {
                "left" => "right",
                "right" => "left",
                "top" => "bottom",
                "bottom" => "top",
                other => other,
            };
            *w = swapped.to_string();
        }
    }
    if sided {
        match words.iter().position(|w| w == "mirror") {
            Some(i) => {
                words.remove(i);
            }
            None => words.push("mirror".into()),
        }
    }
    eff.remove_all("justify");
    if !words.is_empty() {
        let mut j = node!("justify");
        for w in words {
            j.push(Sexp::Sym(w));
        }
        eff.push(j);
    }
}

/// A library footprint flipped to the bottom side, still in its own frame: what pcbnew
/// makes of it with a left-right flip at angle 0 (the footprint then reads angle 0
/// again). Every footprint-frame Y is negated, F.* and B.* layers swap, pad angles
/// negate (a trapezoid's Y delta too), texts follow `flip_text`; the 3D model entry is
/// left as it is (pcbnew leaves it too). Verified item by item against pcbnew-saved
/// boards, 2026-09-29 (see HARDWARE_LESSONS.md).
pub fn flip(module: &Sexp) -> Result<Sexp> {
    let mut fp = module.clone();
    for c in fp.items() {
        if let Some(h @ ("fp_text_box" | "dimension" | "fp_image")) = c.head() {
            bail!("flipping a footprint with {h} is not supported yet");
        }
        if c.is("pad") && c.find("padstack").is_some() {
            bail!("flipping a pad with a custom padstack is not supported yet");
        }
    }
    // Y of every point in the footprint's (and each pad's) frame, and the layers
    fp.walk_mut(&mut |n| match n.head() {
        Some("xy" | "start" | "end" | "mid" | "center" | "offset") => neg(n, 2),
        Some("layer") => {
            if let Some(Sexp::Str(l)) = n.items_mut().get_mut(1) {
                *l = flip_layer(l);
            }
        }
        Some("layers") => {
            for l in n.items_mut().iter_mut().skip(1) {
                if let Sexp::Str(s) = l {
                    *s = flip_layer(s);
                }
            }
        }
        _ => {}
    });
    for c in fp.items_mut() {
        match c.head() {
            Some("property" | "fp_text") => {
                if let Some(at) = c.find_mut("at") {
                    neg(at, 2);
                }
                flip_text(c);
            }
            Some("pad") => {
                if let Some(at) = c.find_mut("at") {
                    neg(at, 2);
                    let a = norm360(-at.num(3));
                    set_angle(at, a);
                }
                if let Some(d) = c.find_mut("rect_delta") {
                    neg(d, 2);
                }
                if let Some(ch) = c.find_mut("chamfer") {
                    for w in ch.items_mut().iter_mut().skip(1) {
                        if let Sexp::Sym(s) = w {
                            *s = match s.as_str() {
                                "top_left" => "bottom_left",
                                "bottom_left" => "top_left",
                                "top_right" => "bottom_right",
                                "bottom_right" => "top_right",
                                o => o,
                            }
                            .to_string();
                        }
                    }
                }
            }
            _ => {}
        }
    }
    Ok(fp)
}

/// A library footprint placed on the board at `pos` (absolute, mm) rotated `rot`, on
/// `side` (a bottom-side one is `flip`ped first). Properties, pads and nets are filled
/// in by the caller; this does the geometry, renames it `Lib:Name` and gives every item
/// a deterministic UUID.
pub fn place(module: &Sexp, fpid: &str, pos: Pt, rot: f64, side: Side, board: &str, reference: &str) -> Result<Sexp> {
    let mut fp = if side == Side::Bottom { flip(module)? } else { module.clone() };
    let items = fp.items_mut();
    items[1] = Sexp::Str(fpid.to_string());
    items.retain(|c| !(c.is("version") || c.is("generator") || c.is("generator_version")));
    for c in items.iter_mut() {
        match c.head() {
            Some("property" | "fp_text" | "pad") => rotate_at(c, rot),
            Some("zone") => {
                // keep-outs are stored in board coordinates
                c.walk_mut(&mut |n| {
                    if n.is("xy") {
                        let p = pos + rotate(crate::board::xy(Some(n)), rot);
                        *n = node!("xy", p.x, p.y);
                    }
                });
            }
            _ => {}
        }
    }
    let mut k = 0;
    fp.walk_mut(&mut |n| {
        if n.is("uuid") {
            k += 1;
            *n = node!("uuid", uid(board, &["fp", reference, &k.to_string()]));
        }
    });
    let layer_at = fp.items().iter().position(|c| c.is("layer")).map_or(2, |i| i + 1);
    let at = if rot == 0.0 { node!("at", pos.x, pos.y) } else { node!("at", pos.x, pos.y, norm180(rot)) };
    fp.items_mut().splice(layer_at..layer_at, [node!("uuid", uid(board, &["fp", reference])), at]);
    Ok(fp)
}

/// A hidden footprint field, as pcbnew adds one for a symbol field. On the bottom side it
/// is the top-side field flipped (B.SilkS, turned 180°, mirrored): inferred from
/// `flip_text`, not checked against pcbnew's netlist update.
pub fn field(key: &str, value: &str, rot: f64, side: Side, board: &str, reference: &str) -> Sexp {
    let bottom = side == Side::Bottom;
    let mut eff = node!("effects", node!("font", node!("size", 1.27, 1.27), node!("thickness", 0)));
    if bottom {
        eff.push(node!("justify", Kw("mirror")));
    }
    node!(
        "property",
        key,
        value,
        node!("at", 0, 0, norm360(rot + if bottom { 180.0 } else { 0.0 })),
        node!("layer", if bottom { "B.SilkS" } else { "F.SilkS" }),
        node!("hide", true),
        node!("uuid", uid(board, &["field", reference, key])),
        eff
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::pt;

    /// Expected values read from boards pcbnew saved after flipping these items
    /// (KiCad 10.0.6, 2026-09-29).
    const MODULE: &str = r#"(footprint "T" (layer "F.Cu")
  (property "Reference" "REF**" (at 1 -2 0) (layer "F.SilkS") (effects (font (size 1 1)) (justify left)))
  (property "Value" "T" (at 0 2 90) (layer "F.Fab") (effects (font (size 1 1))))
  (fp_text user "a" (at -1 0.5 30) (layer "F.Fab") (effects (font (size 1 1)) (justify right bottom)))
  (fp_text user "b" (at 0 0 30) (layer "Cmts.User") (effects (font (size 1 1)) (justify left)))
  (fp_text user "c" (at 0 0 30) (unlocked yes) (layer "F.SilkS") (effects (font (size 1 1)) (justify left)))
  (fp_text user "d" (at 0 0 0) (layer "F.SilkS") (effects (font (size 1 1)) (justify mirror)))
  (fp_arc (start -2 1) (mid -1 2) (end 0 1) (stroke (width 0.1) (type solid)) (layer "F.SilkS"))
  (fp_rect (start -3 -3) (end 3 3) (stroke (width 0.05) (type solid)) (fill no) (layer "F.CrtYd"))
  (pad "1" smd rect (at -2 0 90) (size 1 0.5) (layers "F.Cu" "F.Mask" "F.Paste"))
  (pad "2" smd trapezoid (at 2 0.5 30) (size 1 0.8) (rect_delta 0.2 0.3) (layers "F.Cu" "F.Mask"))
  (pad "3" thru_hole oval (at 0 1.5 60) (size 1.2 0.8) (drill oval 0.8 0.4 (offset 0.1 0.05)) (layers "*.Cu" "*.Mask"))
  (pad "4" smd roundrect (at -1 -2.2 20) (size 0.8 0.5) (layers "F.Cu") (roundrect_rratio 0.2) (chamfer_ratio 0.3) (chamfer top_left bottom_right))
  (pad "5" smd custom (at 0 -1 45) (size 0.4 0.4) (layers "B.Cu") (primitives (gr_line (start 0 0) (end -0.5 0.6) (width 0.2))))
  (zone (layer "F.Cu") (keepout (tracks not_allowed)) (polygon (pts (xy -1 -2.8) (xy 1.5 -2.4))))
  (model "x.step" (offset (xyz 0.1 0.2 0.3)) (rotate (xyz 10 20 30))))"#;

    fn child<'a>(fp: &'a Sexp, head: &str, key: &str) -> &'a Sexp {
        fp.items().iter().find(|c| c.is(head) && c.arg(if head == "fp_text" { 2 } else { 1 }) == Some(key)).unwrap()
    }

    fn text(n: &Sexp) -> String {
        crate::sexpr::dumps(n).split_whitespace().collect::<Vec<_>>().join(" ").replace(" )", ")")
    }

    #[test]
    fn flip_matches_pcbnew() {
        let f = flip(&parse(MODULE).unwrap()).unwrap();
        let has = |head: &str, key: &str, s: &str| {
            let t = text(child(&f, head, key));
            assert!(t.contains(s), "{head} {key}: {s:?} not in {t}");
        };
        assert_eq!(f.get("layer"), Some("B.Cu"));
        // texts: -a, turned a further 180 when under 180 (keeping justification), else
        // justification swapped; mirror toggled on sided layers only; unlocked: 180 - a
        has("property", "Reference", "(at 1 2 180)");
        has("property", "Reference", "(justify left mirror)");
        has("property", "Reference", "(layer \"B.SilkS\")");
        has("property", "Value", "(at 0 -2 270)");
        has("fp_text", "a", "(at -1 -0.5 330)");
        has("fp_text", "a", "(justify left top mirror)");
        has("fp_text", "b", "(at 0 0 330)");
        has("fp_text", "b", "(justify right)");
        assert!(!text(child(&f, "fp_text", "b")).contains("mirror"));
        has("fp_text", "b", "(layer \"Cmts.User\")");
        has("fp_text", "c", "(at 0 0 150)");
        has("fp_text", "c", "(justify left mirror)");
        has("fp_text", "d", "(at 0 0 180)");
        assert!(!text(child(&f, "fp_text", "d")).contains("justify"));
        // pads: Y and angle negated, layers swapped, trapezoid dy and drill offset y negated
        has("pad", "1", "(at -2 0 270)");
        has("pad", "1", "(layers \"B.Cu\" \"B.Mask\" \"B.Paste\")");
        has("pad", "2", "(at 2 -0.5 330)");
        has("pad", "2", "(rect_delta 0.2 -0.3)");
        has("pad", "3", "(at 0 -1.5 300)");
        has("pad", "3", "(offset 0.1 -0.05)");
        has("pad", "3", "(layers \"*.Cu\" \"*.Mask\")");
        has("pad", "4", "(chamfer bottom_left top_right)");
        has("pad", "5", "(end -0.5 -0.6)");
        has("pad", "5", "(layers \"F.Cu\")");
        let t = text(&f);
        assert!(t.contains("(fp_arc (start -2 -1) (mid -1 -2) (end 0 -1)"), "{t}");
        assert!(t.contains("(fp_rect (start -3 3) (end 3 -3)"), "{t}");
        assert!(t.contains("(zone (layer \"B.Cu\")"), "{t}");
        assert!(t.contains("(pts (xy -1 2.8) (xy 1.5 2.4))"), "{t}");
        assert!(t.contains("(offset (xyz 0.1 0.2 0.3)) (rotate (xyz 10 20 30))"), "the model stays: {t}");
    }

    /// A bottom part reads back through `board::Footprint` with its pads where pcbnew puts
    /// them: the footprint-frame Y mirrored, then the usual rotation.
    #[test]
    fn bottom_place_reads_back() {
        let m = parse(MODULE).unwrap();
        let fp = place(&m, "L:T", pt(110.0, 120.0), 90.0, Side::Bottom, "b", "U1").unwrap();
        let f = crate::board::Footprint::from_node(&fp).unwrap();
        assert_eq!(f.layer, "B.Cu");
        let p1 = f.pads.iter().find(|p| p.number == "1").unwrap();
        // library (-2, 0) → flipped (-2, 0) → turned 90° CCW (Y down): (0, +2)
        assert!((p1.pos - pt(110.0, 122.0)).norm() < 1e-9, "{:?}", p1.pos);
        assert!(p1.on_layer("B.Cu") && !p1.on_layer("F.Cu"));
        assert_eq!(norm360(p1.angle), 0.0); // -90 relative + 90
        let p3 = f.pads.iter().find(|p| p.number == "3").unwrap();
        assert_eq!(p3.offset, pt(0.1, -0.05));
        assert_eq!(f.keepouts[0].layers, ["B.Cu"]);
        // the keep-out's library point (1.5, -2.4) → (1.5, 2.4) → turned: (2.4, -1.5)
        assert!((f.keepouts[0].poly[1] - pt(112.4, 118.5)).norm() < 1e-9, "{:?}", f.keepouts[0].poly);
        // a top placement is unchanged by the new argument
        let top = place(&m, "L:T", pt(0.0, 0.0), 0.0, Side::Top, "b", "U1").unwrap();
        assert_eq!(top.get("layer"), Some("F.Cu"));
        let err = flip(&parse(r#"(footprint "X" (fp_text_box "t"))"#).unwrap()).unwrap_err();
        assert!(err.to_string().contains("not supported"), "{err}");
    }
}
