//! `pcb new <name>`: scaffold `boards/<name>/` from `templates/board/` (see `pcbgen::scaffold`).

use anyhow::{Result, bail};

pub fn main(args: &[String]) -> Result<bool> {
    let [name] = args else {
        bail!("usage: pcb new <name>");
    };
    let dir = pcbgen::scaffold::new_board(&pcbgen::repo_root(), name)?;
    println!("created {}\nnext: cargo run --release -p {name}", dir.display());
    Ok(true)
}
