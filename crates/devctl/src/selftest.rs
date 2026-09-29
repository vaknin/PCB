//! `devctl selftest`: the firmware's self-test report from boot to `SELFTEST_DONE`, checked
//! with pcbgen's sim check. Prompts (`SELFTEST_PRESS`, `SELFTEST_LOOK`) are shown to the person
//! at the board as they arrive; what they are asked to look at is recorded as needing a person
//! to confirm, since only they can see it.

use std::time::{Duration, Instant};

use anyhow::Result;
use pcbgen::boardfile::BoardFile;
use serde_json::{Value, json};

use crate::{Link, Session};

/// Reset to `SELFTEST_DONE`; a real board's tests take a few seconds plus a button's 10 s.
const TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, PartialEq, Eq)]
pub enum Prompt {
    /// `SELFTEST_LOOK <test> <what>`: the person checks something by eye.
    Look { test: String, what: String },
    /// `SELFTEST_PRESS <test>`: the person presses a button now.
    Press { test: String },
}

pub fn prompt(line: &str) -> Option<Prompt> {
    if let Some(rest) = line.strip_prefix("SELFTEST_LOOK ") {
        let (test, what) = rest.trim().split_once(' ').unwrap_or((rest.trim(), ""));
        return (!test.is_empty()).then(|| Prompt::Look { test: test.into(), what: what.trim().into() });
    }
    let test = line.strip_prefix("SELFTEST_PRESS ")?.trim();
    (!test.is_empty()).then(|| Prompt::Press { test: test.into() })
}

/// What the person at the board is told. `board.toml`'s Wokwi steps say which button a press
/// prompt wants (`wait = "SELFTEST_PRESS boot_button"`, `press = "BOOT"`).
pub fn instruction(bf: &BoardFile, line: &str, p: &Prompt) -> String {
    match p {
        Prompt::Press { test } => match bf.sim.wokwi_steps.iter().find(|s| s.wait == line).and_then(|s| s.press.as_ref()) {
            Some(button) => format!("press the {button} button now, then let go ({test} test)"),
            None => format!("press the button for the {test} test now, then let go"),
        },
        Prompt::Look { test, what } => format!("look: {test} {what} (does the board show that now?)"),
    }
}

/// Resets the board and collects its report; `ok` in the result is true only with no problems.
pub fn run<L: Link>(bf: &BoardFile, s: &mut Session<L>, target: &str) -> Result<Value> {
    s.reset()?;
    let start = s.log.len();
    let until = Instant::now() + TIMEOUT;
    let mut looks = vec![];
    let mut end = None;
    while let Some(line) = s.line(until.saturating_duration_since(Instant::now()))? {
        match prompt(&line) {
            Some(p @ Prompt::Look { .. }) => {
                println!("   >>> {}", instruction(bf, &line, &p));
                if let Prompt::Look { test, what } = p {
                    looks.push(json!({"test": test, "look": what, "status": "needs a person to confirm"}));
                }
            }
            Some(p) => println!("   >>> {}", instruction(bf, &line, &p)),
            None => {}
        }
        if line.starts_with("SELFTEST_DONE") {
            end = Some(Ok(()));
            break;
        }
        if crate::crashed(&line) {
            end = Some(Err(format!("the firmware crashed: {line}")));
            break;
        }
    }
    let (banner, tests, summary, mut problems) = pcbgen::sim::report(bf, &s.log[start..], target);
    match end {
        Some(Ok(())) => {}
        Some(Err(e)) => problems.insert(0, e),
        None => problems.insert(0, format!("no SELFTEST_DONE within {} s of the reset", TIMEOUT.as_secs())),
    }
    Ok(json!({
        "ok": problems.is_empty(),
        "date": pcbgen::schematic::today(),
        "board": bf.board.name,
        "rev": bf.board.revision,
        "target": target,
        "port": s.link.name(),
        "banner": banner,
        "tests": tests,
        "summary": summary,
        "needs_person": looks,
        "problems": problems,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompts() {
        assert_eq!(prompt("SELFTEST_LOOK status_led on"), Some(Prompt::Look { test: "status_led".into(), what: "on".into() }));
        assert_eq!(prompt("SELFTEST_LOOK rgb red and green "), Some(Prompt::Look { test: "rgb".into(), what: "red and green".into() }));
        assert_eq!(prompt("SELFTEST_LOOK lcd"), Some(Prompt::Look { test: "lcd".into(), what: "".into() }));
        assert_eq!(prompt("SELFTEST_PRESS boot_button"), Some(Prompt::Press { test: "boot_button".into() }));
        for other in ["SELFTEST {\"test\":\"x\"}", "SELFTEST_PRESS ", "SELFTEST_LOOK ", "SELFTEST_DONE {}", "BOARD {}", ""] {
            assert_eq!(prompt(other), None, "{other}");
        }
    }

    #[test]
    fn instructions() {
        let bf = pcbgen::boardfile::load(&pcbgen::repo_root().join("boards/starter")).unwrap().unwrap();
        let say = |l: &str| instruction(&bf, l, &prompt(l).unwrap());
        assert_eq!(say("SELFTEST_PRESS boot_button"), "press the BOOT button now, then let go (boot_button test)");
        assert_eq!(say("SELFTEST_PRESS other"), "press the button for the other test now, then let go");
        assert_eq!(say("SELFTEST_LOOK status_led on"), "look: status_led on (does the board show that now?)");
    }
}
