//! `devctl flash`: the real-board build onto the attached board. Refuses a simulation build and
//! any chip that isn't an ESP32-S3 with 16 MB flash; writes exactly the build it checked.

use std::path::Path;

use anyhow::{Context, Result, bail};
use pcbgen::boardfile::BoardFile;
use pcbgen::sim::{FLASH, idf_path, idf_shell};
use regex::Regex;
use serde_json::Value;

use crate::{SerialLink, Session};

/// Builds `firmware/build`, checks it and the chip, writes it, then checks the boot banner.
pub fn run(bf: &BoardFile, dir: &Path, port: Option<&str>) -> Result<bool> {
    let fw = dir.join("firmware");
    if !fw.join("CMakeLists.txt").exists() {
        bail!("no firmware project in {} (the fw stage makes it)", fw.display());
    }
    let idf = idf_path()?;
    let build = fw.join("build");
    std::fs::create_dir_all(&build)?;
    println!("flash: building the real-board firmware in {}", build.display());
    idf_shell(&idf, &fw, "idf.py -B build build", &build.join("devctl-build.log")).context("building the real-board target")?;
    let config = build.join("config/sdkconfig.json");
    check_config(&std::fs::read_to_string(&config).with_context(|| format!("reading {}", config.display()))?)?;

    let port = match port {
        Some(p) => p.to_string(),
        None => crate::link::find_port()?,
    };
    if port.contains('\'') {
        bail!("port {port:?} has a quote in its name");
    }
    let id_log = build.join("devctl-flash-id.log");
    idf_shell(&idf, &build, &format!("esptool --chip esp32s3 -p '{port}' flash-id"), &id_log).with_context(|| format!("asking the chip on {port} what it is"))?;
    check_chip(&std::fs::read_to_string(&id_log)?).with_context(|| format!("refusing to flash the chip on {port} (esptool's answer: {})", id_log.display()))?;

    // from the build directory just checked, so nothing rebuilds in between
    println!("flash: writing {} to {port}", build.display());
    let script = format!("esptool --chip esp32s3 -p '{port}' -b 460800 --before default-reset --after hard-reset write-flash @flash_args");
    idf_shell(&idf, &build, &script, &build.join("devctl-flash.log")).context("writing the flash")?;

    let mut s = Session::new(SerialLink::open(Some(&port))?);
    let (banner, problems) = crate::boot(bf, &mut s, "real")?;
    println!("== FLASH: {}", if problems.is_empty() { "PASS" } else { "FAIL" });
    println!("   banner: {banner}");
    for p in &problems {
        println!("   problem: {p}");
    }
    Ok(problems.is_empty())
}

/// The build's `config/sdkconfig.json` must be the real-board target for an ESP32-S3 with the
/// module's flash size.
pub fn check_config(json: &str) -> Result<()> {
    let c: Value = serde_json::from_str(json).context("sdkconfig.json is not JSON")?;
    if c["BOARD_TARGET_REAL"] != true || c["BOARD_TARGET_QEMU"] != false || c["BOARD_TARGET_WOKWI"] != false {
        bail!("refusing to flash: firmware/build is not the real-board target (BOARD_TARGET_NAME is {})", c["BOARD_TARGET_NAME"]);
    }
    if c["IDF_TARGET"] != "esp32s3" || c["ESPTOOLPY_FLASHSIZE"] != FLASH {
        bail!("refusing to flash: firmware/build is for {} with {} flash, not esp32s3 with {FLASH}", c["IDF_TARGET"], c["ESPTOOLPY_FLASHSIZE"]);
    }
    Ok(())
}

/// esptool's `flash-id` output must name an ESP32-S3 and 16 MB of flash. The wording is
/// UNVERIFIED (no board yet): esptool v5 is expected to print `Chip type: ESP32-S3 ...` (older:
/// `Chip is ESP32-S3 ...`) and `Detected flash size: 16MB`, so this reads it leniently.
pub fn check_chip(text: &str) -> Result<()> {
    let chip = Regex::new(r"(?im)^.*\bchip\b.*\bESP32-?S3\b").unwrap();
    let size = Regex::new(r"(?i)detected flash size:\s*(\S+)").unwrap();
    if !chip.is_match(text) {
        let said = text.lines().find(|l| l.to_lowercase().contains("chip")).unwrap_or("no chip line");
        bail!("not an ESP32-S3 ({})", said.trim());
    }
    match size.captures(text).map(|c| c[1].to_string()) {
        Some(s) if s.eq_ignore_ascii_case(FLASH) => Ok(()),
        Some(s) => bail!("the chip has {s} of flash, the module needs {FLASH}"),
        None => bail!("esptool didn't report the flash size"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config() {
        let real = r#"{"BOARD_TARGET_NAME":"real","BOARD_TARGET_REAL":true,"BOARD_TARGET_QEMU":false,"BOARD_TARGET_WOKWI":false,"IDF_TARGET":"esp32s3","ESPTOOLPY_FLASHSIZE":"16MB"}"#;
        assert!(check_config(real).is_ok());
        assert!(check_config(&real.replace(r#""BOARD_TARGET_REAL":true,"BOARD_TARGET_QEMU":false"#, r#""BOARD_TARGET_REAL":false,"BOARD_TARGET_QEMU":true"#)).is_err());
        assert!(check_config(&real.replace(r#""BOARD_TARGET_WOKWI":false"#, r#""BOARD_TARGET_WOKWI":true"#)).is_err());
        assert!(check_config(&real.replace("16MB", "4MB")).is_err());
        assert!(check_config(r#"{"IDF_TARGET":"esp32s3"}"#).is_err());
        // the real build this repo has, if built
        let built = pcbgen::repo_root().join("boards/starter/firmware/build/config/sdkconfig.json");
        if let Ok(t) = std::fs::read_to_string(built) {
            assert!(check_config(&t).is_ok());
        }
        let qemu = pcbgen::repo_root().join("boards/starter/firmware/build-qemu/config/sdkconfig.json");
        if let Ok(t) = std::fs::read_to_string(qemu) {
            assert!(check_config(&t).is_err());
        }
    }

    #[test]
    fn chip() {
        // UNVERIFIED sample of esptool v5 output
        let v5 = "Connected to ESP32-S3 on /dev/ttyACM0:\nChip type:          ESP32-S3 (QFN56) (revision v0.2)\nFeatures:           Wi-Fi, BT 5 (LE)\n\nFlash Memory Information:\n=========================\nManufacturer: c8\nDevice: 4018\nDetected flash size: 16MB\n";
        assert!(check_chip(v5).is_ok());
        assert!(check_chip("Chip is ESP32-S3 (revision v0.1)\nDetected flash size: 16MB\n").is_ok());
        assert!(format!("{:#}", check_chip(&v5.replace("16MB", "8MB")).unwrap_err()).contains("8MB"));
        assert!(check_chip(&v5.replace("ESP32-S3", "ESP32-C3")).is_err());
        assert!(check_chip("Chip type: ESP32-S3\n").is_err());
    }
}
