//! The `sim` stage (D-025): the board's firmware in simulation, open source first.
//!
//! - **QEMU** (Espressif's fork, `idf_tools.py install qemu-xtensa`, unlimited): builds the
//!   firmware's QEMU target (`sdkconfig.defaults` + `sdkconfig.qemu`, in `firmware/build-qemu`),
//!   boots it with the module's 16 MB flash and 8 MB octal PSRAM, and checks the boot banner, the
//!   self-tests `board.toml` lists and a provisioning round trip. QEMU has no GPIO, I2C, I2S, USB,
//!   Wi-Fi or deep sleep, so tests that need them report "skip" there.
//! - **Wokwi** (`sim --wokwi`, free plan 50 simulated minutes a month): the pin checks QEMU
//!   can't do. Not built yet.
//!
//! Results go to `firmware/sim.json` for the review page; the serial log to
//! `firmware/build-qemu/sim.log`. ESP-IDF is found at `$IDF_PATH` or `~/esp/esp-idf-v6.1`.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use crate::boardfile::BoardFile;

/// ESP32-S3-WROOM-1-N16R8 (the only module pcbgen has a GPIO table for).
const FLASH: &str = "16MB";
const PSRAM_BYTES: u64 = 8 << 20;
/// How long the whole QEMU run may take (wall clock); a boot takes a few seconds.
const QEMU_TIMEOUT: Duration = Duration::from_secs(90);

/// The otadata entry "ota_0, state VALID" (sequence 1, CRC as the bootloader writes it), copied
/// from an image the firmware itself had marked. A blank otadata makes the app write this entry
/// early in boot, and that write crashes QEMU 9.2.2 with octal PSRAM (SIGSEGV in
/// `psram_quad_read`, verified 2026-09-29), so the image carries it from the start.
const OTADATA_VALID_OTA0: [u8; 32] = [
    0x01, 0x00, 0x00, 0x00, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, //
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x02, 0x00, 0x00, 0x00, 0x9a, 0x98, 0x43, 0x47,
];

pub struct Options {
    pub wokwi: bool,
}

/// Runs the simulations; `fw` is the board's firmware project, `out` where `sim.json` goes.
pub fn run(bf: &BoardFile, fw: &Path, out: &Path, opts: &Options) -> Result<bool> {
    if !fw.join("CMakeLists.txt").exists() {
        bail!("no firmware project in {} (the fw stage copies templates/firmware there)", fw.display());
    }
    let idf = idf_path()?;
    let started = Instant::now();
    let qemu = match qemu_run(bf, fw, &idf) {
        Ok(v) => v,
        Err(e) => json!({"ok": false, "problems": [format!("{e:#}")]}),
    };
    let qemu_ok = qemu["ok"] == true;
    print_summary("QEMU", &qemu);

    let old: Value = std::fs::read_to_string(out.join("sim.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(Value::Null);
    let wokwi = if opts.wokwi {
        bail!("sim --wokwi is not built yet (D-025 Phase A)");
    } else {
        old["wokwi"].clone() // keep the last Wokwi result; it costs quota to redo
    };
    let result = json!({
        "date": crate::schematic::today(),
        "seconds": started.elapsed().as_secs(),
        "qemu": qemu,
        "wokwi": wokwi,
    });
    std::fs::create_dir_all(out)?;
    std::fs::write(out.join("sim.json"), serde_json::to_string_pretty(&result)? + "\n")?;
    println!("sim: {}", out.join("sim.json").display());
    Ok(qemu_ok)
}

fn print_summary(what: &str, r: &Value) {
    let ok = r["ok"] == true;
    println!("== SIM ({what}): {}", if ok { "PASS" } else { "FAIL" });
    for t in r["tests"].as_array().into_iter().flatten() {
        println!("   {:<14} {:<7} {}", t["test"].as_str().unwrap_or("?"), t["result"].as_str().unwrap_or("?"), t["detail"].as_str().unwrap_or(""));
    }
    for p in r["problems"].as_array().into_iter().flatten() {
        println!("   problem: {}", p.as_str().unwrap_or("?"));
    }
}

fn idf_path() -> Result<PathBuf> {
    let p = match std::env::var_os("IDF_PATH") {
        Some(p) => PathBuf::from(p),
        None => PathBuf::from(std::env::var("HOME")?).join("esp/esp-idf-v6.1"),
    };
    if !p.join("export.sh").exists() {
        bail!("ESP-IDF not found at {} (set IDF_PATH)", p.display());
    }
    Ok(p)
}

/// Runs `script` in a bash with ESP-IDF's environment, in `dir`, with its output in `log`.
fn idf_shell(idf: &Path, dir: &Path, script: &str, log: &Path) -> Result<()> {
    let full = format!(". '{}/export.sh' >/dev/null 2>&1 && {script}", idf.display());
    let out = Command::new("bash").arg("-c").arg(&full).current_dir(dir).env("PCB_ROOT", crate::repo_root()).output()?;
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    std::fs::write(log, &text)?;
    if !out.status.success() {
        let tail: Vec<&str> = text.lines().rev().take(25).collect::<Vec<_>>().into_iter().rev().collect();
        bail!("`{script}` failed (log: {}):\n{}", log.display(), tail.join("\n"));
    }
    Ok(())
}

/// Builds one simulation target in `firmware/build-<target>`. Its sdkconfig is regenerated
/// when a defaults file changed, so an edited `sdkconfig.qemu` can't be silently ignored.
fn build(fw: &Path, idf: &Path, target: &str) -> Result<PathBuf> {
    let dir = fw.join(format!("build-{target}"));
    std::fs::create_dir_all(&dir)?;
    let sdkconfig = dir.join("sdkconfig");
    let defaults = ["sdkconfig.defaults".to_string(), format!("sdkconfig.{target}")];
    if let Ok(made) = std::fs::metadata(&sdkconfig).and_then(|m| m.modified()) {
        let newer = defaults.iter().any(|d| std::fs::metadata(fw.join(d)).and_then(|m| m.modified()).is_ok_and(|t| t > made));
        if newer {
            std::fs::remove_file(&sdkconfig)?;
        }
    }
    let script = format!(
        "idf.py -B build-{target} -D SDKCONFIG=build-{target}/sdkconfig -D SDKCONFIG_DEFAULTS='{}' build",
        defaults.join(";")
    );
    idf_shell(idf, fw, &script, &dir.join("build.log")).with_context(|| format!("building the {target} target"))?;
    Ok(dir)
}

/// The flash image QEMU boots: every part of `flash_args` merged into one 16 MB file, plus the
/// otadata entry (see `OTADATA_VALID_OTA0`).
fn flash_image(build: &Path, idf: &Path) -> Result<PathBuf> {
    let image = build.join("sim_flash.bin");
    let script = format!("esptool --chip esp32s3 merge-bin --output sim_flash.bin --pad-to-size {FLASH} @flash_args");
    idf_shell(idf, build, &script, &build.join("merge.log"))?;
    let args = std::fs::read_to_string(build.join("flash_args"))?;
    let otadata = args
        .lines()
        .find(|l| l.ends_with("ota_data_initial.bin"))
        .and_then(|l| l.split_whitespace().next())
        .and_then(|o| u64::from_str_radix(o.trim_start_matches("0x"), 16).ok())
        .context("no ota_data_initial.bin in flash_args: the partition table needs an otadata partition")?;
    let mut bytes = std::fs::read(&image)?;
    let at = otadata as usize;
    bytes[at..at + OTADATA_VALID_OTA0.len()].copy_from_slice(&OTADATA_VALID_OTA0);
    std::fs::write(&image, bytes)?;
    Ok(image)
}

fn qemu_binary() -> Result<PathBuf> {
    let tools = match std::env::var_os("IDF_TOOLS_PATH") {
        Some(p) => PathBuf::from(p),
        None => PathBuf::from(std::env::var("HOME")?).join(".espressif"),
    };
    let dir = tools.join("tools/qemu-xtensa");
    let mut found: Vec<PathBuf> = std::fs::read_dir(&dir)
        .with_context(|| format!("QEMU not installed ({}): idf_tools.py install qemu-xtensa", dir.display()))?
        .filter_map(|e| e.ok())
        .map(|e| e.path().join("qemu/bin/qemu-system-xtensa"))
        .filter(|p| p.exists())
        .collect();
    found.sort();
    found.pop().context("no qemu-system-xtensa under ~/.espressif/tools/qemu-xtensa")
}

/// A running QEMU: its console lines arrive on a channel, so every wait has a deadline.
struct Console {
    child: Child,
    stdin: ChildStdin,
    lines: Receiver<String>,
    log: Vec<String>,
    deadline: Instant,
}

impl Console {
    fn start(qemu: &Path, image: &Path) -> Result<Console> {
        let mut child = Command::new(qemu)
            .args(["-M", "esp32s3", "-m", &format!("{}M", PSRAM_BYTES >> 20)])
            .arg("-drive")
            .arg(format!("file={},if=mtd,format=raw", image.display()))
            .args(["-global", "driver=ssi_psram,property=is_octal,value=true", "-nographic", "-serial", "mon:stdio"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("starting QEMU")?;
        let stdin = child.stdin.take().unwrap();
        let (tx, rx) = channel();
        for pipe in [Box::new(child.stdout.take().unwrap()) as Box<dyn std::io::Read + Send>, Box::new(child.stderr.take().unwrap())] {
            let tx = tx.clone();
            std::thread::spawn(move || {
                for line in BufReader::new(pipe).lines().map_while(Result::ok) {
                    if tx.send(line.trim_end_matches('\r').to_string()).is_err() {
                        break;
                    }
                }
            });
        }
        Ok(Console { child, stdin, lines: rx, log: vec![], deadline: Instant::now() + QEMU_TIMEOUT })
    }

    /// Waits for a line starting with `prefix`; returns the rest of it.
    fn expect(&mut self, prefix: &str) -> Result<String> {
        loop {
            let left = self.deadline.saturating_duration_since(Instant::now());
            match self.lines.recv_timeout(left) {
                Ok(l) => {
                    self.log.push(l.clone());
                    if let Some(rest) = l.strip_prefix(prefix) {
                        return Ok(rest.trim().to_string());
                    }
                    let boots = self.log.iter().filter(|x| x.starts_with("ESP-ROM:")).count();
                    if l.contains("Guru Meditation") || l.starts_with("abort()") || boots > 1 {
                        bail!("the firmware crashed or reset while waiting for `{prefix}`: {l}");
                    }
                }
                Err(RecvTimeoutError::Timeout) => bail!("timed out waiting for `{prefix}`"),
                Err(RecvTimeoutError::Disconnected) => bail!("QEMU exited while waiting for `{prefix}` (see the log)"),
            }
        }
    }

    fn send(&mut self, line: &str) -> Result<()> {
        self.log.push(format!(">> {line}"));
        writeln!(self.stdin, "{line}")?;
        Ok(self.stdin.flush()?)
    }
}

impl Drop for Console {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn qemu_run(bf: &BoardFile, fw: &Path, idf: &Path) -> Result<Value> {
    let build = build(fw, idf, "qemu")?;
    let image = flash_image(&build, idf)?;
    let started = Instant::now();
    let mut con = Console::start(&qemu_binary()?, &image)?;
    let result = qemu_script(bf, &mut con);
    std::fs::write(build.join("sim.log"), con.log.join("\n") + "\n")?;
    let mut r = result?;
    r["seconds"] = json!(started.elapsed().as_secs_f64().round());
    Ok(r)
}

/// What the QEMU run checks. Every problem is collected; `ok` is true only with none.
fn qemu_script(bf: &BoardFile, con: &mut Console) -> Result<Value> {
    let mut problems: Vec<String> = vec![];
    let banner: Value = serde_json::from_str(&con.expect("BOARD ")?).context("BOARD banner is not JSON")?;
    let want = [
        ("name", json!(bf.board.name)),
        ("rev", json!(bf.board.revision)),
        ("target", json!("qemu")),
        ("psram", json!(PSRAM_BYTES)),
        ("nvs", json!(true)),
    ];
    for (k, v) in want {
        if banner[k] != v {
            problems.push(format!("banner {k} is {}, expected {v}", banner[k]));
        }
    }

    let mut tests: Vec<Value> = vec![];
    let done: Value = loop {
        let l = con.expect("SELFTEST")?;
        if let Some(rest) = l.strip_prefix("_DONE") {
            break serde_json::from_str(rest.trim()).context("SELFTEST_DONE is not JSON")?;
        }
        if let Ok(t) = serde_json::from_str::<Value>(&l) {
            tests.push(t);
        }
    };
    for name in &bf.firmware.self_test {
        match tests.iter().find(|t| t["test"] == name.as_str()) {
            None => problems.push(format!("self-test {name} never reported")),
            Some(t) if t["result"] != "pass" && t["result"] != "skip" => {
                problems.push(format!("self-test {name}: {} ({})", t["result"], t["detail"].as_str().unwrap_or("")))
            }
            Some(_) => {}
        }
    }
    if done["fail"] != 0 || done["missing"] != 0 {
        problems.push(format!("self-test summary: {done}"));
    }

    // provisioning: set, list, delete a throwaway key; the value must never come back
    con.expect("PROV READY")?;
    let mut provision = true;
    con.send("PROV SET sim_check 73696d")?;
    provision &= con.expect("PROV ")? == "OK sim_check 3";
    con.send("PROV LIST")?;
    let mut listed = vec![];
    loop {
        let l = con.expect("PROV ")?;
        if l == "END" {
            break;
        }
        listed.push(l);
    }
    provision &= listed.iter().any(|l| l == "KEY sim_check 3");
    con.send("PROV DEL sim_check")?;
    provision &= con.expect("PROV ")? == "OK sim_check 0";
    provision &= !con.log.iter().any(|l| !l.starts_with(">>") && l.contains("73696d"));
    if !provision {
        problems.push("provisioning round trip failed (see sim.log)".into());
    }

    Ok(json!({
        "ok": problems.is_empty(),
        "banner": banner,
        "tests": tests,
        "summary": done,
        "provision": provision,
        "problems": problems,
    }))
}
