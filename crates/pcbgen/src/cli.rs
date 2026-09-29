//! Pipeline driver, called by each board's `main`:
//! `cargo run --release -p <board> -- [sch] [pcb] [route] [check] [fab] [--out DIR]`
//!
//! Stages run in this order whatever order they are named in (default: all). Generated
//! KiCad files go to `<board>/kicad/` and fab files to `<board>/fab/`; `--out DIR` puts
//! both under DIR instead (for comparison runs).

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Result, bail};

use crate::circuit::Circuit;
use crate::layout::Layout;
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
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--out" => match it.next() {
                Some(d) => base = PathBuf::from(d),
                None => bail!("--out needs a directory"),
            },
            "-h" | "--help" => {
                println!("usage: <board> [{}] [--out DIR]", STAGES.join("] ["));
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
    let layout = (board.layout)();
    let out = base.join("kicad");
    let name = circuit.name.clone();
    let pcb_path = out.join(format!("{name}.kicad_pcb"));

    if want("sch") {
        project::write(&out, &name, &layout.rules)?;
        println!("schematic: {}", schematic::write(&mut circuit, &out)?.display());
        if !(gates::netlist(&out, &name, &circuit)? && gates::netclasses(&out, &name, &layout.rules)?) {
            return Ok(false);
        }
    }
    if want("pcb") {
        println!("pcb: {}", pcb::build(&out, &name, &layout.spec)?.display());
    }
    if want("route") {
        route::route(&pcb_path, &layout.route)?;
    }
    if want("check") {
        let n = gates::netlist(&out, &name, &circuit)? && gates::netclasses(&out, &name, &layout.rules)?;
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
