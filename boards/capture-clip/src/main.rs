//! Capture clip, revision A. Run: `cargo run --release -p capture-clip -- [stage ...]`.

mod circuit;
mod layout;

fn main() -> std::process::ExitCode {
    pcbgen::cli::main(pcbgen::cli::Board {
        dir: env!("CARGO_MANIFEST_DIR").into(),
        circuit: circuit::build,
        layout: layout::layout,
    })
}
