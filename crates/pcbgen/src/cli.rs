//! Pipeline driver, called by each board's `main`:
//! `cargo run --release -p <board> -- [sch] [pcb] [route] [check] [fab] [--out DIR] [--tries N]`
//!
//! Stages run in this order whatever order they are named in (default: all). Generated
//! KiCad files go to `<board>/kicad/` and fab files to `<board>/fab/`; `--out DIR` puts
//! both under DIR instead (for comparison runs). `--tries N` overrides the layout's
//! number of Freerouting footprint orders (`--tries 1` for a quick look at a placement).

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Result, bail};

use crate::circuit::Circuit;
use crate::layout::Layout;
use crate::sexpr::Sexp;
use crate::{fab, gates, pcb, project, report, route, schematic};

pub const STAGES: [&str; 5] = ["sch", "pcb", "route", "check", "fab"];

pub struct Board {
    /// The board's own directory (its crate).
    pub dir: PathBuf,
    pub circuit: fn() -> Circuit,
    pub layout: fn() -> Layout,
}

pub fn main(board: Board) -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&board, &args) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(board: &Board, args: &[String]) -> Result<bool> {
    let mut stages: Vec<&str> = vec![];
    let mut base = board.dir.clone();
    let mut tries: Option<u32> = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--out" => match it.next() {
                Some(d) => base = PathBuf::from(d),
                None => bail!("--out needs a directory"),
            },
            "--tries" => match it.next().and_then(|n| n.parse().ok()).filter(|&n| n > 0) {
                Some(n) => tries = Some(n),
                None => bail!("--tries needs a number of at least 1"),
            },
            "-h" | "--help" => {
                println!("usage: <board> [{}] [--out DIR] [--tries N]", STAGES.join("] ["));
                return Ok(true);
            }
            s if STAGES.contains(&s) => stages.push(STAGES.iter().find(|x| **x == s).unwrap()),
            s => bail!("unknown stage {s:?}; stages: {}", STAGES.join(", ")),
        }
    }
    if stages.is_empty() {
        stages = STAGES.to_vec();
    }
    let want = |s: &str| stages.contains(&s);

    let mut circuit = (board.circuit)();
    let mut layout = (board.layout)();
    if let Some(n) = tries {
        layout.route.tries = n;
    }
    let out = base.join("kicad");
    let name = circuit.name.clone();
    let pcb_path = out.join(format!("{name}.kicad_pcb"));
    let sch_path = out.join(format!("{name}.kicad_sch"));
    // KiCad's netlist of the schematic: exported once per run, for sch, pcb and check
    let mut net_tree: Option<Sexp> = None;

    if want("sch") {
        project::write(&out, &name, &layout.rules)?;
        println!("schematic: {}", schematic::write(&mut circuit, &out)?.display());
        let tree = net_tree.insert(gates::export_netlist(&sch_path)?);
        let (n, c) = (gates::netlist(tree, &circuit), gates::netclasses(tree, &layout.rules));
        if !(n && c) {
            return Ok(false);
        }
    }
    if want("pcb") {
        let tree = cached(&mut net_tree, &sch_path)?;
        println!("pcb: {}", pcb::build(&out, &name, &layout.spec, tree)?.display());
    }
    if want("route") {
        route::route(&pcb_path, &layout.route)?;
    }
    if want("check") {
        let tree = cached(&mut net_tree, &sch_path)?;
        let n = gates::netlist(tree, &circuit) & gates::netclasses(tree, &layout.rules);
        let e = gates::erc(&out, &name, &layout.waivers)?;
        let d = gates::drc(&out, &name, &layout.waivers)?; // refills zones and saves the board
        let r = report::run(&out, &name, &layout.rules)?;
        if !(n && e && d && r) {
            return Ok(false);
        }
    }
    if want("fab") {
        // the schematic stage adds PWR_FLAG parts; fab only needs the real ones
        fab::export(&out, &name, &circuit, &base.join("fab"))?;
    }
    Ok(true)
}

fn cached<'a>(slot: &'a mut Option<Sexp>, sch: &Path) -> Result<&'a Sexp> {
    if slot.is_none() {
        *slot = Some(gates::export_netlist(sch)?);
    }
    Ok(slot.as_ref().unwrap())
}
