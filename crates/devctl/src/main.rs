//! `cargo run --release -p devctl -- <flash|selftest|provision|monitor> <board> [--port DEV]
//! [--qemu] [--seconds N]`: bring-up for a board under `boards/`. What each command does and
//! the safety rules: the crate docs (`src/lib.rs`).

use std::process::ExitCode;
use std::time::{Duration, Instant};

use anyhow::{Result, bail};
use devctl::{Link, QemuLink, SerialLink, Session, flash, provision, selftest};

const USAGE: &str = "usage: devctl <flash|selftest|provision|monitor> <board> [--port DEV] [--qemu] [--seconds N]";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String]) -> Result<bool> {
    let mut words: Vec<&str> = vec![];
    let mut port: Option<&str> = None;
    let mut qemu = false;
    let mut seconds: Option<u64> = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--port" => match it.next() {
                Some(p) => port = Some(p),
                None => bail!("--port needs a device (/dev/ttyACM0)"),
            },
            "--qemu" => qemu = true,
            "--seconds" => match it.next().and_then(|n| n.parse().ok()).filter(|&n| n > 0) {
                Some(n) => seconds = Some(n),
                None => bail!("--seconds needs a number of at least 1"),
            },
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(true);
            }
            s if s.starts_with('-') => bail!("unknown option {s:?}\n{USAGE}"),
            s => words.push(s),
        }
    }
    let [command, board] = words[..] else { bail!("{USAGE}") };
    if qemu && port.is_some() {
        bail!("--port and --qemu don't go together");
    }
    let dir = devctl::board_dir(board)?;
    let bf = devctl::load_board(&dir)?;
    let target = devctl::target(qemu);
    let open = || -> Result<Session<Box<dyn Link>>> {
        let link: Box<dyn Link> = if qemu { Box::new(QemuLink::open(&dir)?) } else { Box::new(SerialLink::open(port)?) };
        Ok(Session::new(link))
    };

    match command {
        "flash" => {
            if qemu {
                bail!("flash is for a real board; the sim stage builds the QEMU image");
            }
            flash::run(&bf, &dir, port)
        }
        "selftest" => {
            let mut s = open()?;
            println!("selftest: resetting {} on {}", bf.board.name, s.link.name());
            let r = selftest::run(&bf, &mut s, target)?;
            // real-board results only in bringup/; QEMU runs stay with the QEMU build
            let out = if qemu { dir.join("firmware/build-qemu") } else { dir.join("bringup") };
            std::fs::create_dir_all(&out)?;
            let base = out.join(format!("selftest-{}", r["date"].as_str().unwrap_or("undated")));
            std::fs::write(base.with_extension("json"), serde_json::to_string_pretty(&r)? + "\n")?;
            std::fs::write(base.with_extension("log"), s.log.join("\n") + "\n")?;
            let ok = r["ok"] == true;
            println!("== SELFTEST ({target}): {}", if ok { "PASS" } else { "FAIL" });
            for t in r["tests"].as_array().into_iter().flatten() {
                println!("   {:<14} {:<7} {}", t["test"].as_str().unwrap_or("?"), t["result"].as_str().unwrap_or("?"), t["detail"].as_str().unwrap_or(""));
            }
            for l in r["needs_person"].as_array().into_iter().flatten() {
                println!("   confirm by eye: {} {}", l["test"].as_str().unwrap_or("?"), l["look"].as_str().unwrap_or(""));
            }
            for p in r["problems"].as_array().into_iter().flatten() {
                println!("   problem: {}", p.as_str().unwrap_or("?"));
            }
            println!("selftest: {} (+ .log)", base.with_extension("json").display());
            Ok(ok)
        }
        "provision" => {
            if bf.provision.is_empty() {
                println!("provision: {} has no [provision] entries in board.toml; nothing to send", bf.board.name);
                return Ok(true);
            }
            let mut s = open()?;
            let sent = provision::run(&bf, &mut s, target)?;
            println!("== PROVISION ({target}): PASS, {} values stored and listed; none shown", sent.len());
            Ok(true)
        }
        "monitor" => {
            let mut s = open()?;
            let until = seconds.map(|n| Instant::now() + Duration::from_secs(n));
            loop {
                let wait = until.map_or(Duration::from_secs(1), |u| u.saturating_duration_since(Instant::now()).min(Duration::from_secs(1)));
                if let Some(l) = s.link.next_line(wait)? {
                    println!("{l}");
                }
                if until.is_some_and(|u| Instant::now() >= u) {
                    return Ok(true);
                }
            }
        }
        c => bail!("unknown command {c:?}\n{USAGE}"),
    }
}
