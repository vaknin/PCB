//! ESP32-S3 starter board, Rev 0. Run: `cargo run --release -p starter -- [stage ...]`.

mod circuit;
mod layout;

fn main() -> std::process::ExitCode {
    pcbgen::cli::main(pcbgen::cli::Board {
        dir: env!("CARGO_MANIFEST_DIR").into(),
        circuit: circuit::build,
        layout: layout::layout,
    })
}
