//! The `sim` stage (D-025): the board's firmware in simulation, open source first.
//!
//! - **QEMU** (Espressif's fork, `idf_tools.py install qemu-xtensa`, unlimited): builds the
//!   firmware's QEMU target (`sdkconfig.defaults` + `sdkconfig.qemu`, in `firmware/build-qemu`),
//!   boots it with the module's 16 MB flash and 8 MB octal PSRAM, and checks the boot banner, the
//!   self-tests `board.toml` lists and a provisioning round trip. QEMU has no GPIO, I2C, I2S, USB,
//!   Wi-Fi or deep sleep, so tests that need them report "skip" there.
//! - **QEMU scenarios** (`crate::scenario`): board.toml's `[[sim.scenario]]` steps, each on a
//!   fresh copy of that image, then the board's own `firmware/sim/run.py` if it has one. A
//!   QEMU crash (a signal) reruns the scenario and is counted, never blamed on the firmware.
//! - **Wokwi** (`sim --wokwi`, free plan 50 simulated minutes a month): the pin checks QEMU
//!   can't do. Builds the Wokwi target (`firmware/build-wokwi`) and runs the files the `fw`
//!   stage generated (`crate::wokwi`) with `wokwi-cli`, a tight timeout and the token from
//!   `~/.config/wokwi/token` (passed in the environment, never printed). Only board-side
//!   parts go in: no real secret ever enters a Wokwi run (its network gateway is public).
//!   Every run is logged with its simulated seconds in `~/.config/wokwi/usage.jsonl`, so
//!   the month's quota use is known.
//!
//! Results go to `firmware/sim.json` for the review page (a run without `--wokwi` keeps the
//! last Wokwi result); the serial logs to `firmware/build-{qemu,wokwi}/sim.log`. ESP-IDF is
//! found at `$IDF_PATH` or `~/esp/esp-idf-v6.1`.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use regex::Regex;
use serde_json::{Value, json};

use crate::boardfile::BoardFile;
use crate::scenario;
use crate::wokwi;

/// ESP32-S3-WROOM-1-N16R8 (the only module pcbgen has a GPIO table for).
pub const FLASH: &str = "16MB";
pub const PSRAM_BYTES: u64 = 8 << 20;
/// How long the whole QEMU run may take (wall clock); a boot takes a few seconds.
const QEMU_TIMEOUT: Duration = Duration::from_secs(90);
/// Simulated time a Wokwi run may take. A run that times out is billed all of it, so keep it
/// tight: the starter's self-test ends about 3 s after boot.
const WOKWI_TIMEOUT_MS: u64 = 15_000;
/// The free plan's monthly allowance, simulated seconds.
const WOKWI_MONTH_SECONDS: f64 = 50.0 * 60.0;

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
    let built = build(fw, &idf, "qemu").and_then(|b| Ok((flash_image(&b, &idf)?, b)));
    let qemu = match &built {
        Ok((image, b)) => qemu_run(bf, b, image).unwrap_or_else(|e| json!({"ok": false, "problems": [format!("{e:#}")]})),
        Err(e) => json!({"ok": false, "problems": [format!("{e:#}")]}),
    };
    let qemu_ok = qemu["ok"] == true;
    print_summary("QEMU", &qemu);

    // the scenarios: board.toml's, then the board's own runner's (never on a stale image)
    let mut scenarios: Vec<Value> = vec![];
    let mut hook = Value::Null;
    match (&built, qemu_binary()) {
        (Ok((image, _)), Ok(q)) => {
            scenarios = scenario::run_all(&bf.sim.scenarios, &q, image, fw);
            if let Some((found, summary)) = scenario::run_hook(fw) {
                scenarios.extend(found);
                hook = summary;
            }
        }
        (Err(_), _) => {
            scenarios = bf.sim.scenarios.iter().map(|s| scenario::failed(s, "not run: the QEMU build failed")).collect();
        }
        (_, Err(e)) => {
            scenarios = bf.sim.scenarios.iter().map(|s| scenario::failed(s, &format!("not run: {e:#}"))).collect();
        }
    }
    scenario::print(&scenarios);
    let scenarios_ok = scenarios.iter().all(|s| s["ok"] == true);

    let old: Value = std::fs::read_to_string(out.join("sim.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(Value::Null);
    let mut wokwi_ok = true;
    let wokwi = if opts.wokwi {
        let r = match wokwi_run(bf, fw, &idf) {
            Ok(v) => v,
            Err(e) => json!({"ok": false, "problems": [format!("{e:#}")]}),
        };
        wokwi_ok = r["ok"] == true;
        print_summary("Wokwi", &r);
        r
    } else {
        old["wokwi"].clone() // keep the last Wokwi result; it costs quota to redo
    };
    let result = json!({
        "date": crate::schematic::today(),
        "seconds": started.elapsed().as_secs(),
        "qemu": qemu,
        "scenarios": scenarios,
        "scenario_hook": hook,
        "wokwi": wokwi,
    });
    std::fs::create_dir_all(out)?;
    std::fs::write(out.join("sim.json"), serde_json::to_string_pretty(&result)? + "\n")?;
    println!("sim: {}", out.join("sim.json").display());
    Ok(qemu_ok && scenarios_ok && wokwi_ok)
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

/// ESP-IDF: `$IDF_PATH`, else `~/esp/esp-idf-v6.1`.
pub fn idf_path() -> Result<PathBuf> {
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
pub fn idf_shell(idf: &Path, dir: &Path, script: &str, log: &Path) -> Result<()> {
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

/// Espressif's `qemu-system-xtensa`, the newest installed under `$IDF_TOOLS_PATH` (`~/.espressif`).
pub fn qemu_binary() -> Result<PathBuf> {
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

/// QEMU itself died from a signal (Espressif's QEMU 9.2.2 sometimes segfaults, HARDWARE_LESSONS):
/// that is the simulator, not the firmware, so a scenario is run again. Find it with
/// `err.downcast_ref::<QemuDied>()`.
#[derive(Debug)]
pub struct QemuDied(pub String);

impl std::fmt::Display for QemuDied {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for QemuDied {}

/// The firmware printed a crash (`Guru Meditation`, `abort()`).
pub fn is_crash_line(l: &str) -> bool {
    l.contains("Guru Meditation") || l.starts_with("abort()")
}

/// A running QEMU: its console lines arrive on a channel, so every wait has a deadline.
/// `log` holds every line read and the shown form of every line sent.
pub struct Console {
    child: Child,
    stdin: ChildStdin,
    lines: Receiver<String>,
    pub log: Vec<String>,
    deadline: Instant,
    /// The lines read (not the ones sent); `wait` looks from `cursor` on.
    read: Vec<String>,
    cursor: usize,
    /// Why QEMU is gone, once it is (so every later call says the same).
    gone: Option<String>,
    died: bool,
}

impl Console {
    /// Boots `image` (a whole 16 MB flash) with the module's octal PSRAM.
    pub fn start(qemu: &Path, image: &Path) -> Result<Console> {
        Self::start_with(qemu, image, &[])
    }

    /// The same, with more QEMU arguments, e.g. `["-nic", "user,model=open_eth"]` (Ethernet as
    /// the Wi-Fi stand-in, `CONFIG_ETH_USE_OPENETH`).
    pub fn start_with(qemu: &Path, image: &Path, extra: &[String]) -> Result<Console> {
        let mut cmd = Command::new(qemu);
        // QEMU 9.2.2 sometimes segfaults; preloading `scripts/nodump.c` keeps systemd-coredump
        // (and the desktop's crash notice) out of it. The exit status is unchanged.
        match Command::new(crate::repo_root().join("scripts/nodump.sh")).output() {
            Ok(o) if o.status.success() => drop(cmd.env("LD_PRELOAD", String::from_utf8_lossy(&o.stdout).trim())),
            _ => eprintln!("sim: scripts/nodump.sh failed; a QEMU crash will show as a desktop crash notice"),
        }
        let mut child = cmd
            .args(["-M", "esp32s3", "-m", &format!("{}M", PSRAM_BYTES >> 20)])
            .arg("-drive")
            .arg(format!("file={},if=mtd,format=raw", image.display()))
            .args(["-global", "driver=ssi_psram,property=is_octal,value=true", "-nographic", "-serial", "mon:stdio"])
            .args(extra)
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
        Ok(Console {
            child,
            stdin,
            lines: rx,
            log: vec![],
            deadline: Instant::now() + QEMU_TIMEOUT,
            read: vec![],
            cursor: 0,
            gone: None,
            died: false,
        })
    }

    /// The next console line, or None if none comes within `timeout`; an error once QEMU exited,
    /// a [`QemuDied`] if a signal killed it.
    pub fn next_line(&mut self, timeout: Duration) -> Result<Option<String>> {
        if self.gone.is_none() {
            match self.lines.recv_timeout(timeout) {
                Ok(l) => {
                    self.log.push(l.clone());
                    self.read.push(l.clone());
                    return Ok(Some(l));
                }
                Err(RecvTimeoutError::Timeout) => return Ok(None),
                Err(RecvTimeoutError::Disconnected) => self.exited(),
            }
        }
        let why = self.gone.clone().unwrap_or_default();
        if self.died { Err(QemuDied(why).into()) } else { Err(anyhow::anyhow!(why)) }
    }

    /// Both pipes closed: how QEMU ended.
    fn exited(&mut self) {
        use std::os::unix::process::ExitStatusExt;
        let status = self.child.wait().ok();
        let signal = status.and_then(|s| s.signal());
        self.died = signal.is_some();
        self.gone = Some(match (signal, status.and_then(|s| s.code())) {
            (Some(sig), _) => format!("QEMU itself crashed (signal {sig}); not the firmware's fault"),
            (None, Some(code)) => format!("QEMU exited with code {code}"),
            _ => "QEMU exited".into(),
        });
        self.log.push(format!("==== {}", self.gone.as_deref().unwrap_or("")));
    }

    /// Reads whatever arrives for `d` (the lines go to the log and stay for `wait`).
    pub fn drain(&mut self, d: Duration) {
        let end = Instant::now() + d;
        loop {
            let left = end.saturating_duration_since(Instant::now());
            if left.is_zero() || self.next_line(left.min(Duration::from_millis(50))).is_err() {
                return;
            }
        }
    }

    /// The next line from the cursor on that `re` finds (the cursor moves past it). A crash line
    /// first fails it (with the backtrace drained into the log); so does running out of
    /// `timeout`, after sending `STATUS` so that what the firmware says of itself is in the log.
    pub fn wait(&mut self, re: &Regex, timeout: Duration) -> Result<String> {
        let deadline = Instant::now() + timeout;
        loop {
            while self.cursor < self.read.len() {
                let l = self.read[self.cursor].clone();
                self.cursor += 1;
                if re.is_match(&l) {
                    return Ok(l);
                }
                if is_crash_line(&l) {
                    self.drain(Duration::from_secs(2));
                    bail!("the firmware crashed while waiting for /{re}/: {l}");
                }
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                if self.gone.is_none() && self.send("STATUS").is_ok() {
                    self.drain(Duration::from_secs(1));
                }
                bail!("timed out after {:.0} s waiting for /{re}/", timeout.as_secs_f64());
            }
            self.next_line(left.min(Duration::from_millis(500)))
                .map_err(|e| e.context(format!("while waiting for /{re}/")))?;
        }
    }

    /// The next `<tag> {json}` line whose `fields` all equal the ones given: its JSON.
    pub fn expect_json(&mut self, tag: &str, fields: &serde_json::Map<String, Value>, timeout: Duration) -> Result<Value> {
        let deadline = Instant::now() + timeout;
        let re = Regex::new(&format!(r"^{} \{{", regex::escape(tag)))?;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let l = self.wait(&re, left.max(Duration::from_millis(1))).map_err(|e| e.context(format!("waiting for {tag} {}", Value::Object(fields.clone()))))?;
            let v: Value = serde_json::from_str(&l[tag.len() + 1..]).with_context(|| format!("not JSON: {l}"))?;
            if json_matches(&v, fields) {
                return Ok(v);
            }
        }
    }

    /// Sends `SIM <what>` and waits (10 s) for the firmware's `SIM OK <what> @<ms>`. The cursor
    /// stays: lines printed meanwhile are still there for the next `wait`. Returns the reply.
    pub fn sim(&mut self, what: &str) -> Result<String> {
        let what = what.split_whitespace().collect::<Vec<_>>().join(" ");
        let since = self.read.len();
        self.send(&format!("SIM {what}"))?;
        let ok = Regex::new(&format!(r"^SIM OK {}( @|$)", regex::escape(&what)))?;
        let err = Regex::new(&format!(r"^SIM ERR {}$", regex::escape(&what)))?;
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut at = since;
        loop {
            while at < self.read.len() {
                let l = &self.read[at];
                at += 1;
                if ok.is_match(l) {
                    return Ok(l.clone());
                }
                if err.is_match(l) {
                    bail!("the firmware refused `SIM {what}`: {l}");
                }
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                bail!("no answer to `SIM {what}` in 10 s");
            }
            self.next_line(left.min(Duration::from_millis(100)))?;
        }
    }

    /// Kills QEMU (a pulled plug); the log stays.
    pub fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }

    /// An error if QEMU has exited (a [`QemuDied`] if a signal ended it) or the firmware printed
    /// a crash at any point, whether or not a wait was looking then.
    pub fn check_alive(&mut self) -> Result<()> {
        self.drain(Duration::from_millis(100));
        if self.gone.is_none() && self.child.try_wait().ok().flatten().is_some() {
            self.drain(Duration::from_millis(500)); // the rest of the pipes, then Disconnected
        }
        if self.gone.is_some() {
            self.next_line(Duration::ZERO)?;
        }
        if let Some(l) = self.read.iter().find(|l| is_crash_line(l)) {
            bail!("the firmware crashed: {l}");
        }
        Ok(())
    }

    /// Waits for a line starting with `prefix`; returns the rest of it.
    fn expect(&mut self, prefix: &str) -> Result<String> {
        loop {
            let left = self.deadline.saturating_duration_since(Instant::now());
            match self.next_line(left) {
                Ok(Some(l)) => {
                    if let Some(rest) = l.strip_prefix(prefix) {
                        return Ok(rest.trim().to_string());
                    }
                    let boots = self.log.iter().filter(|x| x.starts_with("ESP-ROM:")).count();
                    if l.contains("Guru Meditation") || l.starts_with("abort()") || boots > 1 {
                        bail!("the firmware crashed or reset while waiting for `{prefix}`: {l}");
                    }
                }
                Ok(None) => bail!("timed out waiting for `{prefix}`"),
                Err(_) => bail!("QEMU exited while waiting for `{prefix}` (see the log)"),
            }
        }
    }

    pub fn send(&mut self, line: &str) -> Result<()> {
        self.send_redacted(line, line)
    }

    /// Sends `line` but logs only `shown` (a line carrying a secret never enters `log`).
    pub fn send_redacted(&mut self, line: &str, shown: &str) -> Result<()> {
        self.log.push(format!(">> {shown}"));
        self.stdin.write_all(line.as_bytes())?;
        self.stdin.write_all(b"\n")?;
        Ok(self.stdin.flush()?)
    }
}

/// Every field in `want` is in `got` with the same value (numbers compared as numbers, so 1 and
/// 1.0 match); other fields of `got` don't matter.
pub fn json_matches(got: &Value, want: &serde_json::Map<String, Value>) -> bool {
    want.iter().all(|(k, w)| match (got.get(k), w) {
        (Some(Value::Number(a)), Value::Number(b)) => a.as_f64() == b.as_f64(),
        (Some(g), w) => g == w,
        (None, _) => false,
    })
}

impl Drop for Console {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn qemu_run(bf: &BoardFile, build: &Path, image: &Path) -> Result<Value> {
    let started = Instant::now();
    let mut con = Console::start(&qemu_binary()?, image)?;
    let result = qemu_script(bf, &mut con);
    std::fs::write(build.join("sim.log"), con.log.join("\n") + "\n")?;
    let mut r = result?;
    r["seconds"] = json!(started.elapsed().as_secs_f64().round());
    Ok(r)
}

/// The first `BOARD {...}` banner in a console log, checked against `board.toml` and `target`
/// (name, rev, target, 8 MiB PSRAM, NVS up): the banner (Null if missing) and its problems.
pub fn banner(bf: &BoardFile, log: &[String], target: &str) -> (Value, Vec<String>) {
    let mut problems: Vec<String> = vec![];
    let banner: Value = match log.iter().find_map(|l| l.strip_prefix("BOARD ")).map(|b| serde_json::from_str(b.trim())) {
        Some(Ok(v)) => v,
        Some(Err(_)) => {
            problems.push("BOARD banner is not JSON".into());
            Value::Null
        }
        None => {
            problems.push("no BOARD banner".into());
            Value::Null
        }
    };
    let want = [
        ("name", json!(bf.board.name)),
        ("rev", json!(bf.board.revision)),
        ("target", json!(target)),
        ("psram", json!(PSRAM_BYTES)),
        ("nvs", json!(true)),
    ];
    for (k, v) in want {
        if !banner.is_null() && banner[k] != v {
            problems.push(format!("banner {k} is {}, expected {v}", banner[k]));
        }
    }
    (banner, problems)
}

/// The boot banner and self-test report in a console log, checked against `board.toml`:
/// `(banner, tests, summary, problems)`. Tests may pass or skip; a failing or missing one, or
/// a banner that doesn't match the board and target, is a problem.
pub fn report(bf: &BoardFile, log: &[String], target: &str) -> (Value, Vec<Value>, Value, Vec<String>) {
    let (banner, mut problems) = banner(bf, log, target);
    let after = |prefix: &str| log.iter().find_map(|l| l.strip_prefix(prefix)).map(str::trim);

    let tests: Vec<Value> = log
        .iter()
        .filter_map(|l| l.strip_prefix("SELFTEST "))
        .filter_map(|t| serde_json::from_str(t.trim()).ok())
        .collect();
    for name in &bf.firmware.self_test {
        match tests.iter().find(|t| t["test"] == name.as_str()) {
            None => problems.push(format!("self-test {name} never reported")),
            Some(t) if t["result"] != "pass" && t["result"] != "skip" => {
                problems.push(format!("self-test {name}: {} ({})", t["result"], t["detail"].as_str().unwrap_or("")))
            }
            Some(_) => {}
        }
    }
    let done: Value = match after("SELFTEST_DONE").map(serde_json::from_str) {
        Some(Ok(v)) => v,
        _ => {
            problems.push("no SELFTEST_DONE summary".into());
            Value::Null
        }
    };
    if !done.is_null() && (done["fail"] != 0 || done["missing"] != 0) {
        problems.push(format!("self-test summary: {done}"));
    }
    (banner, tests, done, problems)
}

/// What the QEMU run checks. Every problem is collected; `ok` is true only with none.
fn qemu_script(bf: &BoardFile, con: &mut Console) -> Result<Value> {
    con.expect("SELFTEST_DONE")?;
    let (banner, tests, done, mut problems) = report(bf, &con.log, "qemu");

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
    // a line longer than the firmware's buffer must be refused, never stored cut off
    con.send(&format!("PROV SET sim_long {}", "00".repeat(600)))?;
    provision &= con.expect("PROV ")? == "ERR line";
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

fn home() -> Result<PathBuf> {
    Ok(PathBuf::from(std::env::var("HOME")?))
}

/// `wokwi-cli` from PATH, or where its installer puts it.
fn wokwi_cli() -> Result<PathBuf> {
    let on_path = std::env::var_os("PATH")
        .into_iter()
        .flat_map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
        .map(|d| d.join("wokwi-cli"))
        .find(|p| p.exists());
    let local = home()?.join(".local/bin/wokwi-cli");
    on_path.or(local.exists().then_some(local)).context("wokwi-cli not installed (https://docs.wokwi.com/wokwi-ci/cli-installation)")
}

fn wokwi_run(bf: &BoardFile, fw: &Path, idf: &Path) -> Result<Value> {
    for f in [wokwi::DIAGRAM, wokwi::TOML, wokwi::SCENARIO] {
        if !fw.join(f).exists() {
            bail!("no {f} in {}: run the fw stage (it writes the Wokwi files when a board.toml pin has a sim part)", fw.display());
        }
    }
    let cli = wokwi_cli()?;
    let token_file = home()?.join(".config/wokwi/token");
    let token = std::fs::read_to_string(&token_file).with_context(|| format!("no Wokwi token at {}", token_file.display()))?;
    // lint is local and free: a bad diagram fails here, before any quota is spent
    let lint = Command::new(&cli).args(["lint", "-q"]).arg(fw).output().context("running wokwi-cli lint")?;
    let lint_text = format!("{}{}", String::from_utf8_lossy(&lint.stdout), String::from_utf8_lossy(&lint.stderr));
    if !lint.status.success() {
        bail!("wokwi-cli lint rejected {}:\n{}", fw.join(wokwi::DIAGRAM).display(), lint_text.trim());
    }

    let build = build(fw, idf, "wokwi")?;
    let serial = build.join("sim.log");
    let _ = std::fs::remove_file(&serial);
    let started = Instant::now();
    // `timeout` bounds the wall clock too, in case the Wokwi service never answers
    let out = Command::new("timeout")
        .arg("300")
        .arg(&cli)
        .arg(fw)
        .args(["--timeout", &WOKWI_TIMEOUT_MS.to_string(), "--scenario"])
        .arg(fw.join(wokwi::SCENARIO))
        .arg("--serial-log-file")
        .arg(&serial)
        .env("WOKWI_CLI_TOKEN", token.trim())
        .stdin(Stdio::null())
        .output()
        .context("running wokwi-cli")?;
    let wall = started.elapsed().as_secs_f64();
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    std::fs::write(build.join("wokwi-cli.log"), &text)?;
    let code = out.status.code().unwrap_or(-1);

    let log: Vec<String> = std::fs::read_to_string(&serial).unwrap_or_default().lines().map(|l| l.trim_end_matches('\r').to_string()).collect();
    let (banner, tests, done, mut problems) = report(bf, &log, "wokwi");
    let timed_out = code == 42;
    match code {
        0 => {}
        42 => problems.insert(0, format!("the scenario didn't finish within {} s of simulated time", WOKWI_TIMEOUT_MS / 1000)),
        124 => problems.insert(0, "wokwi-cli ran 300 s of wall time without finishing".into()),
        _ => {
            // a failed scenario step throws `Error: [<ns>] <why>`; otherwise show the last line
            let plain = Regex::new(r"\x1b\[[0-9;]*m").unwrap().replace_all(&text, "").into_owned();
            let why = plain.lines().find(|l| l.starts_with("Error")).or(plain.lines().rfind(|l| !l.trim().is_empty())).unwrap_or("");
            problems.insert(0, format!("wokwi-cli exit {code}: {} (log: {})", why.trim(), build.join("wokwi-cli.log").display()));
        }
    }
    // Billed simulated time: the whole timeout if it ran out, else about the uptime at
    // SELFTEST_DONE (the firmware prints it; the boot ROM's fraction of a second is not in it).
    let sim_seconds = if timed_out {
        (WOKWI_TIMEOUT_MS / 1000) as f64
    } else {
        done["ms"].as_f64().map_or(wall.min((WOKWI_TIMEOUT_MS / 1000) as f64), |ms| ms / 1000.0)
    };
    let month_total = wokwi_ledger(&bf.board.name, sim_seconds, problems.is_empty())?;
    println!(
        "   Wokwi quota: ~{sim_seconds:.1} s this run; ~{:.0} of {:.0} s used this month (~/.config/wokwi/usage.jsonl)",
        month_total, WOKWI_MONTH_SECONDS
    );
    Ok(json!({
        "ok": problems.is_empty(),
        "date": crate::schematic::today(),
        "banner": banner,
        "tests": tests,
        "summary": done,
        "sim_seconds": (sim_seconds * 10.0).round() / 10.0,
        "wall_seconds": wall.round(),
        "problems": problems,
    }))
}

/// Appends this run to the local Wokwi usage log and returns the month's simulated seconds.
fn wokwi_ledger(board: &str, sim_seconds: f64, ok: bool) -> Result<f64> {
    let path = home()?.join(".config/wokwi/usage.jsonl");
    let today = crate::schematic::today();
    let line = json!({"date": today, "board": board, "sim_seconds": sim_seconds, "ok": ok}).to_string();
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&path)?;
    writeln!(f, "{line}")?;
    let month = &today[..7];
    let total = std::fs::read_to_string(&path)?
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter(|v| v["date"].as_str().is_some_and(|d| d.starts_with(month)))
        .filter_map(|v| v["sim_seconds"].as_f64())
        .sum();
    Ok(total)
}
