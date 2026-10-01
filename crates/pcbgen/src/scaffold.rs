//! `pcb new <name>`: a new board crate in `boards/<name>/`, ready to run through the pipeline.
//!
//! From `templates/board/` (Cargo.toml, src/main.rs, src/circuit.rs, src/layout.rs; `__NAME__`
//! becomes the name): a minimal ESP32-S3 board built from `pcbgen::blocks`, placed on 50 x 50 mm
//! like the starter. From `templates/`: `board.toml` (with the pins the minimal circuit uses
//! added to the template's STATUS_LED), `spec.md` (with the template board.toml's one
//! requirement, so the BOARD.TOML gate passes) and `round.md`.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

/// Names Cargo or this workspace won't take as a board crate's package name.
const RESERVED: &[&str] = &[
    "pcbgen", "devctl", "test", "core", "std", "alloc", "proc-macro", "build", "deps", "examples", "incremental", "as", "async",
    "await", "box", "break", "const", "continue", "crate", "do", "dyn", "else", "enum", "extern", "false", "final", "fn", "for",
    "gen", "if", "impl", "in", "let", "loop", "macro", "match", "mod", "move", "mut", "override", "priv", "pub", "ref", "return",
    "self", "static", "struct", "super", "trait", "true", "try", "type", "typeof", "unsafe", "unsized", "use", "virtual", "where",
    "while", "yield",
];

/// A board name: lower-case letters, digits and single dashes, starting with a letter.
pub fn check_name(name: &str) -> Result<()> {
    let ok = name.len() <= 40
        && name.starts_with(|c: char| c.is_ascii_lowercase())
        && !name.ends_with('-')
        && !name.contains("--")
        && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if !ok {
        bail!("board name {name:?}: use lower-case letters, digits and single dashes, starting with a letter (e.g. \"plant-sensor\")");
    }
    if RESERVED.contains(&name) {
        bail!("board name {name:?} is reserved (a Rust keyword or a name Cargo or this workspace already uses)");
    }
    Ok(())
}

/// The pins the template circuit uses besides the template board.toml's STATUS_LED.
const EXTRA_PINS: &str = r#"
[[pin]]
signal = "BOOT"
pin = "IO0"
net = "BOOT"
gpio = 0
dir = "in"
active_low = true
note = "BOOT button SW2, 10k pull-up (R5); strapping pin: held low at reset = download mode"

[[pin]]
signal = "USB_DM"
pin = "USB_D-"
net = "USB_D-"
gpio = 19
dir = "io"
note = "native USB (flashing and serial console)"

[[pin]]
signal = "USB_DP"
pin = "USB_D+"
net = "USB_D+"
gpio = 20
dir = "io"
note = "native USB (flashing and serial console)"
"#;

/// Replace `from` (which must be in `text`) with `to`.
fn replace_in(text: &str, from: &str, to: &str, file: &str) -> Result<String> {
    if !text.contains(from) {
        bail!("templates/{file} has changed: {from:?} not found; update crates/pcbgen/src/scaffold.rs");
    }
    Ok(text.replace(from, to))
}

/// The files of a new board, as (path inside the board directory, contents).
pub fn files(repo_root: &Path, name: &str) -> Result<Vec<(PathBuf, String)>> {
    let templates = repo_root.join("templates");
    let read = |rel: &str| std::fs::read_to_string(templates.join(rel)).with_context(|| format!("reading templates/{rel}"));
    let mut out = vec![];
    for (from, to) in [
        ("board/Cargo.toml.template", "Cargo.toml"),
        ("board/src/main.rs", "src/main.rs"),
        ("board/src/circuit.rs", "src/circuit.rs"),
        ("board/src/layout.rs", "src/layout.rs"),
    ] {
        out.push((PathBuf::from(to), replace_in(&read(from)?, "__NAME__", name, from)?));
    }

    let toml = read("board.toml")?;
    let toml = replace_in(&toml, "name = \"<name>\"", &format!("name = \"{name}\""), "board.toml")?;
    let toml = toml.replace("<name>", name); // in comments
    let marker = "\n# Where the current comes from";
    let toml = replace_in(&toml, marker, &format!("{EXTRA_PINS}{marker}"), "board.toml")?;
    out.push(("board.toml".into(), toml));

    let spec = read("spec.md")?;
    let spec = replace_in(&spec, "- **R1** ...\n- **R2** ...\n", "- **R1** Shows that it is running with a status light.\n", "spec.md")?;
    let spec = replace_in(&spec, "<Board title>", name, "spec.md")?;
    let spec = replace_in(&spec, "<name>", name, "spec.md")?;
    let spec = replace_in(&spec, "<X>", "0", "spec.md")?;
    out.push(("spec.md".into(), spec));

    out.push(("round.md".into(), read("round.md")?));
    Ok(out)
}

/// Create `boards/<name>/` under `repo_root`. Refuses a bad name or an existing directory.
pub fn new_board(repo_root: &Path, name: &str) -> Result<PathBuf> {
    check_name(name)?;
    let boards = repo_root.join("boards");
    if !boards.is_dir() {
        bail!("{} is not a directory (is {} the pcbgen repository?)", boards.display(), repo_root.display());
    }
    let files = files(repo_root, name)?;
    let dir = boards.join(name);
    match std::fs::create_dir(&dir) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => bail!("{} already exists; pick another name", dir.display()),
        Err(e) => return Err(e).with_context(|| format!("creating {}", dir.display())),
    }
    for (rel, text) in &files {
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, text).with_context(|| format!("writing {}", path.display()))?;
    }
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        for ok in ["demo", "plant-sensor", "clip2", "a-1-b"] {
            assert!(check_name(ok).is_ok(), "{ok}");
        }
        for bad in ["", "Demo", "2clip", "-a", "a-", "a--b", "a_b", "a b", "a/b", "..", "pcbgen", "fn", "test"] {
            assert!(check_name(bad).is_err(), "{bad}");
        }
    }
}
