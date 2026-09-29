//! Verification gates: KiCad ERC and DRC, read from their JSON reports.
//!
//! A gate passes only with zero errors and zero *unwaived* warnings. Waivers live in the
//! board's layout (`Layout::waivers`: violation type, substring, reason), so every
//! accepted warning has a written reason in git.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;
use std::process::Command;

use anyhow::{Result, bail};
use serde_json::Value;

use crate::circuit::Circuit;
use crate::layout::Waiver;
use crate::project::{BoardRules, glob};
use crate::sexpr::{Sexp, parse};

fn cli(args: &[&str]) -> Result<String> {
    let out = Command::new("kicad-cli").args(args).output()?;
    Ok(format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)))
}

fn violations(report: &Value) -> Vec<Value> {
    if let Some(sheets) = report["sheets"].as_array() {
        return sheets.iter().flat_map(|s| s["violations"].as_array().cloned().unwrap_or_default()).collect();
    }
    ["violations", "unconnected_items", "schematic_parity"]
        .iter()
        .flat_map(|k| report[*k].as_array().cloned().unwrap_or_default())
        .collect()
}

fn describe(v: &Value) -> String {
    let items: Vec<&str> =
        v["items"].as_array().into_iter().flatten().map(|i| i["description"].as_str().unwrap_or("")).collect();
    format!("[{}] {}: {} -- {}", s(&v["severity"]), s(&v["type"]), s(&v["description"]), items.join("; "))
}

fn s(v: &Value) -> &str {
    v.as_str().unwrap_or("")
}

fn judge(name: &str, violations: &[Value], waivers: &[Waiver]) -> bool {
    let (mut open, mut waived) = (vec![], vec![]);
    for v in violations {
        if s(&v["severity"]) == "ignore" {
            continue;
        }
        let text = describe(v);
        let w = waivers.iter().find(|w| w.kind == s(&v["type"]) && text.contains(w.substring));
        match w {
            Some(w) if s(&v["severity"]) != "error" => waived.push((text, w.reason)),
            _ => open.push(text),
        }
    }
    let mut counts: BTreeMap<(String, String), usize> = BTreeMap::new();
    for v in violations {
        *counts.entry((s(&v["severity"]).into(), s(&v["type"]).into())).or_default() += 1;
    }
    let counts: Vec<String> = counts.iter().map(|((sev, t), n)| format!("{sev}/{t}: {n}")).collect();
    println!("== {name}: {} violations {}", violations.len(), if counts.is_empty() { String::new() } else { format!("{{{}}}", counts.join(", ")) });
    for (text, why) in &waived {
        println!("   waived: {text}\n           reason: {why}");
    }
    for text in &open {
        println!("   OPEN:   {text}");
    }
    println!("== {name}: {} ({} open, {} waived)", if open.is_empty() { "PASS" } else { "FAIL" }, open.len(), waived.len());
    open.is_empty()
}

pub fn erc(project_dir: &Path, name: &str, waivers: &[Waiver]) -> Result<bool> {
    let reports = project_dir.join("reports");
    std::fs::create_dir_all(&reports)?;
    let out = reports.join("erc.json");
    let _ = std::fs::remove_file(&out);
    let sch = project_dir.join(format!("{name}.kicad_sch"));
    let log = cli(&["sch", "erc", "--format", "json", "--severity-all", "--units", "mm", "-o", &out.to_string_lossy(), &sch.to_string_lossy()])?;
    if !out.exists() {
        bail!("ERC did not produce a report:\n{log}");
    }
    Ok(judge("ERC", &violations(&serde_json::from_str(&std::fs::read_to_string(&out)?)?), waivers))
}

/// DRC with the fab rules and schematic parity; refills zones and saves the board.
pub fn drc(project_dir: &Path, name: &str, waivers: &[Waiver]) -> Result<bool> {
    let reports = project_dir.join("reports");
    std::fs::create_dir_all(&reports)?;
    let out = reports.join("drc.json");
    let _ = std::fs::remove_file(&out);
    let pcb = project_dir.join(format!("{name}.kicad_pcb"));
    let log = cli(&[
        "pcb", "drc", "--format", "json", "--severity-all", "--units", "mm", "--schematic-parity", "--refill-zones",
        "--save-board", "-o", &out.to_string_lossy(), &pcb.to_string_lossy(),
    ])?;
    if !out.exists() {
        bail!("DRC did not produce a report:\n{log}");
    }
    Ok(judge("DRC", &violations(&serde_json::from_str(&std::fs::read_to_string(&out)?)?), waivers))
}

/// Refill every zone with kicad-cli (its DRC refills and saves the board).
pub fn fill_zones(pcb: &Path) -> Result<()> {
    let tmp = pcb.with_extension("fill.json");
    let log = cli(&["pcb", "drc", "--refill-zones", "--save-board", "--format", "json", "-o", &tmp.to_string_lossy(), &pcb.to_string_lossy()])?;
    if !tmp.exists() {
        bail!("zone fill failed:\n{log}");
    }
    std::fs::remove_file(&tmp)?;
    Ok(())
}

pub fn export_netlist(sch: &Path) -> Result<Sexp> {
    let out = sch.with_extension("net");
    let log = cli(&["sch", "export", "netlist", "--format", "kicadsexpr", "-o", &out.to_string_lossy(), &sch.to_string_lossy()])?;
    if !out.exists() {
        bail!("netlist export failed: {log}");
    }
    let tree = parse(&std::fs::read_to_string(&out)?)?;
    std::fs::remove_file(&out)?;
    Ok(tree)
}

/// (net name, pins as (ref, pin number)) for every net in a kicad-cli netlist.
pub fn netlist_nets(tree: &Sexp) -> Vec<(String, Vec<(String, String)>)> {
    tree.find("nets")
        .into_iter()
        .flat_map(|n| n.find_all("net"))
        .map(|n| {
            let pins = n
                .find_all("node")
                .map(|x| (x.get("ref").unwrap_or("").to_string(), x.get("pin").unwrap_or("").to_string()))
                .collect();
            (n.get("name").unwrap_or("").to_string(), pins)
        })
        .collect()
}

/// Round trip: the netlist KiCad reads back from our schematic must group exactly the
/// pins that the circuit connects. Catches a writer bug (a label on the wrong pin, a
/// stray wire) that ERC can't see, since a wrong but tidy schematic passes ERC.
///
/// Nets are compared as pin sets (names differ: KiCad prefixes local labels with "/").
/// Single-pin "unconnected-(...)" nets are KiCad's own names for no-connect pins.
pub fn netlist(tree: &Sexp, c: &Circuit) -> bool {
    let mut kicad: HashMap<BTreeSet<(String, String)>, String> = HashMap::new();
    for (nname, pins) in netlist_nets(tree) {
        if !(nname.starts_with("unconnected-") && pins.len() == 1) {
            kicad.insert(pins.into_iter().collect(), nname);
        }
    }
    // "#..." refs (PWR_FLAG) are ERC markers only; KiCad leaves them out of the netlist
    let mut ours: HashMap<BTreeSet<(String, String)>, String> = HashMap::new();
    for net in c.nets.iter().filter(|n| !n.pins.is_empty()) {
        let set = net
            .pins
            .iter()
            .filter(|p| !c.parts[p.part].reference.starts_with('#'))
            .map(|&p| (c.parts[p.part].reference.clone(), c.pin(p).number.clone()))
            .collect();
        ours.insert(set, net.name.clone());
    }
    let fmt = |m: &HashMap<BTreeSet<(String, String)>, String>, s: &BTreeSet<(String, String)>| format!("{}: {s:?}", m[s]);
    let only_kicad: Vec<String> = kicad.keys().filter(|s| !ours.contains_key(*s)).map(|s| fmt(&kicad, s)).collect();
    let only_ours: Vec<String> = ours.keys().filter(|s| !kicad.contains_key(*s)).map(|s| fmt(&ours, s)).collect();
    println!("== NETLIST: {} nets in the circuit, {} in KiCad's netlist", ours.len(), kicad.len());
    for line in &only_ours {
        println!("   OPEN:   circuit net missing from the schematic: {line}");
    }
    for line in &only_kicad {
        println!("   OPEN:   schematic net not in the circuit: {line}");
    }
    let ok = only_kicad.is_empty() && only_ours.is_empty();
    println!("== NETLIST: {}", if ok { "PASS" } else { "FAIL" });
    ok
}

/// Every net-class pattern must match a net in KiCad's netlist. A pattern that matches
/// nothing leaves its nets on Default silently (seen: "USB_D+" vs KiCad's "/USB_D+"),
/// and every later gate passes on the wrong rules.
pub fn netclasses(tree: &Sexp, rules: &BoardRules) -> bool {
    let nets: Vec<String> = netlist_nets(tree).into_iter().map(|n| n.0).collect();
    let dead: Vec<String> = rules
        .classes
        .iter()
        .flat_map(|nc| nc.patterns.iter().map(move |p| (nc, p)))
        .filter(|(_, p)| !nets.iter().any(|n| glob(p, n)))
        .map(|(nc, p)| format!("{}: {p:?}", nc.name))
        .collect();
    for d in &dead {
        println!("   OPEN:   net-class pattern matches no net: {d}");
    }
    println!("== NETCLASSES: {}", if dead.is_empty() { "PASS" } else { "FAIL" });
    dead.is_empty()
}
