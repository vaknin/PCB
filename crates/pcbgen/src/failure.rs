//! Routing failures (D-020): when no checked footprint order passes the DRC gate, the
//! report of what failed, where, in how many orders and what to try (`route/failure.json`),
//! and the board's log of failed orders across runs (`<board>/route-failures.jsonl`).
//!
//! The same fault in every checked order is a placement or rules problem; in only some
//! of them, routing luck. Each line of the log is one failed order: date, board, order,
//! and its open items' types and parts. An error type that turns up on two boards gets a
//! prevention rule (a constraint the router gets, e.g. `RouteOptions::pad_rings`) and a
//! `HARDWARE_LESSONS.md` entry.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::LazyLock;

use anyhow::Result;
use regex::Regex;
use serde_json::{Value, json};

use crate::geom::Pt;

fn s(v: &Value) -> &str {
    v.as_str().unwrap_or("")
}

fn item_texts(v: &Value) -> impl Iterator<Item = &str> {
    v["items"].as_array().into_iter().flatten().map(|i| s(&i["description"]))
}

/// An item as it looks in every order: tracks and arcs differ in length between routes.
fn normal(desc: &str) -> String {
    static LEN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r",? length [0-9.]+ ?mm").unwrap());
    LEN.replace_all(desc, "").into_owned()
}

/// The same fault in two orders has the same signature: its type and its items, with
/// the details that change from route to route taken out.
pub fn signature(v: &Value) -> String {
    let items: BTreeSet<String> = item_texts(v).map(normal).collect();
    format!("{}: {}", s(&v["type"]), items.into_iter().collect::<Vec<_>>().join("; "))
}

/// References of the parts an item involves ("Pad 1 [GND] of J1 on F.Cu", "Footprint U4").
pub fn parts(v: &Value) -> Vec<String> {
    static REF: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?:\bof|^Footprint) ([A-Za-z_#]+[0-9]+[A-Za-z0-9_]*)").unwrap());
    let set: BTreeSet<String> = item_texts(v).flat_map(|t| REF.captures_iter(t).map(|c| c[1].to_string()).collect::<Vec<_>>()).collect();
    set.into_iter().collect()
}

/// (reference, pad number) of the pads an item involves.
pub fn pads(v: &Value) -> Vec<(String, String)> {
    static PAD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b[Pp]ad (\S+)(?: \[[^\]]*\])? of ([A-Za-z_#]+[0-9]+[A-Za-z0-9_]*)").unwrap());
    let set: BTreeSet<(String, String)> =
        item_texts(v).flat_map(|t| PAD.captures_iter(t).map(|c| (c[2].to_string(), c[1].to_string())).collect::<Vec<_>>()).collect();
    set.into_iter().collect()
}

/// What to try, by DRC type. `{parts}` and `{pads}` are filled in; anything not listed
/// gets "move the parts involved".
const FIXES: &[(&[&str], &str)] = &[
    (
        &["starved_thermal"],
        "A track or via cuts the pour's thermal spokes to {pads}. Keep the router off them: add a pad ring for it to \
         `RouteOptions::pad_rings` (`PadRing::new`, D-020), or move {parts} or its neighbours so the pour has room.",
    ),
    (&["clearance"], "Copper too close together. Widen the gap: move {parts} apart, or give the router more room around them."),
    (
        &["unconnected_items"],
        "A connection was not finished. Give the router room around {parts} (placement, net-class widths), or route more orders.",
    ),
    (&["hole_clearance", "hole_to_hole", "drilled_holes_too_close", "drilled_holes_colocated"], "Holes too close to copper or to each other. Move {parts}."),
    (&["copper_edge_clearance", "edge_clearance"], "Copper too close to the board edge. Move {parts} inward."),
    (&["courtyards_overlap"], "Parts overlap. Move {parts} apart; this is placement, routing can't fix it."),
    (&["tracks_crossing", "shorting_items"], "Two nets touch. Route more orders; if it repeats, move {parts} to open a path."),
    (&["isolated_copper"], "An island of pour with no connection. Move {parts} to let the pour through, or stitch it."),
    (
        &["track_width", "via_diameter", "annular_width", "drill_out_of_range", "hole_size", "connection_width"],
        "Below the fab's minimum. Fix the net class or via size in layout.rs.",
    ),
    (
        &["silk_over_copper", "silk_overlap", "silk_edge_clearance", "silk_mask_clearance", "text_height", "text_thickness"],
        "Silkscreen only (cosmetic). Move the label or {parts}, or waive it with a reason in `Layout::waivers`.",
    ),
    (
        &["missing_footprint", "extra_footprint", "duplicate_footprints", "net_conflict", "footprint_symbol_mismatch", "footprint_filters_mismatch"],
        "The board and the schematic disagree. Re-run the sch and pcb stages; if it stays, the circuit or layout names {parts} wrongly.",
    ),
];

pub fn fix(v: &Value) -> String {
    let kind = s(&v["type"]);
    let parts = parts(v);
    let pads: Vec<String> = pads(v).into_iter().map(|(r, p)| format!("{r} pad {p}")).collect();
    let text = match FIXES.iter().find(|(kinds, _)| kinds.contains(&kind)) {
        Some((_, t)) => t.to_string(),
        None if parts.is_empty() => "No fix on file for this type; read the item in the DRC report.".to_string(),
        None => "Move {parts}.".to_string(),
    };
    let or = |v: Vec<String>, none: &str| if v.is_empty() { none.to_string() } else { v.join(", ") };
    text.replace("{parts}", &or(parts, "the parts involved")).replace("{pads}", &or(pads, "the pad"))
}

/// Round to 0.01 mm.
fn r2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// The failure report. `checked`: each checked order's open DRC items, best-ranked first;
/// `origin`: the board's top-left corner (item positions are given from it, as a layout
/// places parts); `steps`: what the route stage tried.
pub fn report(board: &str, date: &str, origin: Pt, checked: &[(u64, Vec<Value>)], kept: u64, steps: &[String]) -> Value {
    // signature → (an instance, preferably the kept order's; orders that hit it)
    let mut groups: Vec<(String, &Value, Vec<u64>)> = vec![];
    for (order, open) in checked {
        for v in open {
            let sig = signature(v);
            match groups.iter_mut().find(|g| g.0 == sig) {
                Some(g) => {
                    if !g.2.contains(order) {
                        g.2.push(*order);
                    }
                    if *order == kept {
                        g.1 = v;
                    }
                }
                None => groups.push((sig, v, vec![*order])),
            }
        }
    }
    let n = checked.len();
    // most widespread first; then the kept order's; then as found
    let mut idx: Vec<usize> = (0..groups.len()).collect();
    idx.sort_by_key(|&i| (std::cmp::Reverse(groups[i].2.len()), !groups[i].2.contains(&kept), i));
    let items: Vec<Value> = idx
        .iter()
        .map(|&i| {
            let (_, v, orders) = &groups[i];
            let pattern = if n < 2 {
                "only one order was checked"
            } else if orders.len() == n {
                "every checked order: placement or rules, not routing luck"
            } else {
                "some orders only: routing luck; another order or the fix avoids it"
            };
            let places: Vec<Value> = v["items"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|it| {
                    let at = it["pos"]["x"].as_f64().zip(it["pos"]["y"].as_f64()).map(|(x, y)| json!([r2(x - origin.x), r2(y - origin.y)]));
                    json!({"description": it["description"], "at_mm": at})
                })
                .collect();
            json!({
                "type": v["type"],
                "severity": v["severity"],
                "description": v["description"],
                "parts": parts(v),
                "items": places,
                "orders_hit": orders,
                "hits": format!("{} of {n}", orders.len()),
                "pattern": pattern,
                "in_kept_order": orders.contains(&kept),
                "fix": fix(v),
            })
        })
        .collect();
    json!({
        "board": board,
        "date": date,
        "kept_order": kept,
        "checked_orders": checked.iter().map(|c| c.0).collect::<Vec<_>>(),
        "steps": steps,
        "positions": "at_mm: mm from the board's top-left corner, Y down, as a layout places parts",
        "items": items,
    })
}

/// The report as lines for the terminal.
pub fn summary(report: &Value) -> Vec<String> {
    let items = report["items"].as_array().cloned().unwrap_or_default();
    let mut out = vec![format!(
        "route failure: no checked order passes DRC ({} orders checked); kept order {} ({} open item(s) there)",
        report["checked_orders"].as_array().map_or(0, Vec::len),
        report["kept_order"],
        items.iter().filter(|i| i["in_kept_order"] == true).count()
    )];
    for i in &items {
        let at: Vec<String> = i["items"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|it| it["at_mm"].as_array().map(|a| format!("({}, {})", a[0], a[1])))
            .collect();
        out.push(format!(
            "  [{}] {} in {} orders{}: {}{}",
            s(&i["severity"]),
            s(&i["type"]),
            s(&i["hits"]),
            if i["in_kept_order"] == true { ", kept order too" } else { "" },
            i["items"].as_array().into_iter().flatten().map(|it| s(&it["description"])).collect::<Vec<_>>().join("; "),
            if at.is_empty() { String::new() } else { format!(" at {}", at.join(", ")) }
        ));
        out.push(format!("      {}", s(&i["pattern"])));
        out.push(format!("      fix: {}", s(&i["fix"])));
    }
    out
}

/// Log lines, one per checked order that has open items. `layout`: a fingerprint of the
/// placement and rules (the DSN without its path), so a re-run of the same board isn't
/// logged twice.
pub fn log_lines(board: &str, date: &str, layout: &str, checked: &[(u64, Vec<Value>)]) -> Vec<Value> {
    checked
        .iter()
        .filter(|c| !c.1.is_empty())
        .map(|(order, open)| {
            let errors: Vec<Value> = open.iter().map(|v| json!({"type": v["type"], "parts": parts(v)})).collect();
            json!({"date": date, "board": board, "layout": layout, "order": order, "errors": errors})
        })
        .collect()
}

fn read_log(path: &Path) -> Vec<Value> {
    std::fs::read_to_string(path).unwrap_or_default().lines().filter_map(|l| serde_json::from_str(l).ok()).collect()
}

/// Append the lines not logged yet (same board, layout and order, whatever the date);
/// returns how many were added.
pub fn append_log(path: &Path, lines: &[Value]) -> Result<usize> {
    let old = read_log(path);
    let key = |v: &Value| (v["board"].clone(), v["layout"].clone(), v["order"].clone());
    let new: Vec<&Value> = lines.iter().filter(|l| !old.iter().any(|o| key(o) == key(l))).collect();
    if !new.is_empty() {
        let mut text = std::fs::read_to_string(path).unwrap_or_default();
        for l in &new {
            text.push_str(&serde_json::to_string(l)?);
            text.push('\n');
        }
        std::fs::write(path, text)?;
    }
    Ok(new.len())
}

/// Error type → (failed orders with it, boards with it), most frequent first.
pub fn tally(lines: &[Value]) -> Vec<(String, usize, BTreeSet<String>)> {
    let mut m: BTreeMap<String, (usize, BTreeSet<String>)> = BTreeMap::new();
    for l in lines {
        let kinds: BTreeSet<&str> = l["errors"].as_array().into_iter().flatten().map(|e| s(&e["type"])).collect();
        for k in kinds {
            let e = m.entry(k.to_string()).or_default();
            e.0 += 1;
            e.1.insert(s(&l["board"]).to_string());
        }
    }
    let mut v: Vec<_> = m.into_iter().map(|(k, (n, b))| (k, n, b)).collect();
    v.sort_by_key(|x| std::cmp::Reverse(x.1));
    v
}

/// Summary lines of this board's log, and of every sibling board's (`<boards>/*/`) for
/// error types that have now failed on two or more boards.
pub fn log_summary(log: &Path) -> Vec<String> {
    let mine = tally(&read_log(log));
    let mut out = vec![format!(
        "failure log {}: {}",
        log.display(),
        mine.iter().map(|(k, n, _)| format!("{k} in {n} order(s)")).collect::<Vec<_>>().join(", ")
    )];
    let mut all = vec![];
    let boards = log.parent().and_then(Path::parent);
    if let Some(Ok(rd)) = boards.map(std::fs::read_dir) {
        let mut dirs: Vec<_> = rd.flatten().map(|e| e.path().join("route-failures.jsonl")).filter(|p| p.exists()).collect();
        dirs.sort();
        for p in dirs {
            all.extend(read_log(&p));
        }
    }
    for (k, _, b) in tally(&all).into_iter().filter(|x| x.2.len() >= 2) {
        out.push(format!(
            "RULE (D-020): {k} has failed on {} boards ({}): make it a prevention rule the router gets, and add a HARDWARE_LESSONS.md entry",
            b.len(),
            b.into_iter().collect::<Vec<_>>().join(", ")
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::pt;

    /// Shaped as kicad-cli 10's DRC JSON (`violations[]`).
    fn drc() -> Value {
        serde_json::from_str(
            r#"[
  {"description": "Thermal relief connection to zone incomplete (zone min spoke count 2; actual 1)",
   "items": [{"description": "PTH pad SH [GND] of J1", "pos": {"x": 120.68, "y": 146.3}, "uuid": "a"}],
   "severity": "error", "type": "starved_thermal"},
  {"description": "Clearance violation (netclass 'Default' clearance 0.1500 mm; actual 0.1200 mm)",
   "items": [{"description": "Track [/I2C_SDA] on F.Cu, length 3.2100 mm", "pos": {"x": 140.0, "y": 130.0}, "uuid": "b"},
             {"description": "Pad 2 [+3V3] of R7 on F.Cu", "pos": {"x": 140.0, "y": 116.3}, "uuid": "c"}],
   "severity": "error", "type": "clearance"},
  {"description": "Courtyards overlap",
   "items": [{"description": "Footprint U4", "pos": {"x": 146.5, "y": 133.0}, "uuid": "d"},
             {"description": "Footprint C6", "pos": {"x": 146.5, "y": 136.0}, "uuid": "e"}],
   "severity": "error", "type": "courtyards_overlap"}
]"#,
        )
        .unwrap()
    }

    #[test]
    fn reads_parts_pads_and_fixes() {
        let v = drc();
        assert_eq!(parts(&v[0]), ["J1"]);
        assert_eq!(pads(&v[0]), [("J1".to_string(), "SH".to_string())]);
        assert_eq!(parts(&v[1]), ["R7"]);
        assert_eq!(parts(&v[2]), ["C6", "U4"]);
        assert!(fix(&v[0]).contains("J1 pad SH") && fix(&v[0]).contains("pad_rings"));
        assert!(fix(&v[2]).starts_with("Parts overlap. Move C6, U4 apart"));
        let odd = json!({"type": "something_new", "items": [{"description": "Pad 1 [GND] of U9 on F.Cu"}]});
        assert_eq!(fix(&odd), "Move U9.");
        assert!(fix(&json!({"type": "something_new", "items": []})).starts_with("No fix on file"));
    }

    /// A track's length differs between routes; the fault is still the same one.
    #[test]
    fn signature_ignores_track_length() {
        let a = drc()[1].clone();
        let mut b = a.clone();
        b["items"][0]["description"] = json!("Track [/I2C_SDA] on F.Cu, length 7.0 mm");
        assert_eq!(signature(&a), signature(&b));
        assert_eq!(signature(&a), "clearance: Pad 2 [+3V3] of R7 on F.Cu; Track [/I2C_SDA] on F.Cu");
    }

    #[test]
    fn report_counts_orders_per_fault() {
        let v = drc().as_array().unwrap().clone();
        // the overlap in every order (placement); the thermal only in orders 5 and 6
        let checked = vec![(5, vec![v[0].clone(), v[2].clone()]), (6, vec![v[2].clone(), v[0].clone()]), (4, vec![v[2].clone(), v[1].clone()])];
        let r = report("t", "2026-09-29", pt(100.0, 100.0), &checked, 4, &["checked 3".into()]);
        let items = r["items"].as_array().unwrap();
        assert_eq!(items.len(), 3);
        assert_eq!(items[0]["type"], "courtyards_overlap");
        assert_eq!(items[0]["hits"], "3 of 3");
        assert!(s(&items[0]["pattern"]).starts_with("every checked order"));
        assert_eq!(items[1]["type"], "starved_thermal");
        assert_eq!(items[1]["orders_hit"], json!([5, 6]));
        assert_eq!(items[1]["in_kept_order"], false);
        assert!(s(&items[1]["pattern"]).starts_with("some orders only"));
        assert_eq!(items[1]["items"][0]["at_mm"], json!([20.68, 46.3]));
        assert_eq!(items[2]["type"], "clearance");
        assert_eq!(items[2]["in_kept_order"], true);
        assert!(summary(&r).len() == 1 + 3 * 3);
    }

    #[test]
    fn log_appends_once_and_tallies() {
        let v = drc().as_array().unwrap().clone();
        let dir = std::env::temp_dir().join(format!("pcbgen-failure-test-{}", std::process::id()));
        let (a, b) = (dir.join("a"), dir.join("b"));
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        let checked = vec![(5, vec![v[0].clone()]), (6, vec![]), (4, vec![v[0].clone(), v[1].clone()])];
        let lines = log_lines("a", "2026-09-29", "L1", &checked);
        assert_eq!(lines.len(), 2); // order 6 was clean
        assert_eq!(lines[1]["errors"][1], json!({"type": "clearance", "parts": ["R7"]}));
        let log = a.join("route-failures.jsonl");
        assert_eq!(append_log(&log, &lines).unwrap(), 2);
        assert_eq!(append_log(&log, &log_lines("a", "2026-09-30", "L1", &checked)).unwrap(), 0); // same run again
        assert_eq!(append_log(&log, &log_lines("a", "2026-09-30", "L2", &checked)).unwrap(), 2); // placement changed
        let t = tally(&read_log(&log));
        assert_eq!((t[0].0.as_str(), t[0].1), ("starved_thermal", 4));
        // a second board with the same type: the rule fires
        assert!(!log_summary(&log).iter().any(|l| l.starts_with("RULE")));
        append_log(&b.join("route-failures.jsonl"), &log_lines("b", "2026-09-30", "M", &checked[..1])).unwrap();
        let sum = log_summary(&log);
        assert!(sum.iter().any(|l| l.starts_with("RULE (D-020): starved_thermal has failed on 2 boards (a, b)")), "{sum:?}");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
