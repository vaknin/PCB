//! `pcb lib import <LCSC> [--fresh]`: a symbol and footprint KiCad doesn't ship, from the
//! part's EasyEDA entry into the `pcbgen` libraries, recorded UNVERIFIED in lib/IMPORTED.toml
//! (`pcbgen::libimport`).

use anyhow::{Result, bail};
use pcbgen::jlc::Jlc;
use pcbgen::libimport;

const HELP: &str = "usage:
  pcb lib import <LCSC> [--fresh]
      symbol + footprint via easyeda2kicad into lib/symbols/pcbgen.kicad_sym and
      lib/footprints/pcbgen.pretty, named after the MPN; recorded UNVERIFIED in lib/IMPORTED.toml
      --fresh  skip the 1-day cache for the JLCPCB listing (MPN)";

pub fn main(args: &[String]) -> Result<bool> {
    match args.first().map(String::as_str) {
        Some("import") => import(&args[1..]),
        Some("-h" | "--help") => {
            println!("{HELP}");
            Ok(true)
        }
        Some(s) => bail!("unknown lib command {s:?}\n{HELP}"),
        None => bail!("{HELP}"),
    }
}

fn import(args: &[String]) -> Result<bool> {
    let mut jlc = Jlc::default();
    let mut ids = vec![];
    for a in args {
        match a.as_str() {
            "--fresh" => jlc.fresh = true,
            "-h" | "--help" => {
                println!("{HELP}");
                return Ok(true);
            }
            s if s.starts_with("--") => bail!("unknown option {s}\n{HELP}"),
            s => ids.push(s),
        }
    }
    let [lcsc] = ids[..] else { bail!("give one LCSC number\n{HELP}") };
    let r = libimport::import(&pcbgen::repo_root(), lcsc, &jlc)?;
    let e = &r.entry;
    println!("imported {} ({}) with {}:", e.lcsc, e.mpn, e.tool);
    println!("  symbol    {}  (EasyEDA's {:?}, {} pins)", e.symbol, r.symbol.easyeda_name, r.symbol.pins);
    println!("  footprint {}  (EasyEDA's {})", e.footprint, e.easyeda_footprint);
    println!("  recorded in lib/IMPORTED.toml as UNVERIFIED");
    if r.symbol.unspecified_to_passive > 0 {
        println!("note: {} pins had type \"unspecified\" and are now \"passive\"; set real types (power_in, output, ...) when checking the datasheet", r.symbol.unspecified_to_passive);
    }
    if !r.pads_outside_courtyard.is_empty() {
        println!(
            "WARNING: the courtyard doesn't cover {} ({}): widen the F.CrtYd outline to pads + 0.25 mm before placing it",
            r.pads_outside_courtyard.len(),
            r.pads_outside_courtyard.join(", ")
        );
    }
    println!("3D model: add this line to EASYEDA in enclosure/models.py, then run it:");
    println!("    {}", r.models_line);
    println!("  easyeda2kicad's STEP is not in the footprint's frame: set rot_z/move by hand against the pads and");
    println!("  F.Fab (HARDWARE_LESSONS \"3D models\"); until the model exists the case stage fails on it.");
    println!("next: a datasheet check of pins and pads (review-agents.md), then status = \"VERIFIED\" and checked_by.");
    Ok(true)
}
