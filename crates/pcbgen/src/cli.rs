//! Pipeline driver, called by each board's `main`:
//! `cargo run --release -p <board> -- [sch] [pcb] [route] [check] [fab] [fw] [sim] [case] [cost] [review] [--out DIR] [--tries N] [--wokwi]`
//!
//! Stages run in this order whatever order they are named in (default: all). Generated
//! KiCad files go to `<board>/kicad/` and fab files to `<board>/fab/`; `--out DIR` puts
//! both under DIR instead (for comparison runs). `--tries N` overrides the layout's
//! number of Freerouting footprint orders (`--tries 1` for a quick look at a placement).
//!
//! A board with a `board.toml` (D-023) gets the BOARD.TOML gate in `sch`, `check` and `fw`;
//! `fw` writes the firmware's pin header to `<board>/firmware/board_pins.h`, and the Wokwi
//! files (`diagram.json`, `wokwi.toml`, the scenario) when a pin has a `sim` part.
//! `sim` (the firmware in QEMU, plus Wokwi with `--wokwi`; D-025), `case` (the printed case
//! and its fit check, `<board>/case/`; D-025 Phase B), `cost` (live JLCPCB prices, needs the
//! network) and `review` (the owner's review page, `<board>/review/index.html`, and the readiness page
//! `readiness.html` beside it) run only when
//! named.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Result, bail};

use crate::circuit::Circuit;
use crate::layout::Layout;
use crate::sexpr::Sexp;
use crate::{boardfile, case, cost, fab, gates, pcb, project, report, review, route, schematic, sim, wokwi};

pub const STAGES: [&str; 10] = ["sch", "pcb", "route", "check", "fab", "fw", "sim", "case", "cost", "review"];
/// What runs when no stage is named: everything that builds and checks the design.
pub const DEFAULT: usize = 6;

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
    let mut wokwi = false;
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
            "--wokwi" => wokwi = true,
            "-h" | "--help" => {
                println!("usage: <board> [{}] [--out DIR] [--tries N] [--wokwi]", STAGES.join("] ["));
                return Ok(true);
            }
            s if STAGES.contains(&s) => stages.push(STAGES.iter().find(|x| **x == s).unwrap()),
            s => bail!("unknown stage {s:?}; stages: {}", STAGES.join(", ")),
        }
    }
    if stages.is_empty() {
        stages = STAGES[..DEFAULT].to_vec();
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
    let board_file = boardfile::load(&board.dir)?;
    // the BOARD.TOML gate's result: checked once per run, by the first stage that needs it
    let mut toml_ok: Option<bool> = None;
    let mut board_toml = |c: &Circuit| -> Result<bool> {
        if let Some(ok) = toml_ok {
            return Ok(ok);
        }
        let ok = match &board_file {
            Some(bf) => boardfile::gate(bf, c, &board.dir)?,
            None => {
                println!("== BOARD.TOML: none in {} (skipped; see templates/board.toml)", board.dir.display());
                true
            }
        };
        Ok(*toml_ok.insert(ok))
    };

    if want("sch") {
        project::write(&out, &name, &layout.rules)?;
        println!("schematic: {}", schematic::write(&mut circuit, &out)?.display());
        let tree = net_tree.insert(gates::export_netlist(&sch_path)?);
        let (n, c) = (gates::netlist(tree, &circuit), gates::netclasses(tree, &layout.rules));
        let b = board_toml(&circuit)?;
        if !(n && c && b) {
            return Ok(false);
        }
    }
    if want("pcb") {
        let tree = cached(&mut net_tree, &sch_path)?;
        println!("pcb: {}", pcb::build(&out, &name, &layout.spec, tree)?.display());
    }
    if want("route") {
        route::route(&pcb_path, &layout.route, &layout.waivers)?;
    }
    if want("check") {
        let tree = cached(&mut net_tree, &sch_path)?;
        let (nl, nc) = (gates::netlist(tree, &circuit), gates::netclasses(tree, &layout.rules));
        let b = board_toml(&circuit)?;
        let e = gates::erc(&out, &name, &layout.waivers)?;
        let d = gates::drc(&out, &name, &layout.waivers)?; // refills zones and saves the board
        let r = report::run(&out, &name, &layout.rules)?;
        // for the review page: which gates passed, and when (after DRC saved the board)
        let summary = serde_json::json!({
            "date": crate::schematic::today(),
            "netlist": nl, "netclasses": nc, "board_toml": b, "erc": e, "drc": d, "routing": r,
        });
        std::fs::write(out.join("reports/gates.json"), serde_json::to_string_pretty(&summary)? + "\n")?;
        if !(nl && nc && b && e && d && r) {
            return Ok(false);
        }
    }
    if want("fab") {
        // the schematic stage adds PWR_FLAG parts; fab only needs the real ones
        fab::export(&out, &name, &circuit, &base.join("fab"))?;
    }
    if want("fw") {
        match &board_file {
            None => println!("fw: no board.toml in {}; no pin header written", board.dir.display()),
            Some(bf) => {
                if !board_toml(&circuit)? {
                    return Ok(false);
                }
                let dir = base.join("firmware");
                std::fs::create_dir_all(&dir)?;
                if !dir.join("CMakeLists.txt").exists() {
                    copy_template(&crate::repo_root().join("templates/firmware"), &dir)?;
                    println!("fw: new firmware project from templates/firmware in {}", dir.display());
                }
                let path = dir.join("board_pins.h");
                std::fs::write(&path, boardfile::header(bf))?;
                println!("fw: {}", path.display());
                if wokwi::wanted(bf) {
                    std::fs::write(dir.join(wokwi::DIAGRAM), wokwi::diagram(bf))?;
                    std::fs::write(dir.join(wokwi::TOML), wokwi::toml())?;
                    std::fs::write(dir.join(wokwi::SCENARIO), wokwi::scenario(bf))?;
                    println!("fw: {} (+ {}, {})", dir.join(wokwi::DIAGRAM).display(), wokwi::TOML, wokwi::SCENARIO);
                }
            }
        }
    }
    if want("sim") {
        let Some(bf) = &board_file else { bail!("sim needs a board.toml") };
        // builds the board's own firmware project; results go next to the --out header
        let ok = sim::run(bf, &board.dir.join("firmware"), &base.join("firmware"), &sim::Options { wokwi })?;
        if !ok {
            return Ok(false);
        }
    }
    if want("case") {
        let Some(bf) = &board_file else { bail!("case needs a board.toml with a [case] table") };
        // [case] is part of the BOARD.TOML gate: check it before building anything
        if !board_toml(&circuit)? || !case::run(bf, &pcb_path, &base)? {
            return Ok(false);
        }
    }
    if want("cost") {
        let (boards, assembled) = board_file.as_ref().map_or((5, 2), |b| (b.order.boards, b.order.assembled));
        cost::run(&out, &name, &base.join("fab"), boards, assembled)?;
    }
    if want("review") {
        let inputs = review::Inputs { dir: &board.dir, base: &base, circuit: &circuit, board_file: board_file.as_ref(), waivers: &layout.waivers };
        println!("review: {}", review::write(&inputs)?.display());
    }
    Ok(true)
}

/// Copies the firmware template (files and directories) into a new firmware project.
fn copy_template(from: &Path, to: &Path) -> Result<()> {
    for e in std::fs::read_dir(from)? {
        let e = e?;
        let dest = to.join(e.file_name());
        if e.file_type()?.is_dir() {
            std::fs::create_dir_all(&dest)?;
            copy_template(&e.path(), &dest)?;
        } else {
            std::fs::copy(e.path(), &dest)?;
        }
    }
    Ok(())
}

fn cached<'a>(slot: &'a mut Option<Sexp>, sch: &Path) -> Result<&'a Sexp> {
    if slot.is_none() {
        *slot = Some(gates::export_netlist(sch)?);
    }
    Ok(slot.as_ref().unwrap())
}
