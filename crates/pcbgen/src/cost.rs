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
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use crate::board::Board;

pub const API: &str = "https://jlcpcb.com/api/overseas-pcb-order/v1/shoppingCart/smtGood/selectSmtComponentList";

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

/// One BOM line as JLCPCB lists it.
#[derive(Clone, Debug)]
pub struct Listing {
    /// "basic", "preferred" (Preferred Extended) or "extended".
    pub library: String,
    pub stock: u64,
    /// Minimum order (`leastPatchNumber`) and attrition (`lossNumber`) for assembly.
    pub minimum: u64,
    pub attrition: u64,
    /// (from qty, to qty or None for no limit, USD each).
    pub tiers: Vec<(u64, Option<u64>, f64)>,
}

/// The listing for one LCSC code, from the API's JSON answer.
pub fn listing(answer: &Value, lcsc: &str) -> Result<Listing> {
    let list = answer["data"]["componentPageInfo"]["list"].as_array().context("no data.componentPageInfo.list in the answer")?;
    let c = list.iter().find(|c| c["componentCode"] == lcsc).with_context(|| format!("{lcsc} not in JLCPCB's parts list"))?;
    let library = match (c["componentLibraryType"].as_str(), c["preferredComponentFlag"].as_bool()) {
        (Some("base"), _) => "basic",
        (Some("expand"), Some(true)) => "preferred",
        (Some("expand"), _) => "extended",
        (t, _) => bail!("{lcsc}: unknown library type {t:?}"),
    };
    let tiers = c["componentPrices"]
        .as_array()
        .context("no componentPrices")?
        .iter()
        .map(|t| {
            let to = t["endNumber"].as_i64().filter(|&n| n >= 0).map(|n| n as u64);
            Ok((t["startNumber"].as_u64().context("startNumber")?, to, t["productPrice"].as_f64().context("productPrice")?))
        })
        .collect::<Result<Vec<_>>>()?;
    if tiers.is_empty() {
        bail!("{lcsc}: no price tiers");
    }
    Ok(Listing {
        library: library.into(),
        stock: c["stockCount"].as_u64().unwrap_or(0),
        minimum: c["leastPatchNumber"].as_u64().unwrap_or(0),
        attrition: c["lossNumber"].as_u64().unwrap_or(0),
        tiers,
    })
}

impl Listing {
    /// What JLCPCB buys for `needed` placements: needed + attrition, at least the
    /// minimum (the model in the cost estimate, INFERRED).
    pub fn order_qty(&self, needed: u64) -> u64 {
        (needed + self.attrition).max(self.minimum)
    }
    /// Unit price at a quantity: the tier that covers it, else the nearest one.
    pub fn unit(&self, qty: u64) -> f64 {
        let covers = |t: &&(u64, Option<u64>, f64)| t.0 <= qty && t.1.is_none_or(|to| qty <= to);
        self.tiers.iter().find(covers).or_else(|| self.tiers.iter().rfind(|t| t.0 <= qty)).unwrap_or(&self.tiers[0]).2
    }
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

/// One part's listing from JLCPCB (curl, like kicad-cli is run: no HTTP crate).
fn fetch(lcsc: &str) -> Result<Listing> {
    let body = json!({"keyword": lcsc, "currentPage": 1, "pageSize": 10}).to_string();
    let out = Command::new("curl")
        .args(["-sS", "-f", "-m", "30", "--retry", "2", "-X", "POST", API, "-H", "Content-Type: application/json"])
        .args(["-H", "User-Agent: Mozilla/5.0 (X11; Linux x86_64)", "-d", &body])
        .output()
        .context("running curl")?;
    if !out.status.success() {
        bail!("{lcsc}: JLCPCB parts API: {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    let answer: Value = serde_json::from_slice(&out.stdout).with_context(|| format!("{lcsc}: JLCPCB's answer is not JSON"))?;
    listing(&answer, lcsc)
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

/// The `cost` stage: writes `<fab>/cost.json` and prints a summary.
pub fn run(project_dir: &Path, name: &str, fab: &Path, boards: u64, assembled: u64) -> Result<Value> {
    let bom_path = fab.join(format!("{name}-bom.csv"));
    if !bom_path.exists() {
        bail!("no {}; run the fab stage first", bom_path.display());
    }
    let bom = read_bom(&bom_path)?;
    let listings = bom.iter().map(|l| fetch(&l.2)).collect::<Result<Vec<_>>>()?;
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
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
