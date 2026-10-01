//! `pcb`: repository commands. `cargo run --release -p pcbgen --bin pcb -- new <name>`.

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>()[..] {
        ["new", name] => match pcbgen::scaffold::new_board(&pcbgen::repo_root(), name) {
            Ok(dir) => {
                println!("created {}\nnext: cargo run --release -p {name}", dir.display());
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("error: {e:#}");
                ExitCode::FAILURE
            }
        },
        _ => {
            eprintln!("usage: pcb new <name>");
            ExitCode::from(2)
        }
    }
}
