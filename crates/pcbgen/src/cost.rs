//! Cost of one design as its own JLCPCB order (D-021 option A), from the BOM that would
//! be uploaded (`fab/<name>-bom.csv`) and live prices (D-023).
//!
//! Prices, stock, minimums and attrition come from JLCPCB's public parts-search API, the
//! one used in `research/2026-09-29-cost-estimate.md` §1 (no login; undocumented, so a
//! change on their side shows up here as a fetch or parse error). Fixed fees are the
//! VERIFIED ones in `HARDWARE_LESSONS.md`. Shipping and VAT are per parcel, shared by
//! every design in it (D-021), so they are listed apart from the per-design total.

use std::collections::BTreeSet;
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use crate::board::Board;
use crate::circuit::Circuit;
pub use crate::jlc::{API, Listing};
use crate::jlc::{Jlc, mpn_matches};

/// (name, USD, source). JLCPCB Economic PCBA, 2-layer PCB up to 100×100 mm, qty 5.
pub const FEES: [(&str, f64, &str); 5] = [
    ("pcb", 4.00, "5 bare 2-layer PCBs up to 100x100 mm, quote form 2026-09-29"),
    ("setup", 8.18, "Economic PCBA setup, help page 2026-09-09"),
    ("stencil", 1.53, "Economic PCBA stencil, help page 2026-09-09"),
    ("per_joint", 0.0016, "per solder joint, help page 2026-09-09"),
    ("extended", 3.07, "per unique Extended part (Basic and Preferred Extended: none), help page 2026-09-09"),
];
/// FedEx to Israel up to 0.5 kg, per parcel (cost estimate §2, VERIFIED 2026-09-29).
pub const SHIPPING_FEDEX: f64 = 29.59;
/// Israeli VAT: 18% of goods + shipping once the goods pass $75 (HARDWARE_LESSONS).
pub const VAT_LINE: f64 = 75.0;
pub const VAT: f64 = 0.18;

fn fee(name: &str) -> f64 {
    FEES.iter().find(|f| f.0 == name).unwrap().1
}

/// A BOM line: (comment, refs, lcsc).
pub type BomLine = (String, Vec<String>, String);

pub fn read_bom(path: &Path) -> Result<Vec<BomLine>> {
    let mut r = csv::Reader::from_path(path).with_context(|| format!("reading {}", path.display()))?;
    let mut out = vec![];
    for rec in r.records() {
        let rec = rec?;
        let refs = rec.get(1).unwrap_or("").split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
        out.push((rec.get(0).unwrap_or("").into(), refs, rec.get(3).unwrap_or("").into()));
    }
    Ok(out)
}

/// The cost report: every line priced for `assembled` boards, the fees, the per-design
/// total, and the parcel costs if this design ships alone.
pub fn price(bom: &[BomLine], listings: &[Listing], joints: u64, boards: u64, assembled: u64, date: &str) -> Value {
    let mut lines = vec![];
    let (mut parts, mut extended) = (0.0, BTreeSet::new());
    for ((comment, refs, lcsc), l) in bom.iter().zip(listings) {
        let per_board = refs.len() as u64;
        let qty = l.order_qty(per_board * assembled);
        let unit = l.unit(qty);
        let line = unit * qty as f64;
        parts += line;
        if l.library == "extended" {
            extended.insert(lcsc.clone());
        }
        lines.push(json!({
            "lcsc": lcsc, "comment": comment, "refs": refs, "library": l.library, "stock": l.stock,
            "per_board": per_board, "minimum": l.minimum, "attrition": l.attrition, "order_qty": qty,
            "unit_usd": unit, "line_usd": round2(line),
            "short": l.stock < qty,
        }));
    }
    let ext_fees = extended.len() as f64 * fee("extended");
    let joint_fees = joints as f64 * assembled as f64 * fee("per_joint");
    let fixed = fee("pcb") + fee("setup") + fee("stencil");
    let design = parts + ext_fees + joint_fees + fixed;
    let vat = if design > VAT_LINE { VAT * (design + SHIPPING_FEDEX) } else { 0.0 };
    json!({
        "date": date,
        "source": API,
        "order": {"bare_boards": boards, "assembled": assembled},
        "lines": lines,
        "fees": FEES.iter().map(|(n, usd, src)| json!({"name": n, "usd": usd, "source": src})).collect::<Vec<_>>(),
        "joints_per_board": joints,
        "totals_usd": {
            "parts": round2(parts),
            "extended_fees": round2(ext_fees),
            "extended_parts": extended.into_iter().collect::<Vec<_>>(),
            "joints": round2(joint_fees),
            "pcb_setup_stencil": round2(fixed),
            "per_design": round2(design),
        },
        "alone_in_a_parcel_usd": {
            "shipping_fedex": SHIPPING_FEDEX,
            "vat": round2(vat),
            "total": round2(design + SHIPPING_FEDEX + vat),
        },
        "notes": [
            "Order quantity per line = parts for the assembled boards + attrition, at least the minimum (INFERRED model).",
            "Joints = pads of the assembled parts on the board (INFERRED; JLCPCB counts its own).",
            "Shipping and VAT are per parcel; several designs share one (D-021).",
        ],
    })
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// Pads of the assembled parts: the solder joints JLCPCB charges for (INFERRED count).
fn joints(pcb: &Path, bom: &[BomLine]) -> Result<u64> {
    let placed: BTreeSet<&str> = bom.iter().flat_map(|l| l.1.iter().map(String::as_str)).collect();
    let board = Board::load(pcb)?;
    Ok(board
        .footprints
        .iter()
        .filter(|f| placed.contains(f.reference.as_str()))
        .flat_map(|f| &f.pads)
        .filter(|p| p.kind == "smd" || p.kind == "thru_hole")
        .count() as u64)
}

/// The MPN a BOM line should carry: the `MPN` field of its parts in the circuit. None when
/// none of them has one; an error when they disagree.
pub fn line_mpn(line: &BomLine, c: &Circuit) -> std::result::Result<Option<String>, String> {
    let mut mpns: Vec<&str> = c.parts.iter().filter(|p| line.1.contains(&p.reference)).filter_map(|p| p.field("MPN")).collect();
    mpns.sort();
    mpns.dedup();
    match mpns[..] {
        [] => Ok(None),
        [one] => Ok(Some(one.to_string())),
        _ => Err(format!("{} ({}): its parts name different MPNs: {}", line.2, line.1.join(","), mpns.join(", "))),
    }
}

/// The PARTS gate, per BOM line: JLCPCB's MPN for the LCSC code must be the circuit's MPN
/// (a typo in an LCSC code lands on some other part), and JLCPCB must stock what the order
/// takes (parts for the assembled boards + attrition, at least the minimum).
/// (failures, warnings); a line whose parts have no MPN is a warning: nothing to check.
pub fn parts_gate(bom: &[BomLine], listings: &[Listing], mpns: &[std::result::Result<Option<String>, String>], assembled: u64) -> (Vec<String>, Vec<String>) {
    let (mut fail, mut warn) = (vec![], vec![]);
    for (((_, refs, lcsc), l), mpn) in bom.iter().zip(listings).zip(mpns) {
        let at = format!("{lcsc} ({})", refs.join(","));
        match mpn {
            Err(e) => fail.push(e.clone()),
            Ok(None) => warn.push(format!("{at}: no MPN in the circuit to check JLCPCB's \"{}\" against (add .mpn(...))", l.mpn)),
            Ok(Some(m)) if !mpn_matches(m, &l.mpn) => fail.push(format!(
                "{at}: the circuit says MPN \"{m}\" but JLCPCB lists {lcsc} as \"{}\" ({}, {}): wrong LCSC code or wrong MPN",
                l.mpn, l.manufacturer, l.package
            )),
            Ok(Some(_)) => {}
        }
        let qty = l.order_qty(refs.len() as u64 * assembled);
        if l.stock < qty {
            fail.push(format!("{at}: JLCPCB stocks {} but the order takes {qty}", l.stock));
        }
    }
    (fail, warn)
}

/// The `cost` stage: writes `<fab>/cost.json`, prints a summary and the PARTS gate.
/// Ok(false) when the gate fails (cost.json is still written).
pub fn run(project_dir: &Path, name: &str, fab: &Path, circuit: &Circuit, boards: u64, assembled: u64, jlc: &Jlc) -> Result<bool> {
    let bom_path = fab.join(format!("{name}-bom.csv"));
    if !bom_path.exists() {
        bail!("no {}; run the fab stage first", bom_path.display());
    }
    let bom = read_bom(&bom_path)?;
    let listings = bom.iter().map(|l| jlc.get(&l.2)).collect::<Result<Vec<_>>>()?;
    let j = joints(&project_dir.join(format!("{name}.kicad_pcb")), &bom)?;
    let report = price(&bom, &listings, j, boards, assembled, &crate::schematic::today());
    std::fs::write(fab.join("cost.json"), serde_json::to_string_pretty(&report)? + "\n")?;
    let t = &report["totals_usd"];
    let short: Vec<&str> = report["lines"].as_array().unwrap().iter().filter(|l| l["short"] == true).filter_map(|l| l["lcsc"].as_str()).collect();
    println!(
        "cost: {boards} bare, {assembled} assembled: parts ${}, Extended fees ${} ({} parts), PCB+setup+stencil ${}, joints ${} => ${} per design; alone in a parcel ${} with shipping and VAT",
        t["parts"], t["extended_fees"], t["extended_parts"].as_array().map_or(0, |a| a.len()), t["pcb_setup_stencil"], t["joints"], t["per_design"],
        report["alone_in_a_parcel_usd"]["total"]
    );
    if !short.is_empty() {
        println!("   SHORT: not enough JLCPCB stock for {}", short.join(", "));
    }
    let mpns: Vec<_> = bom.iter().map(|l| line_mpn(l, circuit)).collect();
    let (fail, warn) = parts_gate(&bom, &listings, &mpns, assembled);
    for w in &warn {
        println!("   unchecked: {w}");
    }
    for f in &fail {
        println!("   {f}");
    }
    let checked = mpns.iter().filter(|m| matches!(m, Ok(Some(_)))).count();
    println!(
        "== PARTS: {} ({} lines; {checked} MPNs checked against JLCPCB's listing, {} without an MPN; stock checked for {assembled} assembled)",
        if fail.is_empty() { "PASS" } else { "FAIL" },
        bom.len(),
        warn.len()
    );
    Ok(fail.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::jlc::listing;

    fn answer(code: &str, typ: &str, pref: bool) -> Value {
        json!({"code": 200, "data": {"componentPageInfo": {"list": [
            {"componentCode": "C999", "componentLibraryType": "base", "componentPrices": []},
            {"componentCode": code, "componentLibraryType": typ, "preferredComponentFlag": pref, "stockCount": 100,
             "leastPatchNumber": 20, "lossNumber": 10,
             "componentPrices": [{"startNumber": 1, "endNumber": 9, "productPrice": 0.5},
                                 {"startNumber": 10, "endNumber": 29, "productPrice": 0.4},
                                 {"startNumber": 30, "endNumber": -1, "productPrice": 0.3}]}
        ]}}})
    }

    #[test]
    fn reads_the_answer() {
        let l = listing(&answer("C1", "expand", true), "C1").unwrap();
        assert_eq!(l.library, "preferred");
        assert_eq!((l.stock, l.minimum, l.attrition), (100, 20, 10));
        assert_eq!(l.tiers[2], (30, None, 0.3));
        assert_eq!(listing(&answer("C1", "expand", false), "C1").unwrap().library, "extended");
        assert_eq!(listing(&answer("C1", "base", false), "C1").unwrap().library, "basic");
        assert!(listing(&answer("C1", "base", false), "C2").is_err(), "only an exact code match counts");
    }

    #[test]
    fn quantities_and_tiers() {
        let l = listing(&answer("C1", "base", false), "C1").unwrap();
        assert_eq!(l.order_qty(2), 20, "minimum");
        assert_eq!(l.order_qty(15), 25, "needed + attrition");
        assert_eq!(l.unit(5), 0.5);
        assert_eq!(l.unit(29), 0.4);
        assert_eq!(l.unit(1000), 0.3);
    }

    #[test]
    fn totals() {
        let bom: Vec<BomLine> = vec![
            ("mod".into(), vec!["U1".into()], "C1".into()),
            ("r".into(), vec!["R1".into(), "R2".into()], "C2".into()),
        ];
        let mut module = listing(&answer("C1", "expand", false), "C1").unwrap();
        (module.minimum, module.attrition) = (0, 0);
        let r = listing(&answer("C2", "base", false), "C2").unwrap();
        let v = price(&bom, &[module, r], 100, 5, 2, "2026-09-29");
        let t = &v["totals_usd"];
        // module: 2 at 0.5 = 1.00; resistors: max(4 + 10, 20) = 20 at 0.4 = 8.00
        assert_eq!(t["parts"], 9.0);
        assert_eq!(t["extended_fees"], 3.07);
        assert_eq!(t["joints"], 0.32); // 100 joints x 2 boards x 0.0016
        assert_eq!(t["pcb_setup_stencil"], 13.71);
        assert_eq!(t["per_design"], 26.1);
        assert_eq!(v["alone_in_a_parcel_usd"]["vat"], 0.0, "under the $75 line");
        assert_eq!(v["lines"][1]["order_qty"], 20);
        assert_eq!(v["lines"][0]["short"], false);
    }

    #[test]
    fn parts_gate_checks_mpn_and_stock() {
        let bom: Vec<BomLine> = vec![
            ("ESP32".into(), vec!["U1".into()], "C2913202".into()),
            ("10k".into(), vec!["R1".into(), "R2".into()], "C17414".into()),
            ("x".into(), vec!["D1".into()], "C1".into()),
        ];
        let l = |mpn: &str, stock: u64| Listing {
            mpn: mpn.into(),
            stock,
            minimum: 20,
            attrition: 10,
            tiers: vec![(1, None, 0.1)],
            ..Default::default()
        };
        let mpns = vec![Ok(Some("ESP32-S3-WROOM-1-N16R8".to_string())), Ok(Some("0805W8F1002T5E".to_string())), Ok(None)];
        let good = [l("ESP32-S3-WROOM-1-N16R8", 20), l("0805W8F1002T5E", 1000), l("whatever", 1000)];
        let (fail, warn) = parts_gate(&bom, &good, &mpns, 2);
        assert!(fail.is_empty(), "{fail:?}");
        assert_eq!(warn.len(), 1, "a line without an MPN is a warning");
        assert!(warn[0].starts_with("C1 (D1)"));

        // a typo in the LCSC code lands on another part: JLCPCB's MPN differs
        let typo = [l("ESP32-S3-WROOM-1-N8", 1000), l("0805W8F1002T5E", 1000), l("whatever", 1000)];
        let (fail, _) = parts_gate(&bom, &typo, &mpns, 2);
        assert_eq!(fail.len(), 1);
        assert!(fail[0].contains("C2913202 (U1)") && fail[0].contains("\"ESP32-S3-WROOM-1-N8\""), "{fail:?}");

        // stock: 2 boards x 2 resistors + 10 attrition = 14, at least the minimum 20
        let short = [l("ESP32-S3-WROOM-1-N16R8", 1000), l("0805W8F1002T5E", 19), l("whatever", 1000)];
        let (fail, _) = parts_gate(&bom, &short, &mpns, 2);
        assert_eq!(fail, vec!["C17414 (R1,R2): JLCPCB stocks 19 but the order takes 20".to_string()]);

        let (fail, _) = parts_gate(&bom, &good, &[Err("disagree".into()), Ok(None), Ok(None)], 2);
        assert_eq!(fail, vec!["disagree".to_string()]);
    }
}
