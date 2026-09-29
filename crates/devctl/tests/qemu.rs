//! devctl over the QEMU link against the starter's firmware. Skipped (with a note) when QEMU
//! or the starter's QEMU image (`cargo run --release -p starter -- sim`) is missing.

use std::path::PathBuf;

use devctl::{QemuLink, Session, provision, selftest};
use pcbgen::boardfile::BoardFile;
use serde_json::json;

fn starter() -> Option<(BoardFile, PathBuf)> {
    let dir = pcbgen::repo_root().join("boards/starter");
    let image = dir.join("firmware/build-qemu/sim_flash.bin");
    if let Err(e) = pcbgen::sim::qemu_binary() {
        eprintln!("skipped: {e:#}");
        return None;
    }
    if !image.exists() {
        eprintln!("skipped: no {} (cargo run --release -p starter -- sim)", image.display());
        return None;
    }
    Some((devctl::load_board(&dir).unwrap(), dir))
}

#[test]
fn selftest_in_qemu() {
    let Some((bf, dir)) = starter() else { return };
    let mut s = Session::new(QemuLink::open(&dir).unwrap());
    let r = selftest::run(&bf, &mut s, "qemu").unwrap();
    assert_eq!(r["problems"], json!([]), "{r:#}");
    assert_eq!(r["ok"], true);
    assert_eq!(r["banner"]["name"], "starter");
    let tests = r["tests"].as_array().unwrap();
    assert_eq!(tests.len(), 4, "{r:#}");
    assert!(tests.iter().all(|t| t["result"] == "skip"), "{r:#}");
    assert_eq!(r["summary"]["skip"], 4);
    assert_eq!(r["needs_person"], json!([]));
    // the wrong target is caught
    let mut s = Session::new(QemuLink::open(&dir).unwrap());
    let r = selftest::run(&bf, &mut s, "real").unwrap();
    assert_eq!(r["problems"], json!(["banner target is \"qemu\", expected \"real\""]));
}

#[test]
fn provision_in_qemu() {
    let Some((mut bf, dir)) = starter() else { return };
    let file = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("devctl-provision-{}.conf", std::process::id()));
    let (ssid, token) = ("Q7 home=net", "tok-Z9x4-devctl-secret");
    std::fs::write(&file, format!("# test\nwifi_ssid=old\nwifi_ssid=  {ssid}  \napi_token={token}\n")).unwrap();
    for key in ["wifi_ssid", "api_token"] {
        bf.provision.insert(key.into(), format!("file:{}#{key}", file.display()));
    }
    let mut s = Session::new(QemuLink::open(&dir).unwrap());
    let sent = provision::run(&bf, &mut s, "qemu");
    std::fs::remove_file(&file).unwrap();
    let sent = sent.unwrap();
    assert_eq!(sent, [("api_token".to_string(), token.len()), ("wifi_ssid".to_string(), ssid.len())]);

    let logs = [s.log.join("\n"), s.link.console_log().join("\n")];
    assert!(logs[0].contains(&format!(">> PROV SET wifi_ssid ({} bytes hidden)", ssid.len())), "{}", logs[0]);
    assert!(logs[1].contains(&format!("PROV KEY api_token {}", token.len())), "{}", logs[1]);
    for secret in [ssid, token, "Q7 home", "Z9x4"] {
        let hex = provision::hex(secret.as_bytes());
        for log in &logs {
            assert!(!log.contains(secret), "a value leaked into a log");
            assert!(!log.contains(hex.as_str()), "a value's hex leaked into a log");
        }
    }
}
