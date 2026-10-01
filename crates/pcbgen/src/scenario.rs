//! QEMU scenarios (D-025), run by the `sim` stage after its boot check:
//!
//! - `[[sim.scenario]]` in `board.toml` (`boardfile::Scenario`): the firmware driven over its
//!   console (`send`, `sim` for the `SIM` pin stand-ins of `firmware/components/simcmd`, `wait`
//!   for a line, `expect_json` for a `<TAG> {json}` line, `sleep_ms`, `reboot`), each scenario
//!   on a fresh copy of the QEMU image, its log in `firmware/build-qemu/scenarios/<name>.log`.
//! - A board's own runner, `firmware/sim/run.py` (the hook), for what steps can't say (a mock
//!   server, several firmware builds): the stage runs it and reads its
//!   `build-qemu/scenarios.json`.
//!
//! A scenario passes or fails on what the firmware prints. QEMU itself dying from a signal says
//! nothing about the firmware: that scenario is run again, up to `CRASH_RETRIES` times, the
//! crashed attempt's log kept as `<name>.log.crash<n>`, and the count goes into the result.
//! Every result is `{scenario, source, about, ok, seconds, evidence, qemu_crashes, log}` and
//! lands in `firmware/sim.json`'s `scenarios`.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime};

use anyhow::{Context, Result, bail};
use regex::Regex;
use serde_json::{Value, json};

use crate::boardfile::{Scenario, ScenarioStep};
use crate::sim::{Console, QemuDied};

/// How many times a scenario is run again after QEMU itself crashed.
pub const CRASH_RETRIES: u32 = 3;
/// A `wait` or `expect_json` without `timeout_ms`.
const DEFAULT_WAIT: Duration = Duration::from_secs(30);
/// Boot to `PROV READY` before provisioning (PSRAM test and NVS start included).
const BOOT_WAIT: Duration = Duration::from_secs(60);
/// The board's own scenario runner, relative to its firmware project.
pub const HOOK: &str = "sim/run.py";
/// Where scenario logs and images go, relative to the firmware project.
pub const OUT: &str = "build-qemu/scenarios";

/// Runs every scenario on copies of `base` (the stage's QEMU flash image), logs in `fw/OUT`.
pub fn run_all(scenarios: &[Scenario], qemu: &Path, base: &Path, fw: &Path) -> Vec<Value> {
    let out = fw.join(OUT);
    if let Err(e) = std::fs::create_dir_all(&out) {
        return scenarios.iter().map(|s| failed(s, &format!("can't make {}: {e}", out.display()))).collect();
    }
    scenarios.iter().map(|s| run_one(s, qemu, base, &out)).collect()
}

/// A result for a scenario that could not be run at all.
pub fn failed(sc: &Scenario, why: &str) -> Value {
    json!({"scenario": sc.name, "source": "board.toml", "about": sc.about, "ok": false, "seconds": 0.0,
           "evidence": why, "qemu_crashes": 0, "log": Value::Null})
}

pub fn run_one(sc: &Scenario, qemu: &Path, base: &Path, out: &Path) -> Value {
    let started = Instant::now();
    let image = out.join(format!("{}.bin", sc.name));
    let log_path = out.join(format!("{}.log", sc.name));
    let mut crashes = 0;
    let (ok, mut evidence) = loop {
        let mut log = vec![];
        let result = std::fs::copy(base, &image)
            .with_context(|| format!("copying {}", base.display()))
            .and_then(|_| attempt(sc, qemu, &image, &mut log));
        let _ = std::fs::write(&log_path, log.join("\n") + "\n");
        match result {
            Ok(ev) => break (true, ev),
            Err(e) if e.downcast_ref::<QemuDied>().is_some() && crashes < CRASH_RETRIES => {
                crashes += 1;
                let _ = std::fs::rename(&log_path, out.join(format!("{}.log.crash{crashes}", sc.name)));
            }
            Err(e) => break (false, format!("{e:#}")),
        }
    };
    if crashes > 0 {
        evidence += &format!(" [QEMU itself crashed {crashes} time(s) first; rerun]");
    }
    let rel = log_path.strip_prefix(out.parent().and_then(Path::parent).unwrap_or(out)).unwrap_or(&log_path);
    json!({
        "scenario": sc.name,
        "source": "board.toml",
        "about": sc.about,
        "ok": ok,
        "seconds": (started.elapsed().as_secs_f64() * 10.0).round() / 10.0,
        "evidence": evidence,
        "qemu_crashes": crashes,
        "log": rel.to_string_lossy(),
    })
}

/// One run of the scenario; Ok holds its evidence (the lines its steps matched). `log` gets
/// every console line of every boot.
fn attempt(sc: &Scenario, qemu: &Path, image: &Path, log: &mut Vec<String>) -> Result<String> {
    let extra: Vec<String> = if sc.nic { vec!["-nic".into(), "user,model=open_eth".into()] } else { vec![] };
    log.push("==== QEMU start".into());
    let mut con = Console::start_with(qemu, image, &extra)?;
    let mut evidence = vec![];
    let result = steps(sc, &mut con, qemu, image, &extra, log, &mut evidence).and_then(|_| con.check_alive());
    con.kill();
    log.append(&mut con.log);
    result?;
    Ok(evidence.join(" | "))
}

/// A line as evidence: never longer than 160 characters.
fn short(l: &str) -> String {
    match l.char_indices().nth(160) {
        Some((i, _)) => format!("{}…", &l[..i]),
        None => l.to_string(),
    }
}

fn hex(s: &str) -> String {
    s.bytes().map(|b| format!("{b:02x}")).collect()
}

fn steps(sc: &Scenario, con: &mut Console, qemu: &Path, image: &Path, extra: &[String], log: &mut Vec<String>, evidence: &mut Vec<String>) -> Result<()> {
    if !sc.provision.is_empty() {
        con.wait(&Regex::new("^PROV READY")?, BOOT_WAIT).context("before provisioning")?;
        for (k, v) in &sc.provision {
            // literals (board.toml says so), but logged by length like devctl's real values
            con.send_redacted(&format!("PROV SET {k} {}", hex(v)), &format!("PROV SET {k} ({} bytes)", v.len()))?;
            con.wait(&Regex::new(&format!("^PROV OK {} {}$", regex::escape(k), v.len()))?, Duration::from_secs(10))
                .with_context(|| format!("provisioning {k}"))?;
        }
        evidence.push(format!("provisioned {}", sc.provision.keys().cloned().collect::<Vec<_>>().join(", ")));
    }
    for (i, st) in sc.steps.iter().enumerate() {
        step(st, con, qemu, image, extra, log, evidence).with_context(|| format!("step {} ({})", i + 1, describe(st)))?;
    }
    Ok(())
}

/// A step in the words of board.toml, for error messages.
fn describe(st: &ScenarioStep) -> String {
    let q = |s: &Option<String>| s.as_deref().unwrap_or("").to_string();
    match st.verbs().first().copied() {
        Some("send") => format!("send {:?}", q(&st.send)),
        Some("sim") => format!("sim {:?}", q(&st.sim)),
        Some("wait") => format!("wait {:?}", q(&st.wait)),
        Some("expect_json") => format!("expect_json {:?}", q(&st.expect_json)),
        Some("sleep_ms") => format!("sleep_ms {}", st.sleep_ms.unwrap_or(0)),
        Some("reboot") => "reboot".into(),
        _ => "no verb".into(),
    }
}

fn step(st: &ScenarioStep, con: &mut Console, qemu: &Path, image: &Path, extra: &[String], log: &mut Vec<String>, evidence: &mut Vec<String>) -> Result<()> {
    let timeout = st.timeout_ms.map_or(DEFAULT_WAIT, Duration::from_millis);
    if let Some(s) = &st.send {
        con.send(s)?;
    } else if let Some(s) = &st.sim {
        evidence.push(short(&con.sim(s)?));
    } else if let Some(w) = &st.wait {
        evidence.push(short(&con.wait(&Regex::new(w)?, timeout)?));
    } else if let Some(tag) = &st.expect_json {
        let fields: serde_json::Map<String, Value> = st.fields.clone().unwrap_or_default().into_iter().collect();
        let v = con.expect_json(tag, &fields, timeout)?;
        evidence.push(short(&format!("{tag} {v}")));
    } else if let Some(ms) = st.sleep_ms {
        con.drain(Duration::from_millis(ms));
    } else if st.reboot == Some(true) {
        con.kill();
        log.append(&mut con.log);
        log.push("==== QEMU reboot (same image)".into());
        *con = Console::start_with(qemu, image, extra)?;
        evidence.push("rebooted".into());
    } else {
        bail!("a step with no verb (the board.toml gate catches this)");
    }
    Ok(())
}

/// Runs the board's own `firmware/sim/run.py`, if it has one, and returns its scenarios (tagged
/// `source = "sim/run.py"`) with its whole report; None without one.
///
/// It runs without `--no-build`: its builds are its own (capture-clip's update and rollback
/// scenarios need `build-qemu-v2` and `build-qemu-bad`, which only it knows how to make), and its
/// `build-qemu` is the one this stage just built with the same defaults, so ninja finds nothing to do.
pub fn run_hook(fw: &Path) -> Option<(Vec<Value>, Value)> {
    let script = fw.join(HOOK);
    if !script.exists() {
        return None;
    }
    let report_path = fw.join("build-qemu/scenarios.json");
    let started = SystemTime::now();
    let t0 = Instant::now();
    println!("== SIM ({HOOK}): running the board's own scenarios");
    let status = Command::new("python3")
        .arg(&script)
        .current_dir(fw)
        .env("PCB_ROOT", crate::repo_root())
        .stdin(Stdio::null())
        .status();
    let fail = |why: String| {
        let r = json!({"scenario": HOOK, "source": HOOK, "about": "the board's own scenario runner", "ok": false,
                       "seconds": (t0.elapsed().as_secs_f64() * 10.0).round() / 10.0, "evidence": why, "qemu_crashes": 0, "log": Value::Null});
        Some((vec![r], json!({"ok": false})))
    };
    let code = match status {
        Ok(s) => s.code(),
        Err(e) => return fail(format!("couldn't run python3 {}: {e}", script.display())),
    };
    let fresh = std::fs::metadata(&report_path).and_then(|m| m.modified()).is_ok_and(|t| t >= started);
    let report: Option<Value> = fresh.then(|| std::fs::read_to_string(&report_path).ok()).flatten().and_then(|t| serde_json::from_str(&t).ok());
    let Some(report) = report else {
        return fail(format!("{HOOK} exited with {code:?} and wrote no new {}", report_path.display()));
    };
    let mut found: Vec<Value> = report["scenarios"].as_array().cloned().unwrap_or_default();
    for s in &mut found {
        s["source"] = json!(HOOK);
    }
    if found.is_empty() || (code != Some(0) && found.iter().all(|s| s["ok"] == true)) {
        // it failed outside any scenario (a build, the image): say so in the table
        return fail(format!("{HOOK} exited with {code:?}; its report lists {} scenario(s)", found.len()));
    }
    let summary = json!({"ok": report["ok"] == true && code == Some(0), "exit": code, "date": report["date"], "low_water": report["low_water"]});
    Some((found, summary))
}

/// The table rows the stage prints.
pub fn print(results: &[Value]) {
    if results.is_empty() {
        return;
    }
    let passed = results.iter().filter(|r| r["ok"] == true).count();
    println!("== SIM (QEMU scenarios): {} ({passed} of {})", if passed == results.len() { "PASS" } else { "FAIL" }, results.len());
    for r in results {
        let crashes = r["qemu_crashes"].as_u64().unwrap_or(0);
        println!(
            "   {:<14} {:<5} {:>6.1} s{}  {}",
            r["scenario"].as_str().unwrap_or("?"),
            if r["ok"] == true { "pass" } else { "FAIL" },
            r["seconds"].as_f64().unwrap_or(0.0),
            if crashes > 0 { format!(" ({crashes} QEMU crash)") } else { String::new() },
            short(r["evidence"].as_str().unwrap_or(""))
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    /// A stand-in for QEMU: a shell script that boots, answers SIM lines like simcmd, prints a
    /// JSON line on `PING`, and on `DIE` kills itself with SIGSEGV the first `$1` times (counted
    /// in a file next to the image, which is QEMU's `-drive file=` argument).
    const FAKE: &str = r#"#!/bin/sh
img=""; for a in "$@"; do case "$a" in file=*) img="${a#file=}"; img="${img%%,*}";; esac; done
nic=0; for a in "$@"; do [ "$a" = "user,model=open_eth" ] && nic=1; done
echo "ESP-ROM:esp32s3"
echo "BOARD {\"name\":\"t\",\"nic\":$nic,\"img\":\"$(head -c 4 "$img")\"}"
echo "PROV READY"
while read -r l; do
  case "$l" in
    "PROV SET "*) set -- $l; echo "PROV OK $3 $(( ${#4} / 2 ))";;
    "SIM BOGUS"*) echo "SIM ERR ${l#SIM }";;
    "SIM "*) echo "noise"; echo "SIM OK ${l#SIM } @42";;
    PING) echo 'EV {"n":1,"x":"a"}'; echo 'EV {"n":2,"x":"b","y":1.0}';;
    STATUS) echo 'STATUS {"up":1}';;
    DIE) c=$(cat "$img.n" 2>/dev/null || echo 0); echo $((c+1)) > "$img.n"; [ "$c" -lt "${FAKE_DIES:-0}" ] && kill -SEGV $$;;
    CRASH) echo "Guru Meditation Error: Core 0 panic'ed"; echo "Backtrace: 0x1";;
    QUIT) exit 3;;
  esac
done
"#;

    fn setup(name: &str, dies: u32) -> (std::path::PathBuf, std::path::PathBuf, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("pcbgen-scenario-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("fw")).unwrap();
        let qemu = dir.join("qemu");
        std::fs::write(&qemu, FAKE.replace("${FAKE_DIES:-0}", &dies.to_string())).unwrap();
        std::fs::set_permissions(&qemu, std::fs::Permissions::from_mode(0o755)).unwrap();
        let base = dir.join("base.bin");
        std::fs::write(&base, "IMG0").unwrap();
        (dir, qemu, base)
    }

    fn scenario(steps: &str) -> Scenario {
        let toml = format!(
            "[board]\nname = \"t\"\nrevision = \"A\"\nmodule = \"U1\"\n[[power.source]]\nname = \"u\"\nwhat = \"u\"\n[[power.source.limit]]\nname = \"l\"\nma = 1\nsource = \"s\"\n\
             [[sim.scenario]]\nname = \"s\"\nabout = \"a test\"\n{steps}"
        );
        crate::boardfile::parse(&toml).unwrap().sim.scenarios.remove(0)
    }

    #[test]
    fn steps_pass_and_fail() {
        let (dir, qemu, base) = setup("steps", 0);
        let fw = dir.join("fw");
        let sc = scenario(
            "nic = true\nprovision = { note = \"hi\" }\n\
             [[sim.scenario.step]]\nsim = \"PRESS  300\"\n\
             [[sim.scenario.step]]\nsend = \"PING\"\n\
             [[sim.scenario.step]]\nexpect_json = \"EV\"\nfields = { x = \"b\", y = 1 }\n\
             [[sim.scenario.step]]\nsleep_ms = 50\n\
             [[sim.scenario.step]]\nreboot = true\n\
             [[sim.scenario.step]]\nwait = '^BOARD \\{.*\"nic\":1'\n\
             [[sim.scenario.step]]\nwait = \"^PROV READY\"\n",
        );
        let r = &run_all(&[sc], &qemu, &base, &fw)[0];
        assert_eq!(r["ok"], true, "{r}");
        let ev = r["evidence"].as_str().unwrap();
        assert!(ev.starts_with("provisioned note | SIM OK PRESS 300 @42 | EV {\"n\":2,\"x\":\"b\",\"y\":1.0} | rebooted | BOARD {"), "{ev}");
        assert!(ev.ends_with("\"img\":\"IMG0\"} | PROV READY"), "{ev}");
        assert_eq!((r["qemu_crashes"].as_u64(), r["source"].as_str()), (Some(0), Some("board.toml")));
        assert_eq!(r["log"], "build-qemu/scenarios/s.log");
        let log = std::fs::read_to_string(fw.join("build-qemu/scenarios/s.log")).unwrap();
        // the provisioned value is logged by length only; both boots are in the log
        assert!(log.contains(">> PROV SET note (2 bytes)") && !log.contains("6869"), "{log}");
        assert!(log.contains("==== QEMU reboot (same image)") && log.matches("PROV READY").count() == 2, "{log}");

        // a timeout fails, and STATUS goes into the log first
        let sc = scenario("[[sim.scenario.step]]\nwait = \"^NEVER\"\ntimeout_ms = 300\n");
        let r = &run_all(&[sc], &qemu, &base, &fw)[0];
        assert_eq!(r["ok"], false);
        assert!(r["evidence"].as_str().unwrap().contains("step 1 (wait \"^NEVER\"): timed out"), "{r}");
        let log = std::fs::read_to_string(fw.join("build-qemu/scenarios/s.log")).unwrap();
        assert!(log.contains(">> STATUS\nSTATUS {\"up\":1}"), "{log}");

        // a refused SIM line, a crash line, a firmware exit: each a failure, not a QEMU crash
        for (steps, want) in [
            ("[[sim.scenario.step]]\nsim = \"BOGUS 1\"\n", "refused `SIM BOGUS 1`"),
            ("[[sim.scenario.step]]\nsend = \"CRASH\"\n[[sim.scenario.step]]\nwait = \"^NEVER\"\n", "the firmware crashed while waiting"),
            ("[[sim.scenario.step]]\nsend = \"CRASH\"\n[[sim.scenario.step]]\nsleep_ms = 200\n", "the firmware crashed: Guru Meditation"),
            ("[[sim.scenario.step]]\nsend = \"QUIT\"\n[[sim.scenario.step]]\nwait = \"^NEVER\"\n", "QEMU exited with code 3"),
            ("[[sim.scenario.step]]\nexpect_json = \"EV\"\nfields = { n = 3 }\ntimeout_ms = 200\n", "waiting for EV {\"n\":3}"),
        ] {
            let r = &run_all(&[scenario(steps)], &qemu, &base, &fw)[0];
            assert!(r["ok"] == false && r["qemu_crashes"] == 0, "{steps}: {r}");
            assert!(r["evidence"].as_str().unwrap().contains(want), "{steps}: want {want:?} in {r}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn qemu_crashes_are_retried_and_counted() {
        let steps = "[[sim.scenario.step]]\nsend = \"DIE\"\n[[sim.scenario.step]]\nwait = \"^NEVER\"\ntimeout_ms = 2000\n";
        // dies twice, then lives: the third run's own result (a timeout here), with 2 crashes
        let (dir, qemu, base) = setup("retry", 2);
        let fw = dir.join("fw");
        // the counter lives next to the image: each attempt copies the image, not the counter
        let r = &run_all(&[scenario(steps)], &qemu, &base, &fw)[0];
        assert_eq!((r["ok"].as_bool(), r["qemu_crashes"].as_u64()), (Some(false), Some(2)), "{r}");
        let ev = r["evidence"].as_str().unwrap();
        assert!(ev.contains("timed out") && ev.ends_with("[QEMU itself crashed 2 time(s) first; rerun]"), "{ev}");
        let out = fw.join(OUT);
        assert!(out.join("s.log.crash1").exists() && out.join("s.log.crash2").exists() && !out.join("s.log.crash3").exists());
        assert!(std::fs::read_to_string(out.join("s.log.crash1")).unwrap().contains("QEMU itself crashed (signal 11)"));
        let _ = std::fs::remove_dir_all(&dir);

        // always dies: CRASH_RETRIES reruns, then a failure that says it was QEMU
        let (dir, qemu, base) = setup("dies", 99);
        let r = &run_all(&[scenario(steps)], &qemu, &base, &dir.join("fw"))[0];
        assert_eq!((r["ok"].as_bool(), r["qemu_crashes"].as_u64()), (Some(false), Some(CRASH_RETRIES as u64)), "{r}");
        assert!(r["evidence"].as_str().unwrap().contains("QEMU itself crashed (signal 11)"), "{r}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn json_fields_match_as_numbers() {
        let want = |v: Value| v.as_object().unwrap().clone();
        let got = json!({"n": 1, "x": "a", "y": 1.0, "o": {"k": [1]}});
        assert!(crate::sim::json_matches(&got, &want(json!({"n": 1.0, "y": 1}))));
        assert!(crate::sim::json_matches(&got, &want(json!({"o": {"k": [1]}, "x": "a"}))));
        assert!(!crate::sim::json_matches(&got, &want(json!({"x": "b"}))));
        assert!(!crate::sim::json_matches(&got, &want(json!({"z": null}))));
    }
}
