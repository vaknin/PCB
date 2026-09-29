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
//! [[sim.wokwi_step]] wait = "SELFTEST_PRESS boot_button"; press = "BOOT"
//! [provision]      gemini_api_key = "file:~/.config/capture-notes/config#gemini_api_key"
//! [case]           material = "resin"; wall = 1.5; screw = "M3"
//! [[case.opening]] ref = "J1"; kind = "usb_c"
//! ```
//!
//! `gate` checks it against the circuit and the board's `spec.md`; `header` turns it into
//! the firmware's `board_pins.h`. Unknown keys are errors, so a typo can't pass silently.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::Path;

use anyhow::{Context, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};

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
    #[serde(default)]
    pub sim: Sim,
    /// NVS key -> where `devctl provision` gets its value: `file:<path>#<field>` (a
    /// `field = value` line in that file) or `prompt`. Only references; values never enter
    /// the repo (D-025).
    #[serde(default)]
    pub provision: BTreeMap<String, String>,
    /// The printed case (D-025 Phase B), built and fit-checked by the `case` stage; None: no case.
    pub case: Option<Case>,
}

/// `[case]`: a two-part printed shell (bottom tray + lid) around the board, made by
/// `enclosure/case.py` from `case/board.json` (the `case` stage, `crate::case`). mm throughout.
/// Printing rules are JLC3DP's design guide (research/2026-09-29-enclosure-tooling.md §3).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Case {
    /// Sets the minimum wall, hole and skin (`Material::limits`).
    pub material: Material,
    /// Side wall.
    #[serde(default = "d1_5")]
    pub wall: f64,
    /// Floor of the tray and top plate of the lid.
    #[serde(default = "d1_5")]
    pub floor: f64,
    /// PCB edge to the inner wall (static fit 0.2 + print tolerance).
    #[serde(default = "d0_3")]
    pub edge_gap: f64,
    /// Tallest top-side part to the lid's underside.
    #[serde(default = "d1_0")]
    pub top_gap: f64,
    /// PCB underside to the floor: the standoff height.
    #[serde(default = "d3_0")]
    pub bottom_gap: f64,
    /// Fit gate: every case solid to every part solid, except intended contacts.
    #[serde(default = "d0_2")]
    pub min_clearance: f64,
    /// Self-tapping screw for plastic: from below through a standoff and the PCB's mounting
    /// hole into a boss in the lid.
    #[serde(default)]
    pub screw: Screw,
    #[serde(rename = "opening", default)]
    pub openings: Vec<Opening>,
}

fn d0_2() -> f64 {
    0.2
}
fn d0_3() -> f64 {
    0.3
}
fn d1_0() -> f64 {
    1.0
}
fn d1_5() -> f64 {
    1.5
}
fn d3_0() -> f64 {
    3.0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Material {
    /// SLA, JLC3DP 9600 white resin: cheapest, finest; brittle (no snap arms), translucent.
    Resin,
    /// MJF PA12 nylon: tough, black or grey.
    Nylon,
}

/// A material's printing limits, mm (JLC3DP design guide, VERIFIED 2026-09-29, unless noted).
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Limits {
    /// Side wall and floor: resin 1.2 (the upload checker's "wall thickness > 1.2 mm"),
    /// nylon 1.5 (the guide's 50 × 50 mm row).
    pub wall_min: f64,
    /// Smallest hole: SLA Ø1.0, others Ø1.5.
    pub hole_min: f64,
    /// Bosses, snaps and fasteners: "more than 1.5 mm".
    pub boss_wall_min: f64,
    /// Thinnest local skin (LED window, the wall left at the USB-C recess): resin 0.8 (the
    /// upload checker's "thinnest part ≥ 0.8 mm"), nylon 1.0 (PA12-HP's datasheet wall).
    pub skin_min: f64,
    /// Static fit clearance per side (resin 0.2; nylon 0.2-0.4).
    pub static_fit: f64,
    /// Moving fit clearance per side (resin 0.5, nylon 0.6): the button cap in its hole.
    pub moving_fit: f64,
    /// Smallest printable part in each axis (SLA/MJF 5 × 5 × 5).
    pub part_min: f64,
}

impl Material {
    pub fn limits(self) -> Limits {
        match self {
            Material::Resin => Limits { wall_min: 1.2, hole_min: 1.0, boss_wall_min: 1.5, skin_min: 0.8, static_fit: 0.2, moving_fit: 0.5, part_min: 5.0 },
            Material::Nylon => Limits { wall_min: 1.5, hole_min: 1.5, boss_wall_min: 1.5, skin_min: 1.0, static_fit: 0.2, moving_fit: 0.6, part_min: 5.0 },
        }
    }
}

/// Self-tapping (thread-forming) screws for plastic, by nominal size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
pub enum Screw {
    M2,
    #[serde(rename = "M2.5")]
    M2_5,
    #[default]
    M3,
}

/// A screw's holes, mm. Pilot ≈ 0.8-0.85 × d for thread-forming screws into plastic and pan
/// head sizes from ISO 7045 (INFERRED: common vendor tables, not checked against the screws
/// that will be bought).
#[derive(Debug, Clone, Copy, Serialize)]
pub struct ScrewHoles {
    pub size: Screw,
    pub d: f64,
    /// Hole in the lid boss the screw cuts its thread into.
    pub pilot: f64,
    /// Hole the screw passes through (standoff).
    pub clearance: f64,
    pub head_d: f64,
    pub head_h: f64,
}

impl Screw {
    pub fn holes(self) -> ScrewHoles {
        let (d, pilot, clearance, head_d, head_h) = match self {
            Screw::M2 => (2.0, 1.7, 2.4, 4.0, 1.6),
            Screw::M2_5 => (2.5, 2.1, 2.9, 5.0, 2.0),
            Screw::M3 => (3.0, 2.5, 3.4, 5.6, 2.4),
        };
        ScrewHoles { size: self, d, pilot, clearance, head_d, head_h }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OpeningKind {
    /// Wall cutout around a USB-C receptacle's mouth, plus an outer recess for the plug.
    UsbC,
    /// Wall cutout around a connector's mouth (JST, Qwiic).
    Connector,
    /// Lid hole over a tactile switch, with a separate printed cap (plunger).
    Button,
    /// Small lid hole over a switch (reset), with a guide tube for a paper clip.
    Pinhole,
    /// A LED seen through the lid: a thin window (resin) or a hole.
    Led,
    /// Slots in the lid over a part that must see outside air (humidity sensor).
    Vent,
}

impl OpeningKind {
    pub fn as_str(self) -> &'static str {
        match self {
            OpeningKind::UsbC => "usb_c",
            OpeningKind::Connector => "connector",
            OpeningKind::Button => "button",
            OpeningKind::Pinhole => "pinhole",
            OpeningKind::Led => "led",
            OpeningKind::Vent => "vent",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LedStyle {
    /// A pocket from inside, leaving the material's thinnest skin (resin glows through it).
    Window,
    /// A hole through the lid.
    Hole,
}

/// `[[case.opening]]`: what the case must let through for one part. Only `ref` and `kind` are
/// needed; the rest override the kind's defaults (`Opening::resolved`).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Opening {
    #[serde(rename = "ref")]
    pub reference: String,
    pub kind: OpeningKind,
    /// usb_c, connector: gap around the mouth, per side (0.3).
    pub margin: Option<f64>,
    /// usb_c, connector: the mouth's height above the PCB (usb_c 3.26, INFERRED from the HRO
    /// model; a connector must give its datasheet value).
    pub height: Option<f64>,
    /// button: the cap's diameter (4.0); pinhole: the hole (1.4); led: the window pocket (3.0)
    /// or the hole (2.0).
    pub diameter: Option<f64>,
    /// led: window (default) or hole.
    pub style: Option<LedStyle>,
    /// button: the switch's travel (0.25, INFERRED: typical for small tactile switches).
    pub travel: Option<f64>,
    /// vent: number of slots (3), their width (1.2) and length (5.0).
    pub slots: Option<u32>,
    pub slot_width: Option<f64>,
    pub slot_length: Option<f64>,
}

/// An opening with every value its kind uses filled in (written to `case/board.json`).
#[derive(Debug, Clone, Serialize)]
pub struct ResolvedOpening {
    #[serde(rename = "ref")]
    pub reference: String,
    pub kind: OpeningKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub margin: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diameter: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<LedStyle>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub travel: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slots: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slot_width: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slot_length: Option<f64>,
}

impl Opening {
    /// The keys (besides ref and kind) this kind uses.
    fn keys(kind: OpeningKind) -> &'static [&'static str] {
        match kind {
            OpeningKind::UsbC | OpeningKind::Connector => &["margin", "height"],
            OpeningKind::Button => &["diameter", "travel"],
            OpeningKind::Pinhole => &["diameter"],
            OpeningKind::Led => &["style", "diameter"],
            OpeningKind::Vent => &["slots", "slot_width", "slot_length"],
        }
    }

    /// The keys given in board.toml.
    fn given(&self) -> Vec<&'static str> {
        let o = self;
        [
            ("margin", o.margin.is_some()),
            ("height", o.height.is_some()),
            ("diameter", o.diameter.is_some()),
            ("style", o.style.is_some()),
            ("travel", o.travel.is_some()),
            ("slots", o.slots.is_some()),
            ("slot_width", o.slot_width.is_some()),
            ("slot_length", o.slot_length.is_some()),
        ]
        .into_iter()
        .filter(|x| x.1)
        .map(|x| x.0)
        .collect()
    }

    pub fn resolved(&self) -> ResolvedOpening {
        use OpeningKind::*;
        let k = self.kind;
        let style = (k == Led).then(|| self.style.unwrap_or(LedStyle::Window));
        let diameter = match k {
            Button => Some(self.diameter.unwrap_or(4.0)),
            Pinhole => Some(self.diameter.unwrap_or(1.4)),
            Led => Some(self.diameter.unwrap_or(if style == Some(LedStyle::Hole) { 2.0 } else { 3.0 })),
            _ => None,
        };
        ResolvedOpening {
            reference: self.reference.clone(),
            kind: k,
            margin: matches!(k, UsbC | Connector).then(|| self.margin.unwrap_or(0.3)),
            height: match k {
                UsbC => Some(self.height.unwrap_or(3.26)),
                Connector => self.height,
                _ => None,
            },
            diameter,
            style,
            travel: (k == Button).then(|| self.travel.unwrap_or(0.25)),
            slots: (k == Vent).then(|| self.slots.unwrap_or(3)),
            slot_width: (k == Vent).then(|| self.slot_width.unwrap_or(1.2)),
            slot_length: (k == Vent).then(|| self.slot_length.unwrap_or(5.0)),
        }
    }
}

impl Case {
    /// The table as `case/board.json` carries it: defaults filled, the material's limits and
    /// the screw's holes spelled out, so `enclosure/case.py` has one source of numbers.
    pub fn resolved(&self) -> serde_json::Value {
        serde_json::json!({
            "material": self.material,
            "limits": self.material.limits(),
            "wall": self.wall,
            "floor": self.floor,
            "edge_gap": self.edge_gap,
            "top_gap": self.top_gap,
            "bottom_gap": self.bottom_gap,
            "min_clearance": self.min_clearance,
            "screw": self.screw.holes(),
            "openings": self.openings.iter().map(Opening::resolved).collect::<Vec<_>>(),
        })
    }
}

/// Simulation settings (D-025). QEMU needs none; Wokwi needs the parts on the pins (`PinMap::sim`)
/// and the steps that drive them during the self-test run.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sim {
    #[serde(rename = "wokwi_step", default)]
    pub wokwi_steps: Vec<WokwiStep>,
}

/// One step of the Wokwi self-test run: wait for a console line, then press a button or check
/// a pin. The run always ends by waiting for `SELFTEST_DONE`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WokwiStep {
    /// Console text to wait for first.
    pub wait: String,
    /// Press the button on this signal (its pin has `sim = "button"`)...
    pub press: Option<String>,
    /// ...for this long (200 ms if not given; 1000 or more is a hold).
    pub hold_ms: Option<u64>,
    /// ...or check this signal's pin (a LED) is at `level`.
    pub expect: Option<String>,
    pub level: Option<u8>,
}

/// The Wokwi part that stands in for a pin in simulation (D-025).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SimPart {
    /// Pushbutton to GND with a 10k pull-up (active low).
    Button,
    /// LED from the pin through 1k to GND (active high).
    Led,
    /// One cathode of a common-anode RGB LED on 3V3 (active low).
    LedR,
    LedG,
    LedB,
    /// Potentiometer wiper (an analog input such as a battery divider).
    Pot,
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
    /// The Wokwi part standing in for this pin, if any.
    pub sim: Option<SimPart>,
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
    p.extend(sim_problems(bf));
    if let Some(case) = &bf.case {
        p.extend(case_problems(case, c));
    }
    let key =Regex::new(r"^[a-z0-9_]{1,15}$").unwrap();
    for (k, v) in &bf.provision {
        if !key.is_match(k) {
            p.push(format!("provision key {k:?}: NVS keys are [a-z0-9_], at most 15 characters"));
        }
        let ok = v == "prompt" || v.strip_prefix("file:").and_then(|f| f.split_once('#')).is_some_and(|(f, field)| !f.is_empty() && !field.is_empty());
        if !ok {
            p.push(format!("provision {k}: {v:?} must be \"prompt\" or \"file:<path>#<field>\""));
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

fn sim_problems(bf: &BoardFile) -> Vec<String> {
    let mut p = vec![];
    let mut seen = HashSet::new();
    for pin in &bf.pins {
        let Some(part) = pin.sim else { continue };
        let at = format!("pin {} sim {part:?}", pin.signal);
        let want_in = matches!(part, SimPart::Button | SimPart::Pot);
        if want_in && pin.dir == Dir::Out || !want_in && pin.dir == Dir::In {
            p.push(format!("{at}: doesn't fit dir {:?}", pin.dir.as_str()));
        }
        if matches!(part, SimPart::LedR | SimPart::LedG | SimPart::LedB) && !seen.insert(part) {
            p.push(format!("{at}: the RGB LED has one {part:?}"));
        }
        if matches!(pin.gpio, 19 | 20 | 43 | 44) {
            p.push(format!("{at}: GPIO {} is USB or the UART console", pin.gpio));
        }
    }
    let part_of = |sig: &str| bf.pins.iter().find(|x| x.signal == sig).and_then(|x| x.sim);
    for (i, st) in bf.sim.wokwi_steps.iter().enumerate() {
        let at = format!("sim.wokwi_step {}", i + 1);
        if st.wait.trim().is_empty() {
            p.push(format!("{at}: wait is empty"));
        }
        match (&st.press, &st.expect) {
            (Some(sig), None) => {
                if part_of(sig) != Some(SimPart::Button) {
                    p.push(format!("{at}: press {sig:?} needs a pin with sim = \"button\""));
                }
                if st.level.is_some() {
                    p.push(format!("{at}: level goes with expect, not press"));
                }
            }
            (None, Some(sig)) => {
                if !matches!(part_of(sig), Some(SimPart::Led | SimPart::LedR | SimPart::LedG | SimPart::LedB)) {
                    p.push(format!("{at}: expect {sig:?} needs a pin with a LED sim part"));
                }
                if !matches!(st.level, Some(0 | 1)) {
                    p.push(format!("{at}: expect needs level = 0 or 1"));
                }
                if st.hold_ms.is_some() {
                    p.push(format!("{at}: hold_ms goes with press, not expect"));
                }
            }
            _ => p.push(format!("{at}: needs exactly one of press or expect")),
        }
    }
    p
}

/// `[case]` against the material's printing limits and the circuit's parts. What only the 3D
/// shapes can show (clearances, alignment) is the `case` stage's fit gate.
fn case_problems(case: &Case, c: &Circuit) -> Vec<String> {
    let mut p = vec![];
    let lim = case.material.limits();
    let m = match case.material {
        Material::Resin => "resin",
        Material::Nylon => "nylon",
    };
    let num = |v: f64| v.is_finite() && v >= 0.0;
    for (key, v, min) in [
        ("wall", case.wall, lim.wall_min),
        ("floor", case.floor, lim.wall_min),
        ("edge_gap", case.edge_gap, lim.static_fit),
        ("min_clearance", case.min_clearance, 0.0),
        ("top_gap", case.top_gap, case.min_clearance),
        ("bottom_gap", case.bottom_gap, case.min_clearance),
    ] {
        if !num(v) || v < min {
            p.push(format!("case.{key} = {v} mm is under the minimum {min} mm ({m})"));
        }
    }
    let mut seen = HashSet::new();
    for o in &case.openings {
        let at = format!("case.opening {}", o.reference);
        if c.find_part(&o.reference).is_none() {
            p.push(format!("{at}: no such part in the circuit"));
        }
        if !seen.insert(o.reference.as_str()) {
            p.push(format!("{at}: listed twice"));
        }
        let allowed = Opening::keys(o.kind);
        for k in o.given() {
            if !allowed.contains(&k) {
                p.push(format!("{at}: {k} is not used by a {} opening (it takes {})", o.kind.as_str(), allowed.join(", ")));
            }
        }
        let r = o.resolved();
        if o.kind == OpeningKind::Connector && r.height.is_none() {
            p.push(format!("{at}: a connector needs height (its mouth's height above the PCB, from the datasheet)"));
        }
        for (k, v) in [("margin", r.margin), ("height", r.height), ("diameter", r.diameter), ("travel", r.travel), ("slot_width", r.slot_width), ("slot_length", r.slot_length)] {
            if v.is_some_and(|v| !(v.is_finite() && v > 0.0)) {
                p.push(format!("{at}: {k} must be more than 0"));
            }
        }
        if r.margin.is_some_and(|v| v < lim.static_fit) {
            p.push(format!("{at}: margin {} mm is under the static fit {} mm ({m})", r.margin.unwrap(), lim.static_fit));
        }
        if r.slots == Some(0) {
            p.push(format!("{at}: slots must be at least 1"));
        }
        // holes through the case: the pinhole, a LED hole, the vent slots
        let hole = match (o.kind, r.style) {
            (OpeningKind::Pinhole, _) | (OpeningKind::Led, Some(LedStyle::Hole)) => r.diameter,
            (OpeningKind::Vent, _) => r.slot_width,
            _ => None,
        };
        if let Some(h) = hole.filter(|&h| h < lim.hole_min) {
            p.push(format!("{at}: a {h} mm hole is under the smallest printable hole {} mm ({m})", lim.hole_min));
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
    fn sim_parts_and_wokwi_steps() {
        let button = "active_low = true\nsim = \"button\"\n";
        let steps = "[[sim.wokwi_step]]\nwait = \"SELFTEST_PRESS\"\npress = \"BOOT\"\nhold_ms = 1200\n";
        let ok = SAMPLE.replacen("active_low = true\n", button, 1) + steps;
        assert_eq!(run(&ok, &circuit()), Vec::<String>::new());
        let bf = parse(&ok).unwrap();
        assert_eq!(bf.pins[1].sim, Some(SimPart::Button));
        assert_eq!(bf.sim.wokwi_steps[0].hold_ms, Some(1200));
        assert!(parse(&SAMPLE.replacen("active_low = true\n", "sim = \"buzzer\"\n", 1)).is_err());

        let bad = |from: &str, to: &str, want: &str| {
            let p = run(&ok.replacen(from, to, 1), &circuit());
            assert!(p.iter().any(|x| x.contains(want)), "{from:?} -> {to:?}: want {want:?}, got {p:?}");
        };
        bad("sim = \"button\"", "sim = \"led\"", "doesn't fit dir");
        bad("press = \"BOOT\"", "press = \"I2C_SDA\"", "needs a pin with sim = \"button\"");
        bad("press = \"BOOT\"\nhold_ms = 1200", "expect = \"BOOT\"\nlevel = 1", "needs a pin with a LED sim part");
        bad("press = \"BOOT\"", "press = \"BOOT\"\nexpect = \"BOOT\"", "exactly one of press or expect");
        bad("wait = \"SELFTEST_PRESS\"", "wait = \" \"", "wait is empty");
        // a sim part on the USB pin
        bad("gpio = 20\ndir = \"io\"", "gpio = 20\ndir = \"io\"\nsim = \"led\"", "USB or the UART console");
    }

    #[test]
    fn provision_references() {
        let with = |v: &str| run(&format!("{SAMPLE}\n[provision]\n{v}\n"), &circuit());
        assert_eq!(with("gemini_api_key = \"file:~/.config/capture-notes/config#gemini_api_key\""), Vec::<String>::new());
        assert_eq!(with("wifi_pass = \"prompt\""), Vec::<String>::new());
        assert!(with("wifi_pass = \"hunter2\"")[0].contains("must be \"prompt\""));
        assert!(with("file = \"file:x\"")[0].contains("file:<path>#<field>"));
        assert!(with("a_key_that_is_too_long = \"prompt\"")[0].contains("at most 15"));
    }

    const CASE: &str = r#"
[case]
material = "resin"
wall = 1.5

[[case.opening]]
ref = "U1"
kind = "usb_c"

[[case.opening]]
ref = "R1"
kind = "led"
style = "hole"
"#;

    #[test]
    fn case_parses_resolves_and_passes() {
        let text = format!("{SAMPLE}{CASE}");
        assert_eq!(run(&text, &circuit()), Vec::<String>::new());
        let case = parse(&text).unwrap().case.unwrap();
        assert_eq!((case.floor, case.edge_gap, case.top_gap, case.bottom_gap, case.min_clearance), (1.5, 0.3, 1.0, 3.0, 0.2));
        assert_eq!(case.screw, Screw::M3);
        let r = case.resolved();
        assert_eq!(r["material"], "resin");
        assert_eq!(r["limits"]["hole_min"], 1.0);
        assert_eq!(r["screw"]["size"], "M3");
        assert_eq!(r["screw"]["pilot"], 2.5);
        let usb = &r["openings"][0];
        assert_eq!((usb["ref"].as_str(), usb["kind"].as_str()), (Some("U1"), Some("usb_c")));
        assert_eq!((usb["margin"].as_f64(), usb["height"].as_f64()), (Some(0.3), Some(3.26)));
        assert!(usb.get("diameter").is_none(), "{usb}");
        let led = &r["openings"][1];
        assert_eq!((led["style"].as_str(), led["diameter"].as_f64()), (Some("hole"), Some(2.0)));
        // no [case]: nothing about a case
        assert!(parse(SAMPLE).unwrap().case.is_none());
        // unknown kinds, materials, screws and keys don't parse
        assert!(parse(&text.replace("kind = \"usb_c\"", "kind = \"hdmi\"")).is_err());
        assert!(parse(&text.replace("\"resin\"", "\"wood\"")).is_err());
        assert!(parse(&text.replace("wall = 1.5", "wall = 1.5\nscrew = \"M4\"")).is_err());
        assert!(parse(&text.replace("wall = 1.5", "wall = 1.5\nwal = 1.5")).is_err());
        assert!(parse(&text.replace("style = \"hole\"", "style = \"hole\"\ncolour = \"red\"")).is_err());
        assert_eq!(parse(&text.replace("wall = 1.5", "wall = 1.5\nscrew = \"M2.5\"")).unwrap().case.unwrap().screw, Screw::M2_5);
    }

    #[test]
    fn case_errors() {
        let base = format!("{SAMPLE}{CASE}");
        let bad = |from: &str, to: &str, want: &str| {
            assert!(base.contains(from), "{from:?} not in the case sample");
            let p = run(&base.replacen(from, to, 1), &circuit());
            assert!(p.len() == 1 && p[0].contains(want), "{from:?} -> {to:?}: want {want:?}, got {p:?}");
        };
        bad("wall = 1.5", "wall = 1.0", "case.wall = 1 mm is under the minimum 1.2 mm (resin)");
        bad("material = \"resin\"", "material = \"nylon\"\nfloor = 1.2", "case.floor = 1.2 mm is under the minimum 1.5 mm (nylon)");
        bad("wall = 1.5", "wall = 1.5\nedge_gap = 0.1", "case.edge_gap");
        bad("wall = 1.5", "wall = 1.5\ntop_gap = 0.1", "case.top_gap");
        bad("wall = 1.5", "wall = 1.5\nbottom_gap = -1", "case.bottom_gap");
        bad("ref = \"U1\"", "ref = \"J9\"", "case.opening J9: no such part");
        bad("ref = \"R1\"", "ref = \"U1\"", "case.opening U1: listed twice");
        bad("kind = \"usb_c\"", "kind = \"usb_c\"\ntravel = 0.3", "travel is not used by a usb_c opening (it takes margin, height)");
        bad("kind = \"usb_c\"", "kind = \"connector\"", "a connector needs height");
        bad("kind = \"usb_c\"", "kind = \"usb_c\"\nmargin = 0.1", "margin 0.1 mm is under the static fit 0.2 mm");
        bad("style = \"hole\"", "style = \"hole\"\ndiameter = 0.8", "a 0.8 mm hole is under the smallest printable hole 1 mm (resin)");
        bad("kind = \"led\"\nstyle = \"hole\"", "kind = \"pinhole\"\ndiameter = 0.9", "a 0.9 mm hole");
        bad("kind = \"led\"\nstyle = \"hole\"", "kind = \"vent\"\nslots = 0", "slots must be at least 1");
        bad("kind = \"led\"\nstyle = \"hole\"", "kind = \"vent\"\nslot_width = 0.5", "a 0.5 mm hole");
        bad("kind = \"led\"\nstyle = \"hole\"", "kind = \"button\"\ntravel = 0", "travel must be more than 0");
        // a LED window is a pocket, not a hole: a small one is fine
        assert_eq!(run(&base.replace("style = \"hole\"", "diameter = 0.8"), &circuit()), Vec::<String>::new());
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
