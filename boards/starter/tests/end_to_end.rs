//! The whole pipeline on the starter board, into a scratch directory. Needs kicad-cli and
//! Freerouting (scripts/fetch-tools.sh) and takes a few minutes, so it only runs when asked:
//! `cargo test --release -p starter -- --ignored`.

use std::path::Path;
use std::process::Command;

fn json(path: &Path) -> serde_json::Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))).unwrap()
}

#[test]
#[ignore = "needs kicad-cli and Freerouting; takes minutes"]
fn full_run_passes_every_gate() {
    let out = std::env::temp_dir().join(format!("pcbgen-e2e-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&out);
    let run = Command::new(env!("CARGO_BIN_EXE_starter")).arg("--out").arg(&out).output().unwrap();
    let log = String::from_utf8_lossy(&run.stdout);
    assert!(run.status.success(), "pipeline failed:\n{log}\n{}", String::from_utf8_lossy(&run.stderr));

    let reports = out.join("kicad/reports");
    let routing = json(&reports.join("routing.json"));
    assert_eq!(routing["unrouted_connections"], 0);
    assert!(routing["copper_in_keepouts"].as_array().unwrap().is_empty());
    assert!(routing["router"]["kept_order"].is_u64(), "routing.json names the kept router order");
    // only the locked 0.2 mm escape stubs may be under their class width (D-017)
    let thin: f64 = routing["thinner_than_class_mm"].as_object().unwrap().values().filter_map(|v| v.as_f64()).sum();
    assert!(thin < 3.0, "{thin} mm of track under its class width");

    let drc = json(&reports.join("drc.json"));
    for key in ["unconnected_items", "schematic_parity"] {
        assert!(drc[key].as_array().unwrap().is_empty(), "DRC {key}: {}", drc[key]);
    }
    let errors: Vec<_> = drc["violations"].as_array().unwrap().iter().filter(|v| v["severity"] == "error").collect();
    assert!(errors.is_empty(), "DRC errors: {errors:?}");

    for f in ["starter-bom.csv", "starter-cpl.csv", "starter-gerbers.zip", "README.md"] {
        assert!(out.join("fab").join(f).exists(), "fab/{f} missing");
    }
    let _ = std::fs::remove_dir_all(&out);
}
