//! Load footprints from `.kicad_mod` files and place them in a board.
//!
//! KiCad's board-file conventions (read from boards KiCad saved): pad and graphic
//! positions stay in the footprint's own frame; pad, property and text *angles* include
//! the footprint's rotation; zones inside a footprint (keep-outs) are stored in board
//! coordinates.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};

use crate::geom::{Pt, norm180, norm360, rotate};
use crate::sexpr::{Sexp, parse};
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

/// A library footprint placed on the board at `pos` (absolute, mm) rotated `rot`.
/// Properties, pads and nets are filled in by the caller; this does the geometry,
/// renames it `Lib:Name` and gives every item a deterministic UUID.
pub fn place(module: &Sexp, fpid: &str, pos: Pt, rot: f64, board: &str, reference: &str) -> Sexp {
    let mut fp = module.clone();
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
    fp
}

/// A hidden footprint field, as pcbnew adds one for a symbol field.
pub fn field(key: &str, value: &str, rot: f64, board: &str, reference: &str) -> Sexp {
    node!(
        "property",
        key,
        value,
        node!("at", 0, 0, norm360(rot)),
        node!("layer", "F.SilkS"),
        node!("hide", true),
        node!("uuid", uid(board, &["field", reference, key])),
        node!("effects", node!("font", node!("size", 1.27, 1.27), node!("thickness", 0)))
    )
}
