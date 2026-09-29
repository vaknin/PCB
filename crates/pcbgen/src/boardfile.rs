//! `board.toml`: the facts that the circuit, the firmware and the spec must agree on
//! (D-022, D-023). One per board, next to its `Cargo.toml`:
//!
//! ```toml
//! [board]          name = "starter"; revision = "0"; module = "U1"
//! [[pin]]          signal = "I2C_SDA"; pin = "IO1"; net = "I2C_SDA"; gpio = 1; dir = "io"
//! [power]          source = "USB-C 5 V"; budget_ma = 500
//! [[power.load]]   name = "ESP32-S3 Wi-Fi TX peak"; ma = 355; source = "WROOM-1 datasheet"
//! [[requirement]]  id = "R1"; text = "..."; covered_by = ["part:J1", "pin:I2C_SDA", "test:sht40", "gate:drc"]
//! [firmware]       self_test = ["sht40"]
//! ```
//!
//! `gate` checks it against the circuit and the board's `spec.md`; `header` turns it into
//! the firmware's `board_pins.h`. Unknown keys are errors, so a typo can't pass silently.

use std::collections::{BTreeSet, HashSet};
use std::path::Path;

use anyhow::{Context, Result};
use regex::Regex;
use serde::Deserialize;

use crate::circuit::Circuit;

pub const FILE: &str = "board.toml";
/// Gate names a requirement may be covered by (`gate:<name>`).
pub const GATES: [&str; 6] = ["erc", "drc", "netlist", "netclasses", "routing", "board_toml"];

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoardFile {
    pub board: BoardInfo,
    #[serde(rename = "pin", default)]
    pub pins: Vec<PinMap>,
    pub power: Power,
    #[serde(rename = "requirement", default)]
    pub requirements: Vec<Requirement>,
    #[serde(default)]
    pub firmware: Firmware,
    #[serde(default)]
    pub order: Order,
}

/// How the board would be ordered (D-021 option A: its own JLCPCB order in a shared parcel).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Order {
    /// Bare PCBs made (JLCPCB's smallest batch is 5).
    #[serde(default = "five")]
    pub boards: u64,
    /// Of those, how many JLCPCB assembles (Economic PCBA: 2 to 50).
    #[serde(default = "two")]
    pub assembled: u64,
    /// The owner's budget for this design, USD, before shipping and VAT; None if not set.
    pub budget_usd: Option<f64>,
}

fn five() -> u64 {
    5
}
fn two() -> u64 {
    2
}

impl Default for Order {
    fn default() -> Self {
        Order { boards: 5, assembled: 2, budget_usd: None }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoardInfo {
    /// Must equal `Circuit::name`.
    pub name: String,
    /// Must equal `Circuit::rev`.
    pub revision: String,
    /// Reference of the MCU module whose pins the map names ("U1").
    pub module: String,
}

/// One firmware signal on one module pin.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PinMap {
    /// Firmware name, upper-case C identifier: the header defines `PIN_<signal>`.
    pub signal: String,
    /// The module symbol's pin name ("IO1", "TXD0", "USB_D+").
    pub pin: String,
    /// The circuit net on that pin, as named in the circuit code (no KiCad "/" prefix).
    pub net: String,
    pub gpio: u32,
    pub dir: Dir,
    #[serde(default)]
    pub active_low: bool,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Dir {
    In,
    Out,
    Io,
}

impl Dir {
    fn as_str(self) -> &'static str {
        match self {
            Dir::In => "in",
            Dir::Out => "out",
            Dir::Io => "io",
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Power {
    pub source: String,
    pub budget_ma: f64,
    #[serde(rename = "load", default)]
    pub loads: Vec<Load>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Load {
    pub name: String,
    pub ma: f64,
    /// Where the number comes from; say INFERRED if it is a guess.
    pub source: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Requirement {
    /// "R1", "R2", ...: the same IDs as in `spec.md`.
    pub id: String,
    pub text: String,
    /// What shows the requirement is met: `part:<ref>`, `pin:<signal>`, `test:<self-test>`
    /// or `gate:<gate>` (one of `GATES`).
    pub covered_by: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Firmware {
    /// Self-tests the firmware runs at bring-up (lower-case identifiers).
    #[serde(default)]
    pub self_test: Vec<String>,
}

/// The board's `board.toml`, or None if it has none (boards from before D-023).
pub fn load(dir: &Path) -> Result<Option<BoardFile>> {
    let path = dir.join(FILE);
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    parse(&text).with_context(|| format!("in {}", path.display())).map(Some)
}

pub fn parse(text: &str) -> Result<BoardFile> {
    Ok(toml::from_str(text)?)
}

/// The GPIO number behind a module pin name, for the modules pcbgen knows.
/// ESP32-S3-WROOM-1: `IOnn` is GPIOnn; TXD0/RXD0 are GPIO43/44 (Espressif WROOM-1
/// datasheet v1.8, Table 3-1); USB_D-/USB_D+ are GPIO19/20 (the KiCad symbol's alternate
/// names IO19/IO20). Other pins (EN, 3V3, GND) are not GPIOs.
fn gpio_of(lib_id: &str, pin: &str) -> Option<u32> {
    debug_assert!(knows(lib_id));
    match pin {
        "TXD0" => Some(43),
        "RXD0" => Some(44),
        "USB_D-" => Some(19),
        "USB_D+" => Some(20),
        _ => pin.strip_prefix("IO").filter(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())).and_then(|n| n.parse().ok()),
    }
}

fn knows(lib_id: &str) -> bool {
    lib_id.starts_with("RF_Module:ESP32-S3-WROOM-1")
}

/// Problems between `board.toml`, the circuit and the spec's requirement IDs
/// (None: the board has no `spec.md`). Empty means the gate passes.
pub fn problems(bf: &BoardFile, c: &Circuit, spec_ids: Option<&BTreeSet<String>>) -> Vec<String> {
    let mut p = vec![];
    let b = &bf.board;
    if b.name != c.name {
        p.push(format!("board.name {:?} is not the circuit's name {:?}", b.name, c.name));
    }
    if b.revision != c.rev {
        p.push(format!("board.revision {:?} is not the circuit's revision {:?}", b.revision, c.rev));
    }

    // --- pin map --------------------------------------------------------------
    let module = c.find_part(&b.module).map(|id| (id.0, &c.parts[id.0]));
    match module {
        None => p.push(format!("board.module {:?} is not a part in the circuit", b.module)),
        Some((_, m)) if !knows(&m.lib_id) => {
            p.push(format!("module {} ({}) has no GPIO table in pcbgen's boardfile.rs; add one", m.reference, m.lib_id))
        }
        _ => {}
    }
    let ident = Regex::new(r"^[A-Z][A-Z0-9_]*$").unwrap();
    let (mut signals, mut gpios, mut pads) = (HashSet::new(), HashSet::new(), HashSet::new());
    for m in &bf.pins {
        let at = format!("pin {}", m.signal);
        if !ident.is_match(&m.signal) {
            p.push(format!("{at}: signal must be an upper-case C identifier (A-Z, 0-9, _)"));
        }
        if !signals.insert(m.signal.as_str()) {
            p.push(format!("{at}: signal listed twice"));
        }
        if !gpios.insert(m.gpio) {
            p.push(format!("{at}: GPIO {} used by another signal", m.gpio));
        }
        if !pads.insert(m.pin.as_str()) {
            p.push(format!("{at}: module pin {} used by another signal", m.pin));
        }
        let Some((mi, part)) = module.filter(|(_, part)| knows(&part.lib_id)) else { continue };
        let Some(pin) = part.symbol.pins.iter().find(|x| x.name == m.pin) else {
            p.push(format!("{at}: {} ({}) has no pin named {:?}", part.reference, part.lib_id, m.pin));
            continue;
        };
        match gpio_of(&part.lib_id, &m.pin) {
            Some(g) if g != m.gpio => p.push(format!("{at}: {} is GPIO {g}, not {}", m.pin, m.gpio)),
            None => p.push(format!("{at}: {} is not a GPIO", m.pin)),
            _ => {}
        }
        match c.net_of(mi, pin) {
            Some(n) if n.name == m.net => {}
            Some(n) => p.push(format!("{at}: {} is on net {}, not {}", m.pin, n.name, m.net)),
            None => p.push(format!("{at}: {} is on no net (net {} expected)", m.pin, m.net)),
        }
    }
    // the other way round: every GPIO the circuit uses must be in the map
    if let Some((mi, part)) = module.filter(|(_, part)| knows(&part.lib_id)) {
        let mapped: HashSet<&str> = bf.pins.iter().map(|m| m.pin.as_str()).collect();
        let mut seen = HashSet::new();
        for pin in &part.symbol.pins {
            if gpio_of(&part.lib_id, &pin.name).is_none() || mapped.contains(pin.name.as_str()) || !seen.insert(&pin.name) {
                continue;
            }
            if let Some(n) = c.net_of(mi, pin) {
                p.push(format!("{}.{} ({}) is on net {} but not in the pin map", part.reference, pin.number, pin.name, n.name));
            }
        }
    }

    // --- power ----------------------------------------------------------------
    for l in &bf.power.loads {
        if !(l.ma.is_finite() && l.ma >= 0.0) {
            p.push(format!("power load {:?}: ma must be 0 or more", l.name));
        }
    }
    let total: f64 = bf.power.loads.iter().map(|l| l.ma).sum();
    if !(bf.power.budget_ma.is_finite() && bf.power.budget_ma > 0.0) {
        p.push("power.budget_ma must be more than 0".into());
    } else if total > bf.power.budget_ma {
        p.push(format!("power: loads total {total} mA, over the {} mA budget", bf.power.budget_ma));
    }

    // --- order ------------------------------------------------------------------
    let o = &bf.order;
    if !(2..=50).contains(&o.assembled) || o.assembled > o.boards {
        p.push(format!("order: {} assembled of {} boards; JLCPCB Economic assembles 2 to 50, at most the boards made", o.assembled, o.boards));
    }
    if o.budget_usd.is_some_and(|b| !(b.is_finite() && b > 0.0)) {
        p.push("order.budget_usd must be more than 0".into());
    }

    // --- firmware self-tests ---------------------------------------------------
    let test_ident = Regex::new(r"^[a-z][a-z0-9_]*$").unwrap();
    let mut tests = HashSet::new();
    for t in &bf.firmware.self_test {
        if !test_ident.is_match(t) {
            p.push(format!("firmware.self_test {t:?}: must be a lower-case identifier (a-z, 0-9, _)"));
        }
        if !tests.insert(t.as_str()) {
            p.push(format!("firmware.self_test {t:?} listed twice"));
        }
    }

    // --- requirements ------------------------------------------------------------
    let rid = Regex::new(r"^R[1-9][0-9]*$").unwrap();
    let mut ids = BTreeSet::new();
    for r in &bf.requirements {
        let at = format!("requirement {}", r.id);
        if !rid.is_match(&r.id) {
            p.push(format!("{at}: id must look like R1, R2, ..."));
        }
        if !ids.insert(r.id.clone()) {
            p.push(format!("{at}: id listed twice"));
        }
        if r.text.trim().is_empty() {
            p.push(format!("{at}: text is empty"));
        }
        if r.covered_by.is_empty() {
            p.push(format!("{at}: nothing covers it (covered_by is empty)"));
        }
        for cov in &r.covered_by {
            let ok = match cov.split_once(':') {
                Some(("part", x)) => c.find_part(x).is_some(),
                Some(("pin", x)) => signals.contains(x),
                Some(("test", x)) => tests.contains(x),
                Some(("gate", x)) => GATES.contains(&x),
                _ => {
                    p.push(format!("{at}: {cov:?} must start with part:, pin:, test: or gate:"));
                    continue;
                }
            };
            if !ok {
                let what = match cov.split_once(':').unwrap().0 {
                    "part" => "no such part in the circuit".to_string(),
                    "pin" => "no such signal in the pin map".to_string(),
                    "test" => "not in firmware.self_test".to_string(),
                    _ => format!("gates are {}", GATES.join(", ")),
                };
                p.push(format!("{at}: {cov:?}: {what}"));
            }
        }
    }
    for t in &bf.firmware.self_test {
        if !bf.requirements.iter().any(|r| r.covered_by.iter().any(|c| c == &format!("test:{t}"))) {
            p.push(format!("firmware.self_test {t:?} covers no requirement"));
        }
    }
    match spec_ids {
        None => p.push("the board has no spec.md (copy templates/spec.md)".into()),
        Some(spec) => {
            for id in spec.difference(&ids) {
                p.push(format!("spec.md requirement {id} is not in board.toml"));
            }
            for id in ids.difference(spec) {
                p.push(format!("board.toml requirement {id} is not in spec.md"));
            }
        }
    }
    p
}

/// Requirement IDs in a `spec.md`: list items starting with a bold ID, `- **R1** ...`.
pub fn spec_ids(spec: &str) -> BTreeSet<String> {
    let re = Regex::new(r"(?m)^\s*[-*]\s+\*\*(R[0-9]+)\*\*").unwrap();
    re.captures_iter(spec).map(|m| m[1].to_string()).collect()
}

/// The `BOARD.TOML` gate: prints the power margin and every problem.
/// `dir` is the board's directory (for its `spec.md`).
pub fn gate(bf: &BoardFile, c: &Circuit, dir: &Path) -> Result<bool> {
    let spec = dir.join("spec.md");
    let ids = if spec.exists() { Some(spec_ids(&std::fs::read_to_string(&spec)?)) } else { None };
    let total: f64 = bf.power.loads.iter().map(|l| l.ma).sum();
    println!(
        "== BOARD.TOML: {} pins, {} requirements, {} self-tests; power {total:.1} of {} mA ({:.1} mA margin)",
        bf.pins.len(),
        bf.requirements.len(),
        bf.firmware.self_test.len(),
        bf.power.budget_ma,
        bf.power.budget_ma - total
    );
    let p = problems(bf, c, ids.as_ref());
    for line in &p {
        println!("   OPEN:   {line}");
    }
    println!("== BOARD.TOML: {}", if p.is_empty() { "PASS" } else { "FAIL" });
    Ok(p.is_empty())
}

/// The firmware's `board_pins.h` (ESP-IDF). Deterministic: the same file gives the same text.
pub fn header(bf: &BoardFile) -> String {
    let one_line = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut h = String::new();
    h += "// Generated by pcbgen from board.toml; do not edit. Regenerate with the `fw` stage.\n";
    h += "#pragma once\n\n#include \"driver/gpio.h\"\n\n";
    h += &format!("#define BOARD_NAME \"{}\"\n#define BOARD_REVISION \"{}\"\n", bf.board.name, bf.board.revision);
    for m in &bf.pins {
        let mut c = format!("{}: module pin {}, net {}, {}", m.signal, m.pin, m.net, m.dir.as_str());
        if m.active_low {
            c += ", active low";
        }
        if !m.note.is_empty() {
            c += &format!("; {}", one_line(&m.note));
        }
        h += &format!("\n// {c}\n#define PIN_{} GPIO_NUM_{}\n", m.signal, m.gpio);
        if m.active_low {
            h += &format!("#define PIN_{}_ACTIVE_LOW 1\n", m.signal);
        }
    }
    let tests: Vec<String> = bf.firmware.self_test.iter().map(|t| format!("\"{t}\"")).collect();
    h += "\n// Self-tests the firmware runs at bring-up: static const char *t[] = BOARD_SELF_TESTS;\n";
    h += &format!("#define BOARD_SELF_TEST_COUNT {}\n#define BOARD_SELF_TESTS {{{}}}\n", tests.len(), tests.join(", "));
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
[board]
name = "t"
revision = "A"
module = "U1"

[[pin]]
signal = "I2C_SDA"
pin = "IO1"
net = "SDA"
gpio = 1
dir = "io"

[[pin]]
signal = "BOOT"
pin = "IO0"
net = "BOOT"
gpio = 0
dir = "in"
active_low = true
note = "strapping pin"

[[pin]]
signal = "USB_DP"
pin = "USB_D+"
net = "DP"
gpio = 20
dir = "io"

[power]
source = "USB"
budget_ma = 500

[[power.load]]
name = "module"
ma = 355
source = "datasheet"

[[requirement]]
id = "R1"
text = "Reads the sensor"
covered_by = ["part:R1", "pin:I2C_SDA", "test:sensor", "gate:drc"]

[firmware]
self_test = ["sensor"]
"#;

    /// ESP32-S3 module on SDA/BOOT/DP, plus a resistor so SDA has two pins.
    fn circuit() -> Circuit {
        let mut c = Circuit::new("t", "test", "A");
        let (sda, boot, dp, gnd) = (c.net("SDA"), c.net("BOOT"), c.net("DP"), c.net("GND"));
        let u1 = c.part("U1", "RF_Module:ESP32-S3-WROOM-1", "m", "fp").id();
        c.connect(sda, u1, &["IO1"]);
        c.connect(boot, u1, &["IO0"]);
        c.connect(dp, u1, &["USB_D+"]);
        c.connect(gnd, u1, &["GND"]);
        let r1 = c.part("R1", "Device:R", "1k", "fp").id();
        c.connect(sda, r1, &["1"]);
        c.connect(gnd, r1, &["2"]);
        c
    }

    fn ids(v: &[&str]) -> BTreeSet<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    fn run(toml: &str, c: &Circuit) -> Vec<String> {
        problems(&parse(toml).unwrap(), c, Some(&ids(&["R1"])))
    }

    /// The single problem a one-line change to SAMPLE causes, checked by substring.
    fn one(from: &str, to: &str, want: &str) {
        assert!(SAMPLE.contains(from), "{from:?} not in SAMPLE");
        let p = run(&SAMPLE.replacen(from, to, 1), &circuit());
        assert!(p.len() == 1 && p[0].contains(want), "{from:?} -> {to:?}: want {want:?}, got {p:?}");
    }

    #[test]
    fn sample_parses_and_passes() {
        let bf = parse(SAMPLE).unwrap();
        assert_eq!(bf.pins.len(), 3);
        assert_eq!(bf.pins[1].dir, Dir::In);
        assert!(bf.pins[1].active_low && !bf.pins[0].active_low);
        assert_eq!(bf.power.loads[0].ma, 355.0);
        assert_eq!(run(SAMPLE, &circuit()), Vec::<String>::new());
    }

    #[test]
    fn unknown_keys_are_errors() {
        assert!(parse(&SAMPLE.replace("gpio = 1\n", "gpio = 1\ngpoi = 1\n")).is_err());
        assert!(parse(&SAMPLE.replace("[firmware]", "[firmwear]")).is_err());
        assert!(parse(&SAMPLE.replace("dir = \"io\"", "dir = \"inout\"")).is_err());
    }

    #[test]
    fn gpio_table() {
        let m = "RF_Module:ESP32-S3-WROOM-1";
        assert_eq!(gpio_of(m, "IO48"), Some(48));
        assert_eq!(gpio_of(m, "TXD0"), Some(43));
        assert_eq!(gpio_of(m, "RXD0"), Some(44));
        assert_eq!(gpio_of(m, "USB_D-"), Some(19));
        assert_eq!(gpio_of(m, "USB_D+"), Some(20));
        for not in ["EN", "3V3", "GND", "IO", "IOx1"] {
            assert_eq!(gpio_of(m, not), None, "{not}");
        }
    }

    #[test]
    fn each_error() {
        one("name = \"t\"", "name = \"u\"", "not the circuit's name");
        one("revision = \"A\"", "revision = \"B\"", "not the circuit's revision");
        one("gpio = 1\n", "gpio = 2\n", "IO1 is GPIO 1, not 2");
        one("net = \"SDA\"", "net = \"SCL\"", "on net SDA, not SCL");
        one("signal = \"BOOT\"", "signal = \"Boot\"", "upper-case C identifier");
        one("budget_ma = 500", "budget_ma = 300", "over the 300 mA budget");
        one("[firmware]", "[order]\nassembled = 6\n\n[firmware]", "6 assembled of 5 boards");
        one("\"gate:drc\"", "\"gate:lint\"", "gates are");
        one("\"part:R1\"", "\"part:R9\"", "no such part");
        one("\"pin:I2C_SDA\"", "\"pin:I2C_SCL\"", "no such signal");
        one("covered_by = [\"part:R1\", \"pin:I2C_SDA\", \"test:sensor\", \"gate:drc\"]", "covered_by = [\"test:sensor\", \"drc\"]", "must start with");
        one("self_test = [\"sensor\"]", "self_test = [\"sensor\", \"led\"]", "\"led\" covers no requirement");
        // GPIO 0 twice, and IO0 then no longer in the map
        let p = run(&SAMPLE.replacen("pin = \"IO1\"\nnet = \"SDA\"\ngpio = 1", "pin = \"IO0\"\nnet = \"BOOT\"\ngpio = 0", 1), &circuit());
        assert!(p.iter().any(|x| x.contains("GPIO 0 used by another")), "{p:?}");
        assert!(p.iter().any(|x| x.contains("IO1) is on net SDA but not in the pin map")), "{p:?}");
    }

    #[test]
    fn pins_the_module_lacks_or_that_are_not_gpios() {
        let p = run(&SAMPLE.replacen("pin = \"IO1\"", "pin = \"IO99\"", 1), &circuit());
        assert!(p.iter().any(|x| x.contains("has no pin named \"IO99\"")), "{p:?}");
        let p = run(&SAMPLE.replacen("pin = \"IO1\"", "pin = \"EN\"", 1), &circuit());
        assert!(p.iter().any(|x| x.contains("EN is not a GPIO")), "{p:?}");
        let mut c = circuit();
        let u1 = c.find_part("U1").unwrap();
        let x = c.net("X");
        c.connect(x, u1, &["IO48"]);
        let p = run(SAMPLE, &c);
        assert!(p.len() == 1 && p[0].contains("IO48) is on net X but not in the pin map"), "{p:?}");
    }

    #[test]
    fn module_must_exist_and_be_known() {
        one("module = \"U1\"", "module = \"U7\"", "not a part in the circuit");
        let p = run(&SAMPLE.replace("module = \"U1\"", "module = \"R1\""), &circuit());
        assert!(p.len() == 1 && p[0].contains("has no GPIO table"), "{p:?}");
    }

    #[test]
    fn spec_ids_must_match() {
        let bf = parse(SAMPLE).unwrap();
        let p = problems(&bf, &circuit(), Some(&ids(&["R1", "R2"])));
        assert_eq!(p, vec!["spec.md requirement R2 is not in board.toml"]);
        let p = problems(&bf, &circuit(), Some(&ids(&[])));
        assert_eq!(p, vec!["board.toml requirement R1 is not in spec.md"]);
        assert!(problems(&bf, &circuit(), None)[0].contains("no spec.md"));
        let spec = "# x\n- **R1** one\n  - **R2** nested\n* **R10** star\nnot **R3** a list item\n- R4 not bold\n";
        assert_eq!(spec_ids(spec), ids(&["R1", "R10", "R2"]));
    }

    #[test]
    fn header_text() {
        let h = header(&parse(SAMPLE).unwrap());
        assert!(h.starts_with("// Generated by pcbgen"));
        for want in [
            "#pragma once\n",
            "#include \"driver/gpio.h\"\n",
            "#define BOARD_NAME \"t\"\n#define BOARD_REVISION \"A\"\n",
            "// I2C_SDA: module pin IO1, net SDA, io\n#define PIN_I2C_SDA GPIO_NUM_1\n",
            "// BOOT: module pin IO0, net BOOT, in, active low; strapping pin\n#define PIN_BOOT GPIO_NUM_0\n#define PIN_BOOT_ACTIVE_LOW 1\n",
            "#define PIN_USB_DP GPIO_NUM_20\n",
            "#define BOARD_SELF_TEST_COUNT 1\n#define BOARD_SELF_TESTS {\"sensor\"}\n",
        ] {
            assert!(h.contains(want), "missing {want:?} in\n{h}");
        }
        assert!(!h.contains("PIN_I2C_SDA_ACTIVE_LOW"));
    }
}
