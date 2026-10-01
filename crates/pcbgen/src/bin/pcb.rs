//! `pcb`: project tools that are not a board's pipeline stage.
//! `cargo run --release -p pcbgen --bin pcb -- <command> ...`
//!
//! Each command lives in its own module under `src/bin/pcb/` and adds one line to
//! `COMMANDS`, so commands added in parallel merge without conflicts.

#[path = "pcb/lib.rs"]
mod lib;
#[path = "pcb/new.rs"]
mod new;
#[path = "pcb/parts.rs"]
mod parts;

use std::process::ExitCode;

use anyhow::Result;

/// (name, one-line help, entry). An entry gets the arguments after its name and returns
/// Ok(false) for a clean failure (a check that did not pass).
type Entry = fn(&[String]) -> Result<bool>;
const COMMANDS: &[(&str, &str, Entry)] = &[
    ("lib", "import a part KiCad lacks from EasyEDA (see `pcb lib --help`)", lib::main),
    ("new", "scaffold boards/<name>/ from templates/board/", new::main),
    ("parts", "search JLCPCB's parts and show one part (see `pcb parts --help`)", parts::main),
];

fn usage() {
    println!("usage: pcb <command> [args]\n\ncommands:");
    for (name, help, _) in COMMANDS {
        println!("  {name:<8} {help}");
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(cmd) = args.first() else {
        usage();
        return ExitCode::FAILURE;
    };
    if cmd == "-h" || cmd == "--help" || cmd == "help" {
        usage();
        return ExitCode::SUCCESS;
    }
    let Some((_, _, entry)) = COMMANDS.iter().find(|c| c.0 == cmd) else {
        eprintln!("error: unknown command {cmd:?}");
        usage();
        return ExitCode::FAILURE;
    };
    match entry(&args[1..]) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}
