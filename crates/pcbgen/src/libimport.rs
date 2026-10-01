//! `pcb lib import C123`: a symbol and footprint KiCad doesn't ship, from the part's EasyEDA
//! library entry (easyeda2kicad, from `enclosure/.venv`), into the repo's own `pcbgen`
//! libraries: `lib/symbols/pcbgen.kicad_sym` and `lib/footprints/pcbgen.pretty/`.
//!
//! Both get one stable name, the part's MPN (`pcbgen:IP5306`), and an entry in
//! `lib/IMPORTED.toml` with `status = "UNVERIFIED"`. EasyEDA's libraries are drawn by many
//! hands; nothing imported is trusted until a datasheet check (pins and pads against the
//! manufacturer's PDF) sets `status = "VERIFIED"` and `checked_by`. Until then, a board that
//! uses it has a red item on its readiness page (`unverified_risks`).
//!
//! What the import changes from easyeda2kicad's output (all checked on C181692, 2026-10-01):
//! - Both files are re-saved by `kicad-cli sym/fp upgrade` (easyeda2kicad 1.0.1 writes
//!   KiCad 6 symbols and KiCad 5 `module` footprints), so KiCad has parsed them.
//! - Pins of type `unspecified` (all of them, from EasyEDA) become `passive`: KiCad's ERC warns
//!   on every connection to an unspecified pin. The datasheet check may set real types.
//! - The symbol's own `LCSC Part`, `MPN` and `Manufacturer` fields are dropped: the circuit's
//!   `.lcsc()`/`.mpn()` are the one source, checked by the PARTS gate.
//! - The footprint's model, a WRL in the scratch directory, becomes
//!   `${KICAD10_3DMODEL_DIR}/pcbgen.3dshapes/<name>.step`: `enclosure/models.py` fetches it once
//!   its line is added to the EASYEDA table, aligned by hand (HARDWARE_LESSONS "3D models").

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::sexpr::{Sexp, dumps, parse};
use crate::{kicad_cli, node};

/// The library name both imported symbols and footprints live in.
pub const LIB: &str = "pcbgen";
pub const TOOL: &str = "easyeda2kicad 1.0.1";

/// One `[[part]]` of `lib/IMPORTED.toml`.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Imported {
    pub lcsc: String,
    pub mpn: String,
    /// `pcbgen:<name>`.
    pub symbol: String,
    /// `pcbgen:<name>`.
    pub footprint: String,
    /// EasyEDA's own footprint name, which says the package it was drawn for.
    #[serde(default)]
    pub easyeda_footprint: String,
    /// Date of the import, YYYY-MM-DD.
    pub date: String,
    #[serde(default)]
    pub tool: String,
    /// "UNVERIFIED" until a datasheet check, then "VERIFIED".
    pub status: String,
    /// Who checked it against which datasheet (page/table), empty until then.
    #[serde(default)]
    pub checked_by: String,
}

impl Imported {
    pub fn verified(&self) -> bool {
        self.status == "VERIFIED"
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct ImportedFile {
    #[serde(default)]
    part: Vec<Imported>,
}

pub fn imported_path(root: &Path) -> PathBuf {
    root.join("lib").join("IMPORTED.toml")
}
pub fn symbol_lib_path(root: &Path) -> PathBuf {
    root.join("lib").join("symbols").join(format!("{LIB}.kicad_sym"))
}
pub fn footprint_dir(root: &Path) -> PathBuf {
    root.join("lib").join("footprints").join(format!("{LIB}.pretty"))
}

/// Every entry of `lib/IMPORTED.toml` (none when the file doesn't exist).
pub fn load(root: &Path) -> Result<Vec<Imported>> {
    let path = imported_path(root);
    if !path.exists() {
        return Ok(vec![]);
    }
    let f: ImportedFile = toml::from_str(&std::fs::read_to_string(&path)?).with_context(|| path.display().to_string())?;
    for p in &f.part {
        if !matches!(p.status.as_str(), "UNVERIFIED" | "VERIFIED") {
            bail!("{}: {} has status {:?}; use \"UNVERIFIED\" or \"VERIFIED\"", path.display(), p.lcsc, p.status);
        }
        if p.verified() && p.checked_by.trim().is_empty() {
            bail!("{}: {} is VERIFIED but checked_by is empty (who checked it, against which datasheet page)", path.display(), p.lcsc);
        }
    }
    Ok(f.part)
}

/// The name an MPN is imported under: letters, digits, `.`, `-` and `_` kept, anything else `_`.
pub fn stable_name(mpn: &str) -> String {
    let s: String = mpn.trim().chars().map(|c| if c.is_ascii_alphanumeric() || "._-".contains(c) { c } else { '_' }).collect();
    s.trim_matches('_').to_string()
}

/// The `[[part]]` block appended to IMPORTED.toml.
pub fn entry_toml(e: &Imported) -> String {
    let q = |s: &str| toml::Value::String(s.to_string()).to_string();
    format!(
        "\n[[part]]\nlcsc = {}\nmpn = {}\nsymbol = {}\nfootprint = {}\neasyeda_footprint = {}\ndate = {}\ntool = {}\nstatus = {}\nchecked_by = {}\n",
        q(&e.lcsc),
        q(&e.mpn),
        q(&e.symbol),
        q(&e.footprint),
        q(&e.easyeda_footprint),
        q(&e.date),
        q(&e.tool),
        q(&e.status),
        q(&e.checked_by)
    )
}

const IMPORTED_HEADER: &str = "# Symbols and footprints imported with `pcb lib import` (easyeda2kicad) into the `pcbgen`
# libraries (lib/symbols/pcbgen.kicad_sym, lib/footprints/pcbgen.pretty). Each stays
# UNVERIFIED, and a red item on the readiness page of every board using it, until a datasheet
# check confirms its pins and pads: then status = \"VERIFIED\" and checked_by says who and
# against which datasheet page.
";

/// What the import changed in the symbol, for the report.
#[derive(Debug, Default, PartialEq)]
pub struct SymbolChanges {
    pub easyeda_name: String,
    pub pins: usize,
    pub unspecified_to_passive: usize,
}

/// The one symbol in easyeda2kicad's (upgraded) library, renamed to `name`, its footprint set
/// to `pcbgen:<name>`, its LCSC/MPN/manufacturer fields dropped, `unspecified` pins passive.
pub fn prepare_symbol(lib_text: &str, name: &str) -> Result<(Sexp, SymbolChanges)> {
    let lib = parse(lib_text)?;
    let syms: Vec<&Sexp> = lib.find_all("symbol").collect();
    let [sym] = syms[..] else {
        bail!("expected one symbol from easyeda2kicad, got {}", syms.len());
    };
    let mut sym = sym.clone();
    if sym.find("extends").is_some() {
        bail!("the symbol extends another one; not supported");
    }
    let old = sym.arg(1).unwrap_or_default().to_string();
    let mut ch = SymbolChanges { easyeda_name: old.clone(), ..Default::default() };
    sym.items_mut()[1] = Sexp::Str(name.into());
    sym.items_mut().retain(|c| !(c.is("property") && matches!(c.arg(1), Some("LCSC Part" | "MPN" | "Manufacturer"))));
    for c in sym.items_mut().iter_mut() {
        if c.is("property") && c.arg(1) == Some("Footprint") {
            c.items_mut()[2] = Sexp::Str(format!("{LIB}:{name}"));
        }
        if c.is("symbol") {
            let sub = c.arg(1).unwrap_or_default().to_string();
            let Some(rest) = sub.strip_prefix(&old) else { bail!("unit {sub:?} is not named after {old:?}") };
            c.items_mut()[1] = Sexp::Str(format!("{name}{rest}"));
        }
    }
    sym.walk_mut(&mut |n| {
        if n.is("pin") {
            ch.pins += 1;
            if n.arg(1) == Some("unspecified") {
                n.items_mut()[1] = Sexp::Sym("passive".into());
                ch.unspecified_to_passive += 1;
            }
        }
    });
    if ch.pins == 0 {
        bail!("the symbol has no pins");
    }
    Ok((sym, ch))
}

/// The pcbgen symbol library with `sym` added (a new library when `existing` is None).
/// Refuses a name the library already has.
pub fn merge_symbol(existing: Option<&str>, sym: Sexp) -> Result<String> {
    let mut lib = match existing {
        Some(t) => parse(t)?,
        None => node!("kicad_symbol_lib", node!("version", 20251024), node!("generator", "pcbgen")),
    };
    let name = sym.arg(1).unwrap_or_default();
    if lib.find_all("symbol").any(|s| s.arg(1) == Some(name)) {
        bail!("{LIB}.kicad_sym already has a symbol {name:?}");
    }
    lib.push(sym);
    Ok(dumps(&lib) + "\n")
}

/// easyeda2kicad's (upgraded) footprint renamed to `name`, with its model pointed at
/// `${KICAD10_3DMODEL_DIR}/pcbgen.3dshapes/<name>.step` and its LCSC field dropped.
pub fn prepare_footprint(text: &str, name: &str) -> Result<Sexp> {
    let mut fp = parse(text)?;
    if !fp.is("footprint") {
        bail!("not a KiCad footprint (head {:?})", fp.head());
    }
    fp.items_mut()[1] = Sexp::Str(name.into());
    fp.items_mut().retain(|c| !(c.is("property") && c.arg(1) == Some("LCSC Part")));
    for c in fp.items_mut().iter_mut() {
        if c.is("property") && c.arg(1) == Some("Value") {
            c.items_mut()[2] = Sexp::Str(name.into());
        }
    }
    fp.remove_all("model");
    fp.push(node!(
        "model",
        model_path(name),
        node!("offset", node!("xyz", 0.0, 0.0, 0.0)),
        node!("scale", node!("xyz", 1.0, 1.0, 1.0)),
        node!("rotate", node!("xyz", 0.0, 0.0, 0.0))
    ));
    Ok(fp)
}

/// Where the footprint looks for its 3D model (`lib/3dmodels/pcbgen.3dshapes/<name>.step`
/// once `enclosure/models.py` has fetched and aligned it).
pub fn model_path(name: &str) -> String {
    format!("${{KICAD10_3DMODEL_DIR}}/{LIB}.3dshapes/{name}.step")
}

/// Copper pads reaching past the F.CrtYd outline's bounding box (easyeda2kicad's courtyards
/// are sometimes the body only), as "pad 1" lines. Empty when there is no courtyard either way.
pub fn pads_outside_courtyard(fp: &Sexp) -> Vec<String> {
    let mut crt = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for g in fp.items().iter().filter(|g| g.find("layer").and_then(|l| l.arg(1)) == Some("F.CrtYd")) {
        for key in ["start", "end", "center", "mid"] {
            if let Some(p) = g.find(key) {
                let (x, y) = (p.num(1), p.num(2));
                crt = (crt.0.min(x), crt.1.min(y), crt.2.max(x), crt.3.max(y));
            }
        }
        if g.is("fp_circle")
            && let (Some(c), Some(e)) = (g.find("center"), g.find("end"))
        {
            let r = (c.num(1) - e.num(1)).hypot(c.num(2) - e.num(2));
            crt = (crt.0.min(c.num(1) - r), crt.1.min(c.num(2) - r), crt.2.max(c.num(1) + r), crt.3.max(c.num(2) + r));
        }
    }
    if crt.0 > crt.2 {
        return vec!["no F.CrtYd outline at all".into()];
    }
    let mut out = vec![];
    for p in fp.find_all("pad") {
        let (Some(at), Some(size)) = (p.find("at"), p.find("size")) else { continue };
        let rot = if at.items().len() > 3 { at.num(3).rem_euclid(180.0) } else { 0.0 };
        let (w, h) = if (rot - 90.0).abs() < 1.0 { (size.num(2), size.num(1)) } else { (size.num(1), size.num(2)) };
        let (x, y) = (at.num(1), at.num(2));
        let eps = 1e-6;
        if x - w / 2.0 < crt.0 - eps || x + w / 2.0 > crt.2 + eps || y - h / 2.0 < crt.1 - eps || y + h / 2.0 > crt.3 + eps {
            out.push(format!("pad {}", p.arg(1).unwrap_or("?")));
        }
    }
    out
}

/// One red readiness line per IMPORTED.toml entry not yet VERIFIED that `parts` uses.
/// `parts` = (reference, symbol lib_id, footprint).
pub fn unverified_used<'a>(entries: &[Imported], parts: impl Iterator<Item = (&'a str, &'a str, &'a str)> + Clone) -> Vec<String> {
    let mut out = vec![];
    for e in entries.iter().filter(|e| !e.verified()) {
        let refs: Vec<&str> = parts.clone().filter(|(_, sym, fp)| *sym == e.symbol || *fp == e.footprint).map(|(r, _, _)| r).collect();
        if !refs.is_empty() {
            out.push(format!(
                "{} ({}, LCSC {}): its symbol and footprint were imported from EasyEDA and are not yet checked against the datasheet (lib/IMPORTED.toml says UNVERIFIED). A wrong pin or pad would need a new board.",
                refs.join(", "),
                e.mpn,
                e.lcsc
            ));
        }
    }
    out
}

/// The readiness page's items for a circuit: `unverified_used` over the repo's IMPORTED.toml.
/// An unreadable IMPORTED.toml is itself a red item.
pub fn unverified_risks(c: &crate::circuit::Circuit) -> Vec<String> {
    match load(&crate::repo_root()) {
        Ok(entries) => unverified_used(&entries, c.parts.iter().map(|p| (p.reference.as_str(), p.lib_id.as_str(), p.footprint.as_str()))),
        Err(e) => vec![format!("lib/IMPORTED.toml can't be read, so imported parts can't be checked: {e:#}")],
    }
}

/// What `import` did, for the command's report.
#[derive(Debug)]
pub struct Report {
    pub entry: Imported,
    pub symbol: SymbolChanges,
    pub pads_outside_courtyard: Vec<String>,
    /// The line to add to `EASYEDA` in `enclosure/models.py`.
    pub models_line: String,
}

/// easyeda2kicad in the enclosure venv (`cd enclosure && uv sync` installs it).
pub fn easyeda2kicad(root: &Path) -> Result<PathBuf> {
    let exe = root.join("enclosure/.venv/bin/easyeda2kicad");
    if !exe.exists() {
        bail!("{} is missing: run `cd enclosure && uv sync`", exe.display());
    }
    Ok(exe)
}

/// Import `lcsc` into the pcbgen libraries under `root` (see the module docs). Nothing in
/// `lib/` changes unless every step before the writes succeeded.
pub fn import(root: &Path, lcsc: &str, jlc: &crate::jlc::Jlc) -> Result<Report> {
    let lcsc = lcsc.trim().to_uppercase();
    if !(lcsc.len() > 1 && lcsc.starts_with('C') && lcsc[1..].chars().all(|c| c.is_ascii_digit())) {
        bail!("{lcsc:?} is not an LCSC part number (C followed by digits)");
    }
    let entries = load(root)?;
    if let Some(e) = entries.iter().find(|e| e.lcsc == lcsc) {
        bail!("{lcsc} is already imported as {} (lib/IMPORTED.toml)", e.symbol);
    }
    let listing = jlc.get(&lcsc)?;
    let name = stable_name(&listing.mpn);
    if name.is_empty() {
        bail!("{lcsc} has no usable MPN in JLCPCB's listing ({:?})", listing.mpn);
    }
    let (sym_lib, fp_dir) = (symbol_lib_path(root), footprint_dir(root));
    let fp_path = fp_dir.join(format!("{name}.kicad_mod"));
    if fp_path.exists() {
        bail!("{} already exists; refusing to import over it", fp_path.display());
    }
    if let Some(e) = entries.iter().find(|e| e.symbol == format!("{LIB}:{name}") || e.footprint == format!("{LIB}:{name}")) {
        bail!("the name {LIB}:{name} is taken by {} in lib/IMPORTED.toml", e.lcsc);
    }
    let existing = if sym_lib.exists() { Some(std::fs::read_to_string(&sym_lib)?) } else { None };

    let scratch = std::env::temp_dir().join(format!("pcbgen-import-{lcsc}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch)?;
    let result = (|| {
        let out = Command::new(easyeda2kicad(root)?)
            .args(["--symbol", "--footprint", "--lcsc_id", &lcsc, "--output"])
            .arg(scratch.join("e2k"))
            .current_dir(&scratch)
            .output()
            .context("running easyeda2kicad")?;
        let log = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
        let mods: Vec<PathBuf> = std::fs::read_dir(scratch.join("e2k.pretty"))
            .map(|d| d.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|x| x == "kicad_mod")).collect())
            .unwrap_or_default();
        if !out.status.success() || !scratch.join("e2k.kicad_sym").exists() || mods.len() != 1 {
            bail!("easyeda2kicad gave no symbol and single footprint for {lcsc}:\n{log}");
        }
        let easyeda_fp = mods[0].file_stem().unwrap_or_default().to_string_lossy().into_owned();
        let s = |p: &Path| p.to_string_lossy().into_owned();
        kicad_cli(&["sym", "upgrade", "--force", &s(&scratch.join("e2k.kicad_sym")), "-o", &s(&scratch.join("sym.kicad_sym"))])?;
        kicad_cli(&["fp", "upgrade", "--force", &s(&scratch.join("e2k.pretty")), "-o", &s(&scratch.join("fp.pretty"))])?;
        let (sym, changes) = prepare_symbol(&std::fs::read_to_string(scratch.join("sym.kicad_sym"))?, &name)?;
        let fp = prepare_footprint(&std::fs::read_to_string(scratch.join("fp.pretty").join(format!("{easyeda_fp}.kicad_mod")))?, &name)?;
        let outside = pads_outside_courtyard(&fp);

        // both through KiCad once more, so what lands in lib/ is what KiCad wrote
        std::fs::write(scratch.join("merged.kicad_sym"), merge_symbol(existing.as_deref(), sym)?)?;
        kicad_cli(&["sym", "upgrade", "--force", &s(&scratch.join("merged.kicad_sym")), "-o", &s(&scratch.join("final.kicad_sym"))])?;
        let one = scratch.join("one.pretty");
        std::fs::create_dir_all(&one)?;
        std::fs::write(one.join(format!("{name}.kicad_mod")), dumps(&fp) + "\n")?;
        kicad_cli(&["fp", "upgrade", "--force", &s(&one), "-o", &s(&scratch.join("final.pretty"))])?;
        let final_fp = scratch.join("final.pretty").join(format!("{name}.kicad_mod"));
        let final_sym = std::fs::read_to_string(scratch.join("final.kicad_sym"))?;
        if !parse(&final_sym)?.find_all("symbol").any(|x| x.arg(1) == Some(name.as_str())) {
            bail!("KiCad's re-saved library lost the symbol {name}");
        }
        let entry = Imported {
            lcsc: lcsc.clone(),
            mpn: listing.mpn.clone(),
            symbol: format!("{LIB}:{name}"),
            footprint: format!("{LIB}:{name}"),
            easyeda_footprint: easyeda_fp,
            date: crate::schematic::today(),
            tool: TOOL.into(),
            status: "UNVERIFIED".into(),
            checked_by: String::new(),
        };

        // the writes, last
        std::fs::create_dir_all(sym_lib.parent().unwrap())?;
        std::fs::create_dir_all(&fp_dir)?;
        std::fs::copy(&final_fp, &fp_path)?;
        std::fs::write(&sym_lib, final_sym)?;
        let path = imported_path(root);
        let mut text = if path.exists() { std::fs::read_to_string(&path)? } else { IMPORTED_HEADER.to_string() };
        text += &entry_toml(&entry);
        std::fs::write(&path, text)?;

        let models_line = format!(
            "{{\"path\": \"{LIB}.3dshapes/{name}.step\", \"lcsc\": \"{lcsc}\", \"rot_z\": 0, \"move\": [0.0, 0.0, 0.0], \"why\": \"UNVERIFIED: align by hand against the pads and F.Fab\"}},"
        );
        Ok(Report { entry, symbol: changes, pads_outside_courtyard: outside, models_line })
    })();
    let _ = std::fs::remove_dir_all(&scratch);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    const SYM: &str = include_str!("../tests/data/easyeda-C181692.kicad_sym");
    const FP: &str = include_str!("../tests/data/easyeda-C181692.kicad_mod");

    #[test]
    fn names() {
        assert_eq!(stable_name("IP5306"), "IP5306");
        assert_eq!(stable_name(" TPS63020DSJR "), "TPS63020DSJR");
        assert_eq!(stable_name("LTV-217(TP1,F)"), "LTV-217_TP1_F");
        assert_eq!(stable_name("SY8089A1AAC/R"), "SY8089A1AAC_R");
    }

    #[test]
    fn symbol_renamed_and_cleaned() {
        let (sym, ch) = prepare_symbol(SYM, "IP5306X").unwrap();
        assert_eq!(ch, SymbolChanges { easyeda_name: "IP5306".into(), pins: 9, unspecified_to_passive: 9 });
        assert_eq!(sym.arg(1), Some("IP5306X"));
        let props: Vec<&str> = sym.find_all("property").filter_map(|p| p.arg(1)).collect();
        assert!(!props.iter().any(|p| ["LCSC Part", "MPN", "Manufacturer"].contains(p)), "{props:?}");
        let fp = sym.find_all("property").find(|p| p.arg(1) == Some("Footprint")).unwrap();
        assert_eq!(fp.arg(2), Some("pcbgen:IP5306X"));
        assert!(sym.find_all("symbol").all(|u| u.arg(1).unwrap().starts_with("IP5306X_")));
        let text = dumps(&sym);
        assert!(!text.contains("unspecified"));
    }

    #[test]
    fn merge_refuses_a_taken_name() {
        let (sym, _) = prepare_symbol(SYM, "IP5306").unwrap();
        let lib = merge_symbol(None, sym.clone()).unwrap();
        assert_eq!(parse(&lib).unwrap().find_all("symbol").count(), 1);
        let err = merge_symbol(Some(&lib), sym).unwrap_err().to_string();
        assert!(err.contains("already has a symbol \"IP5306\""), "{err}");
        let (other, _) = prepare_symbol(SYM, "OTHER").unwrap();
        let two = merge_symbol(Some(&lib), other).unwrap();
        let names: Vec<String> = parse(&two).unwrap().find_all("symbol").map(|s| s.arg(1).unwrap().to_string()).collect();
        assert_eq!(names, ["IP5306", "OTHER"]);
    }

    #[test]
    fn footprint_renamed_with_repo_model() {
        let fp = prepare_footprint(FP, "IP5306").unwrap();
        assert_eq!(fp.arg(1), Some("IP5306"));
        let models: Vec<&Sexp> = fp.find_all("model").collect();
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].arg(1), Some("${KICAD10_3DMODEL_DIR}/pcbgen.3dshapes/IP5306.step"));
        assert!(!dumps(&fp).contains("/tmp/"));
        assert!(!fp.find_all("property").any(|p| p.arg(1) == Some("LCSC Part")));
        assert_eq!(fp.find_all("pad").count(), 9);
    }

    #[test]
    fn easyeda_courtyard_misses_the_pads() {
        // C181692's courtyard is the 4.9 x 3.9 body; the gull-wing pads reach y = +-3.51
        let fp = prepare_footprint(FP, "IP5306").unwrap();
        let out = pads_outside_courtyard(&fp);
        assert_eq!(out, (1..=8).map(|n| format!("pad {n}")).collect::<Vec<_>>());
    }

    #[test]
    fn entries_round_trip_and_flag_unverified_parts() {
        let e = Imported {
            lcsc: "C181692".into(),
            mpn: "IP5306".into(),
            symbol: "pcbgen:IP5306".into(),
            footprint: "pcbgen:IP5306".into(),
            easyeda_footprint: "ESOP-8_L4.9-W3.9-P1.27-LS6.0-BL-EP".into(),
            date: "2026-10-01".into(),
            tool: TOOL.into(),
            status: "UNVERIFIED".into(),
            checked_by: String::new(),
        };
        let text = IMPORTED_HEADER.to_string() + &entry_toml(&e);
        let back: ImportedFile = toml::from_str(&text).unwrap();
        assert_eq!(back.part, vec![e.clone()]);

        let parts = [("U5", "pcbgen:IP5306", "pcbgen:IP5306"), ("R1", "Device:R", "Resistor_SMD:R_0805_2012Metric")];
        let red = unverified_used(std::slice::from_ref(&e), parts.iter().copied());
        assert_eq!(red.len(), 1);
        assert!(red[0].starts_with("U5 (IP5306, LCSC C181692)"), "{}", red[0]);
        // a footprint used with another symbol still counts
        let fp_only = [("U7", "Regulator_Linear:XC6220B331MR", "pcbgen:IP5306")];
        assert_eq!(unverified_used(std::slice::from_ref(&e), fp_only.iter().copied()).len(), 1);
        // not used, or verified: nothing
        assert!(unverified_used(std::slice::from_ref(&e), parts[1..].iter().copied()).is_empty());
        let v = Imported { status: "VERIFIED".into(), checked_by: "datasheet agent, IP5306 datasheet p.3".into(), ..e };
        assert!(unverified_used(&[v], parts.iter().copied()).is_empty());
    }

    #[test]
    fn load_rejects_bad_status() {
        let root = std::env::temp_dir().join(format!("pcbgen-imported-{}", std::process::id()));
        std::fs::create_dir_all(root.join("lib")).unwrap();
        assert!(load(&root).unwrap().is_empty());
        let base = "[[part]]\nlcsc = \"C1\"\nmpn = \"X\"\nsymbol = \"pcbgen:X\"\nfootprint = \"pcbgen:X\"\ndate = \"2026-10-01\"\n";
        std::fs::write(imported_path(&root), format!("{base}status = \"checked\"\n")).unwrap();
        assert!(load(&root).unwrap_err().to_string().contains("status"));
        std::fs::write(imported_path(&root), format!("{base}status = \"VERIFIED\"\n")).unwrap();
        assert!(load(&root).unwrap_err().to_string().contains("checked_by is empty"));
        std::fs::write(imported_path(&root), format!("{base}status = \"UNVERIFIED\"\n")).unwrap();
        assert_eq!(load(&root).unwrap().len(), 1);
        std::fs::remove_dir_all(&root).unwrap();
    }
}
