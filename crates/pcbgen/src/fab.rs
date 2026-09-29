//! Fab outputs for JLCPCB: gerbers + drill (zipped), BOM and CPL (pick-and-place).
//!
//! Everything is made by kicad-cli from the checked board; the BOM comes from the circuit
//! (the LCSC numbers live there). Gerbers, drill and positions all use the board's
//! drill/place origin (bottom-left corner, set by the pcb stage), so they line up.
//!
//! Rotations: JLCPCB's reel orientation differs from KiCad's footprint zero for some
//! packages. `ROTATIONS` holds those corrections, from the community table that the
//! kicad-jlcpcb-tools plugin downloads (matthewlai/JLCKicadTools cpl_rotations_db.csv,
//! fetched 2026-09-29). A footprint with no entry is listed as UNVERIFIED in fab/README:
//! check it in JLCPCB's placement preview before paying.

use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::Path;

use anyhow::{Result, bail};
use regex::Regex;

use crate::circuit::{Circuit, Part};
use crate::kicad_cli;

const GERBER_LAYERS: &str = "F.Cu,B.Cu,F.Paste,B.Paste,F.Silkscreen,B.Silkscreen,F.Mask,B.Mask,Edge.Cuts";

/// Footprint-name regex → degrees added to KiCad's rotation (top side).
const ROTATIONS: [(&str, i32); 6] = [
    (r"^SOT-223", 180),
    (r"^SOT-23", -90),
    (r"^TSOT-23", 180),
    (r"^DFN-", 270),
    (r"^USB_C_Receptacle_HRO_TYPE-C-31-M-12", 180),
    (r"^ESP32-W", 270),
];
/// Footprints that need no correction: unpolarised two-terminal parts only (LEDs and
/// diodes have a polarity, so a 180 deg error would matter: they stay UNVERIFIED).
const NO_CORRECTION: &str = r"^(R|C|Fuse)_\d{4}_";

fn correction(footprint: &str) -> Option<i32> {
    let name = footprint.rsplit(':').next().unwrap_or(footprint);
    for (pat, deg) in ROTATIONS {
        if Regex::new(pat).unwrap().is_match(name) {
            return Some(deg);
        }
    }
    Regex::new(NO_CORRECTION).unwrap().is_match(name).then_some(0)
}

/// "R10" → ("R", 10), for sorting designators naturally.
fn refkey(r: &str) -> (String, u64) {
    let alpha: String = r.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
    let num = r[alpha.len()..].chars().take_while(|c| c.is_ascii_digit()).collect::<String>();
    match num.parse() {
        Ok(n) if !alpha.is_empty() => (alpha, n),
        _ => (r.to_string(), 0),
    }
}

/// Python's `f"{v:g}"` for the rotations and positions JLCPCB reads.
fn fmt_g(v: f64) -> String {
    crate::sexpr::fmt_num(v)
}

pub fn export(project_dir: &Path, name: &str, c: &Circuit, out: &Path) -> Result<()> {
    let pcb = project_dir.join(format!("{name}.kicad_pcb"));
    let pcb_s = pcb.to_string_lossy().into_owned();
    if out.exists() {
        std::fs::remove_dir_all(out)?;
    }
    let gerbers = out.join("gerbers");
    std::fs::create_dir_all(&gerbers)?;
    let g = gerbers.to_string_lossy().into_owned();
    kicad_cli(&["pcb", "export", "gerbers", "-o", &g, "-l", GERBER_LAYERS, "--use-drill-file-origin", "--subtract-soldermask", &pcb_s])?;
    kicad_cli(&["pcb", "export", "drill", "-o", &format!("{g}/"), "--format", "excellon", "--drill-origin", "plot", "--excellon-units", "mm", &pcb_s])?;
    let zip_path = out.join(format!("{name}-gerbers.zip"));
    {
        let mut files: Vec<_> = std::fs::read_dir(&gerbers)?.collect::<Result<Vec<_>, _>>()?;
        files.sort_by_key(|e| e.file_name());
        let mut z = zip::ZipWriter::new(std::fs::File::create(&zip_path)?);
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for f in files {
            z.start_file(f.file_name().to_string_lossy(), opts)?;
            z.write_all(&std::fs::read(f.path())?)?;
        }
        z.finish()?;
    }

    let pos = out.join("positions-kicad.csv");
    kicad_cli(&[
        "pcb", "export", "pos", "-o", &pos.to_string_lossy(), "--format", "csv", "--units", "mm", "--side", "both",
        "--use-drill-file-origin", "--exclude-dnp", &pcb_s,
    ])?;

    // BOM: parts JLCPCB places = in the BOM, not DNP, with an LCSC number
    let real = |p: &&Part| !p.reference.starts_with('#');
    let placed: Vec<&Part> = c.parts.iter().filter(real).filter(|p| p.in_bom && !p.dnp && p.field("LCSC").is_some()).collect();
    let mut skipped: Vec<&str> = c
        .parts
        .iter()
        .filter(real)
        .filter(|p| !placed.iter().any(|q| q.reference == p.reference))
        .map(|p| p.reference.as_str())
        .collect();
    skipped.sort();
    let mut groups: BTreeMap<(String, String, String), Vec<&str>> = BTreeMap::new();
    for p in &placed {
        let fp = p.footprint.rsplit(':').next().unwrap_or("").to_string();
        groups.entry((p.value.clone(), fp, p.field("LCSC").unwrap().to_string())).or_default().push(&p.reference);
    }
    // lines sorted by their first-listed part, each line's parts in natural order
    let mut lines: Vec<_> = groups.into_iter().collect();
    lines.sort_by(|a, b| a.1[0].cmp(b.1[0]));
    for (_, refs) in lines.iter_mut() {
        refs.sort_by_key(|r| refkey(r));
    }
    let mut w = csv_writer(&out.join(format!("{name}-bom.csv")))?;
    w.write_record(["Comment", "Designator", "Footprint", "LCSC"])?;
    for ((value, fp, lcsc), refs) in &lines {
        w.write_record([value.as_str(), &refs.join(","), fp, lcsc])?;
    }
    w.flush()?;

    // CPL with rotation corrections
    let (mut unverified, mut notes) = (vec![], vec![]);
    let mut rd = csv::Reader::from_path(&pos)?;
    let hdr = rd.headers()?.clone();
    let col = |k: &str| hdr.iter().position(|h| h == k).ok_or_else(|| anyhow::anyhow!("position file has no {k} column"));
    let (c_ref, c_x, c_y, c_rot, c_side) = (col("Ref")?, col("PosX")?, col("PosY")?, col("Rot")?, col("Side")?);
    let mut w = csv_writer(&out.join(format!("{name}-cpl.csv")))?;
    w.write_record(["Designator", "Mid X", "Mid Y", "Layer", "Rotation"])?;
    let mut in_cpl = vec![];
    for row in rd.records() {
        let row = row?;
        let r = &row[c_ref];
        let Some(part) = placed.iter().find(|p| p.reference == r) else { continue };
        let fp = &part.footprint;
        let short = fp.rsplit(':').next().unwrap_or("");
        let corr = match correction(fp) {
            Some(c) => c,
            None => {
                unverified.push(format!("{r} ({short})"));
                0
            }
        };
        let side = row[c_side].to_lowercase();
        if side != "top" && corr != 0 {
            bail!("{r}: bottom-side rotation corrections are not handled");
        }
        let krot: f64 = row[c_rot].parse()?;
        let rot = (krot + corr as f64).rem_euclid(360.0);
        if corr != 0 {
            notes.push(format!("{r}: KiCad {} deg + {corr} = {} deg", fmt_g(krot), fmt_g(rot)));
        }
        let (x, y): (f64, f64) = (row[c_x].parse()?, row[c_y].parse()?);
        w.write_record([
            r.to_string(),
            format!("{x:.4}mm"),
            format!("{y:.4}mm"),
            if side == "top" { "Top".into() } else { "Bottom".into() },
            fmt_g(rot),
        ])?;
        in_cpl.push(r.to_string());
    }
    w.flush()?;
    let mut missing: Vec<&str> = placed.iter().map(|p| p.reference.as_str()).filter(|r| !in_cpl.iter().any(|x| x == r)).collect();
    missing.sort();
    if !missing.is_empty() {
        bail!("parts in the BOM but not in the placement file: {missing:?}");
    }

    std::fs::write(out.join("README.md"), readme(name, placed.len(), &skipped, &notes, &unverified))?;
    println!(
        "fab: {}, {name}-bom.csv ({} lines, {} parts), {name}-cpl.csv; not assembled: {}",
        zip_path.file_name().unwrap().to_string_lossy(),
        lines.len(),
        placed.len(),
        if skipped.is_empty() { "none".into() } else { skipped.join(", ") }
    );
    if !unverified.is_empty() {
        println!("fab: rotation UNVERIFIED for {}: check JLCPCB's placement preview", unverified.join(", "));
    }
    Ok(())
}

fn readme(name: &str, n_parts: usize, skipped: &[&str], notes: &[String], unverified: &[String]) -> String {
    let list = |v: &[String]| if v.is_empty() { vec!["- none".to_string()] } else { v.iter().map(|x| format!("- {x}")).collect() };
    let mut lines = vec![
        format!("# Fab files: {name}"),
        String::new(),
        format!("Generated by `cargo run --release -p {name} -- fab`; do not edit by hand."),
        String::new(),
        format!("- `{name}-gerbers.zip`: upload as the PCB (gerbers + drill)."),
        format!("- `{name}-bom.csv` and `{name}-cpl.csv`: upload for assembly ({n_parts} parts)."),
        format!(
            "- Not assembled (no LCSC part, or test points/holes): {}.",
            if skipped.is_empty() { "none".into() } else { skipped.join(", ") }
        ),
        String::new(),
        "## Rotation corrections applied".into(),
        String::new(),
    ];
    lines.extend(list(notes));
    lines.extend([
        String::new(),
        "## Rotation UNVERIFIED (no known correction)".into(),
        String::new(),
        "Check each of these in JLCPCB's placement preview (pin 1 marker) before paying.".into(),
        String::new(),
    ]);
    lines.extend(list(unverified));
    lines.join("\n") + "\n"
}

/// CSV with CRLF line ends, as the Python pipeline wrote them (JLCPCB takes both).
fn csv_writer(path: &Path) -> Result<csv::Writer<std::fs::File>> {
    Ok(csv::WriterBuilder::new().terminator(csv::Terminator::CRLF).from_path(path)?)
}
