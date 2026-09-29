//! The `case` stage (D-025 Phase B): a printed two-part case around the board, fit-checked
//! against the board's own 3D model every run.
//!
//! 1. `case/board.json` from the saved `.kicad_pcb` (`board_json`): the outline, mounting holes,
//!    every footprint's side, position, rotation, courtyard and F.Fab boxes, and `[case]` with its
//!    defaults filled in. **Coordinates are KiCad board mm (x right, y down, the page origin)**;
//!    z is up from the PCB's bottom face (its top face is at `thickness`).
//! 2. `case/board.step` with `kicad-cli pcb export step --user-origin 0x0mm` and the models in
//!    `lib/3dmodels`: STEP x = x, STEP y = -y, z as above (`STEP_ORIGIN`). kicad-cli exits 0 when a
//!    3D model is missing and only prints `Could not add 3D model for <ref>.`, so the stage fails
//!    on any such line: a fit check against a board with parts missing proves nothing.
//! 3. `enclosure/case.py` (CadQuery, `enclosure/.venv`) builds the tray, lid and button caps from
//!    board.json, checks them against board.step and writes `case/fit.json`, the STEP/STL files and
//!    PNG renders. The stage passes only if fit.json says `"ok": true` (never the exit code alone).

use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};
use regex::Regex;
use serde_json::{Value, json};

use crate::board::{Board, Footprint};
use crate::boardfile::{BoardFile, Case};
use crate::geom::{Pt, bbox, edge_dist, inside};

/// How board.step's coordinates relate to board.json's (checked on the starter, 2026-09-29).
pub const STEP_ORIGIN: &str = "kicad-cli --user-origin 0x0mm: STEP x = x, STEP y = -y (KiCad's y points down), \
     STEP z = z; the PCB's bottom face is at z = 0 (its dielectric fills 0..thickness - copper and mask, \
     and parts sit on z = thickness)";

/// Runs the stage for the board file at `pcb`; outputs go to `<base>/case/`.
pub fn run(bf: &BoardFile, pcb: &Path, base: &Path) -> Result<bool> {
    let Some(case) = &bf.case else {
        println!("case: no [case] in board.toml (skipped; see templates/board.toml)");
        return Ok(true);
    };
    let root = crate::repo_root();
    let python = root.join("enclosure/.venv/bin/python");
    if !python.exists() {
        bail!("enclosure/.venv is missing; install it once with: cd enclosure && uv sync");
    }
    let out = base.join("case");
    std::fs::create_dir_all(&out)?;

    let text = std::fs::read_to_string(pcb).with_context(|| format!("reading {} (run the pcb stage first)", pcb.display()))?;
    let tree = crate::sexpr::parse(&text)?;
    let board = Board::from_sexp(&tree)?;
    let thick = tree.find("general").and_then(|g| g.find("thickness")).map_or(1.6, |t| t.num(1));
    let bj = board_json(&board, thick, &bf.board.name, case);
    std::fs::write(out.join("board.json"), serde_json::to_string_pretty(&bj)? + "\n")?;
    println!("case: {} ({} footprints, {} mounting holes)", out.join("board.json").display(), bj["footprints"].as_array().map_or(0, Vec::len), bj["mounting_holes"].as_array().map_or(0, Vec::len));

    let step = out.join("board.step");
    export_step(pcb, &step, &root.join("lib/3dmodels"))?;
    println!("case: {}", step.display());

    // a stale fit.json must never be read as this run's result
    let fit_path = out.join("fit.json");
    let _ = std::fs::remove_file(&fit_path);
    let status = Command::new(&python).arg(root.join("enclosure/case.py")).arg(&out).status().context("running enclosure/case.py")?;
    let Ok(fit) = std::fs::read_to_string(&fit_path).map_err(anyhow::Error::from).and_then(|t| Ok(serde_json::from_str::<Value>(&t)?)) else {
        println!("== CASE FIT: FAIL (case.py wrote no readable fit.json; exit {status})");
        return Ok(false);
    };
    print_summary(&fit);
    let ok = fit["ok"] == true;
    if ok && !status.success() {
        println!("== CASE FIT: FAIL (fit.json says ok but case.py exited with {status})");
        return Ok(false);
    }
    println!("== CASE FIT: {}", if ok { "PASS" } else { "FAIL" });
    Ok(ok)
}

fn print_summary(fit: &Value) {
    for c in fit["checks"].as_array().into_iter().flatten() {
        let v = |k: &str| c[k].as_f64().map_or(String::new(), |x| format!("{x:.3}"));
        println!(
            "   {} {}: {} ({} {}) {}",
            if c["ok"] == true { "ok  " } else { "FAIL" },
            c["name"].as_str().unwrap_or("?"),
            v("value"),
            c["cmp"].as_str().unwrap_or(""),
            v("limit"),
            c["detail"].as_str().unwrap_or("")
        );
    }
    for m in fit["clearance"].as_array().into_iter().flatten() {
        println!(
            "   clearance {}: {:.3} mm to {}",
            m["part"].as_str().unwrap_or("?"),
            m["min_mm"].as_f64().unwrap_or(f64::NAN),
            m["nearest"].as_str().unwrap_or("?")
        );
    }
    for p in fit["problems"].as_array().into_iter().flatten() {
        println!("   OPEN:   {}", p.as_str().unwrap_or("?"));
    }
}

/// Exports the board's STEP with the repo's 3D models; fails on any missing model.
pub fn export_step(pcb: &Path, step: &Path, models: &Path) -> Result<()> {
    let var = format!("KICAD10_3DMODEL_DIR={}", models.display());
    let out = Command::new("kicad-cli")
        .args(["pcb", "export", "step", "--no-dnp", "--user-origin", "0x0mm", "-f", "-D", &var, "-o"])
        .arg(step)
        .arg(pcb)
        .output()
        .context("running kicad-cli pcb export step")?;
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    if !out.status.success() {
        bail!("kicad-cli pcb export step failed:\n{text}");
    }
    let missing = missing_models(&text);
    if !missing.is_empty() {
        bail!(
            "kicad-cli found no 3D model for {} (it still exits 0); add the model under lib/3dmodels \
             (enclosure/models.py, HARDWARE_LESSONS \"3D models\"):\n{text}",
            missing.join(", ")
        );
    }
    Ok(())
}

/// The references kicad-cli couldn't add a 3D model for (`Could not add 3D model for R6.`).
pub fn missing_models(output: &str) -> Vec<String> {
    let re = Regex::new(r"Could not add 3D model for (.+?)\.?\s*$").unwrap();
    output.lines().filter_map(|l| re.captures(l.trim_end())).map(|c| c[1].to_string()).collect()
}

fn r4(v: f64) -> f64 {
    (v * 1e4).round() / 1e4
}

fn pt_json(p: Pt) -> Value {
    json!([r4(p.x), r4(p.y)])
}

fn box_json(polys: &[Vec<Pt>]) -> Value {
    let all: Vec<Pt> = polys.iter().flatten().copied().collect();
    if all.is_empty() {
        return Value::Null;
    }
    let (lo, hi) = bbox(&all);
    json!({"min": pt_json(lo), "max": pt_json(hi)})
}

/// A mounting hole: a `MountingHole:` footprint, or one whose pads are all one hole of 2 mm or
/// more. Its (centre, drill).
fn mounting_hole(fp: &Footprint) -> Option<(Pt, f64)> {
    let drilled: Vec<(Pt, f64)> = fp.pads.iter().filter_map(|p| p.drill.filter(|d| d.0 > 0.0).map(|d| (p.pos, d.0.max(d.1)))).collect();
    let big = drilled.iter().copied().max_by(|a, b| a.1.total_cmp(&b.1));
    let one_hole = !fp.pads.is_empty() && drilled.len() == fp.pads.len() && drilled.iter().all(|h| (h.0 - drilled[0].0).norm() < 1e-3);
    match big {
        Some(h) if fp.lib_id.starts_with("MountingHole:") || (one_hole && h.1 >= 2.0) => Some(h),
        _ => None,
    }
}

/// `case/board.json`: what `enclosure/case.py` builds the case from (see the module doc).
pub fn board_json(board: &Board, thickness: f64, name: &str, case: &Case) -> Value {
    let (lo, hi) = bbox(&board.outline);
    let mut holes = vec![];
    let mut fps = vec![];
    for fp in &board.footprints {
        if fp.reference.is_empty() {
            continue;
        }
        if let Some((c, drill)) = mounting_hole(fp) {
            // room for a boss: from the hole's centre to the nearest courtyard edge
            let room = fp.courtyards.iter().filter(|p| inside(c, p)).map(|p| edge_dist(c, p)).fold(f64::INFINITY, f64::min);
            holes.push(json!({
                "ref": fp.reference, "x": r4(c.x), "y": r4(c.y), "drill": r4(drill),
                "courtyard_r": if room.is_finite() { json!(r4(room)) } else { Value::Null },
            }));
        }
        fps.push(json!({
            "ref": fp.reference,
            "value": fp.value,
            "lib_id": fp.lib_id,
            "side": if fp.layer == "B.Cu" { "bottom" } else { "top" },
            "x": r4(fp.pos.x),
            "y": r4(fp.pos.y),
            "rot": r4(fp.rot),
            "courtyard": box_json(&fp.courtyards),
            "fab": box_json(&fp.fab),
        }));
    }
    json!({
        "schema": 1,
        "board": name,
        "units": "mm",
        "coords": "KiCad board coordinates: x right, y down, from the page origin; z up from the PCB's bottom face",
        "step_origin": STEP_ORIGIN,
        "thickness": r4(thickness),
        "outline": board.outline.iter().map(|p| pt_json(*p)).collect::<Vec<_>>(),
        "bbox": {"min": pt_json(lo), "max": pt_json(hi)},
        "mounting_holes": holes,
        "footprints": fps,
        "case": case.resolved(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_model_lines() {
        // as kicad-cli 10.0.6 prints them (on stdout), among its other lines
        let out = "Loading board\nFile not found: ${KICAD10_3DMODEL_DIR}/Resistor_SMD.3dshapes/R_0805_2012Metric.step\n\
                   Could not add 3D model for R6.\nCould not add 3D model for U1.\r\nSTEP file 'x.step' created.\n";
        assert_eq!(missing_models(out), vec!["R6", "U1"]);
        assert!(missing_models("STEP file 'x.step' created.\nExport time 2.8 s\n").is_empty());
    }

    #[test]
    fn missing_model_fails_the_export() {
        // needs kicad-cli; the starter's board file with an empty model directory
        let root = crate::repo_root();
        let pcb = root.join("boards/starter/kicad/starter.kicad_pcb");
        if Command::new("kicad-cli").arg("--version").output().is_err() || !pcb.exists() {
            eprintln!("skipped: needs kicad-cli and the starter's board file");
            return;
        }
        let dir = std::env::temp_dir().join(format!("pcbgen-case-test-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("models")).unwrap();
        let err = export_step(&pcb, &dir.join("b.step"), &dir.join("models")).unwrap_err().to_string();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(err.contains("found no 3D model for") && err.contains("J1") && err.contains("U1"), "{err}");
    }

    #[test]
    fn mounting_holes_and_json() {
        let pcb = crate::repo_root().join("boards/starter/kicad/starter.kicad_pcb");
        let Ok(board) = Board::load(&pcb) else {
            eprintln!("skipped: no starter board file");
            return;
        };
        let bf = crate::boardfile::load(&crate::repo_root().join("boards/starter")).unwrap().unwrap();
        let j = board_json(&board, 1.6, "starter", bf.case.as_ref().unwrap());
        let holes = j["mounting_holes"].as_array().unwrap();
        assert_eq!(holes.len(), 4, "{holes:?}");
        assert!(holes.iter().all(|h| h["drill"] == 3.2 && h["courtyard_r"].as_f64().unwrap() > 3.0));
        assert_eq!(j["bbox"], json!({"min": [100.0, 100.0], "max": [150.0, 150.0]}));
        let j1 = j["footprints"].as_array().unwrap().iter().find(|f| f["ref"] == "J1").unwrap();
        // the HRO USB-C's body (F.Fab) is 8.94 wide, its mouth on the board edge
        assert_eq!(j1["fab"], json!({"min": [120.53, 142.65], "max": [129.47, 149.95]}));
        assert_eq!(j1["side"], "top");
    }
}
