//! devctl: bring-up for the pipeline's ESP32-S3 boards (D-025), over the board's USB console,
//! or with `--qemu` over the firmware's QEMU image. `<board>` is a directory under `boards/`;
//! its `board.toml` says what the firmware must report.
//!
//! - `flash <board>`: builds the real target (`idf.py -B build build`), refuses unless that
//!   build's sdkconfig is `BOARD_TARGET_REAL`, checks the attached chip is an ESP32-S3 with
//!   16 MB flash, writes the checked build with esptool, then checks the boot banner.
//! - `selftest <board>`: resets the board, tells the person at it what to press or look at
//!   (`SELFTEST_PRESS`/`SELFTEST_LOOK`), checks the report against `board.toml` and writes
//!   `bringup/selftest-<date>.json` and `.log`. With `--qemu` they go to `firmware/build-qemu/`,
//!   so `bringup/` only ever holds real-board results.
//! - `provision <board>`: sends each `[provision]` value (`file:<path>#<field>` or `prompt`)
//!   to the board's NVS and checks it was stored.
//! - `monitor <board> [--seconds N]`: prints the console.
//!
//! Safety rules:
//! - Only a build whose sdkconfig says `BOARD_TARGET_REAL` is flashed; QEMU and Wokwi builds
//!   never reach a board.
//! - A secret is never printed, logged or put in an error message: logs show
//!   `>> PROV SET <key> (<n> bytes hidden)`, and value buffers are zeroed after use.
//! - No secret is sent before the banner shows this board's name, revision and target.
//!
//! The serial side (reset by RTS, the port re-enumerating, esptool's output) is UNVERIFIED:
//! there is no board yet (Phase D.5).

pub mod flash;
pub mod link;
pub mod provision;
pub mod selftest;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Result, bail};
use pcbgen::boardfile::BoardFile;
use serde_json::Value;

pub use link::{Link, QemuLink, SerialLink};

/// Reset to banner; a real board with 8 MB of PSRAM tested at boot takes about 3 s.
const BOOT_TIMEOUT: Duration = Duration::from_secs(20);

/// `boards/<board>` in this repository.
pub fn board_dir(board: &str) -> Result<PathBuf> {
    if board.is_empty() || !board.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_') {
        bail!("{board:?} is not a board name (a directory under boards/)");
    }
    Ok(pcbgen::repo_root().join("boards").join(board))
}

pub fn load_board(dir: &Path) -> Result<BoardFile> {
    match pcbgen::boardfile::load(dir)? {
        Some(bf) => Ok(bf),
        None => bail!("no board.toml in {}", dir.display()),
    }
}

/// The banner's `target` for a link: "qemu" or "real".
pub fn target(qemu: bool) -> &'static str {
    if qemu { "qemu" } else { "real" }
}

/// A line that means the firmware crashed.
pub fn crashed(line: &str) -> bool {
    line.contains("Guru Meditation") || line.starts_with("abort()")
}

/// A link plus devctl's own log of it: every line read, and the shown form of every line sent.
pub struct Session<L: Link> {
    pub link: L,
    pub log: Vec<String>,
}

impl<L: Link> Session<L> {
    pub fn new(link: L) -> Self {
        Session { link, log: vec![] }
    }

    pub fn line(&mut self, timeout: Duration) -> Result<Option<String>> {
        let l = self.link.next_line(timeout)?;
        if let Some(l) = &l {
            self.log.push(l.clone());
        }
        Ok(l)
    }

    /// Sends `line`; every log gets `shown` instead.
    pub fn send(&mut self, line: &str, shown: &str) -> Result<()> {
        self.log.push(format!(">> {shown}"));
        self.link.send(line, shown)
    }

    pub fn reset(&mut self) -> Result<()> {
        self.log.push("== reset".into());
        self.link.reset()
    }

    /// Waits up to `timeout` for a line starting with `prefix` and returns it; a crash is an error.
    pub fn expect(&mut self, prefix: &str, timeout: Duration) -> Result<String> {
        let until = Instant::now() + timeout;
        loop {
            match self.line(until.saturating_duration_since(Instant::now()))? {
                Some(l) if l.starts_with(prefix) => return Ok(l),
                Some(l) if crashed(&l) => bail!("the firmware crashed while waiting for `{prefix}`: {l}"),
                Some(_) => {}
                None => bail!("no `{prefix}` line within {} s on {}", timeout.as_secs(), self.link.name()),
            }
        }
    }
}

/// Resets the board and reads its banner, checked against `board.toml` (pcbgen's sim check):
/// the banner and its problems.
pub fn boot<L: Link>(bf: &BoardFile, s: &mut Session<L>, target: &str) -> Result<(Value, Vec<String>)> {
    s.reset()?;
    let line = s.expect("BOARD ", BOOT_TIMEOUT)?;
    Ok(pcbgen::sim::banner(bf, &[line], target))
}
