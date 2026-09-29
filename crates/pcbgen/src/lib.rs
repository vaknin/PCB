//! pcbgen: circuit code → KiCad 10 schematic → ERC → code-placed PCB → Freerouting → DRC →
//! JLCPCB fab files. KiCad files are written directly as S-expressions; kicad-cli does
//! netlist export, ERC, DRC, zone fill and fab exports. No KiCad library is loaded.

pub mod board;
pub mod boardfile;
pub mod circuit;
pub mod cli;
pub mod cost;
pub mod dsn;
pub mod fab;
pub mod failure;
pub mod footprint;
pub mod gates;
pub mod geom;
pub mod layout;
pub mod pcb;
pub mod project;
pub mod report;
pub mod review;
pub mod route;
pub mod schematic;
pub mod ses;
pub mod sexpr;
pub mod sim;
pub mod stitch;
pub mod symlib;

use std::path::PathBuf;
use std::process::Command;

use anyhow::{Result, bail};

/// The pcbgen repository (fab rules, local libraries, Freerouting live here).
pub fn repo_root() -> PathBuf {
    let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let root = here.join("../..");
    root.canonicalize().unwrap_or(root)
}

/// Deterministic UUID for a generated item: uuid5(URL namespace, "pcbgen:<board>:<a/b/c>").
pub fn uid(board: &str, key: &[&str]) -> String {
    let name = format!("pcbgen:{board}:{}", key.join("/"));
    uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_URL, name.as_bytes()).to_string()
}

/// Run kicad-cli; fail with its output on a non-zero exit.
pub fn kicad_cli(args: &[&str]) -> Result<String> {
    let out = Command::new("kicad-cli").args(args).output()?;
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    if !out.status.success() {
        bail!("kicad-cli {} failed:\n{text}", args.iter().take(3).cloned().collect::<Vec<_>>().join(" "));
    }
    Ok(text)
}
