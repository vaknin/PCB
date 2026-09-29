//! Circuit-as-code model: parts, pins, nets.
//!
//! Every pin of every part must end up either on a net or explicitly marked
//! no-connect; `Circuit::check()` enforces that before any file is written.
//! Authoring mistakes (an unknown symbol or pin, a pin on two nets) panic at the line
//! of board code that made them (`#[track_caller]`).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::symlib::{self, Pin, Symbol};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NetId(pub usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PartId(pub usize);

/// One pin of one part: index into `parts` and into that part's `symbol.pins`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PinRef {
    pub part: usize,
    pub pin: usize,
}

pub struct Net {
    pub name: String,
    pub pins: Vec<PinRef>,
}

pub struct Part {
    pub reference: String,
    pub lib_id: String,
    pub value: String,
    pub footprint: String,
    /// Extra symbol fields in order (LCSC, MPN, ...).
    pub fields: Vec<(String, String)>,
    pub block: String,
    /// Schematic rotation, degrees CCW.
    pub rot: i32,
    pub in_bom: bool,
    pub on_board: bool,
    pub dnp: bool,
    pub symbol: Arc<Symbol>,
}

impl Part {
    pub fn field(&self, key: &str) -> Option<&str> {
        self.fields.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }
}

pub struct Circuit {
    pub name: String,
    pub title: String,
    pub rev: String,
    pub company: String,
    pub parts: Vec<Part>,
    pub nets: Vec<Net>,
    pub pwr_flags: Vec<String>,
    pin_net: HashMap<(usize, String), usize>,
    nc: HashSet<(usize, String)>,
}

/// Sets optional properties of a part just added with `Circuit::part`.
pub struct PartBuilder<'a> {
    part: &'a mut Part,
    id: PartId,
}

impl PartBuilder<'_> {
    pub fn lcsc(self, v: &str) -> Self {
        self.field("LCSC", v)
    }
    pub fn mpn(self, v: &str) -> Self {
        self.field("MPN", v)
    }
    pub fn field(self, key: &str, v: &str) -> Self {
        self.part.fields.push((key.into(), v.into()));
        self
    }
    pub fn block(self, v: &str) -> Self {
        self.part.block = v.into();
        self
    }
    pub fn rot(self, deg: i32) -> Self {
        self.part.rot = deg;
        self
    }
    pub fn not_in_bom(self) -> Self {
        self.part.in_bom = false;
        self
    }
    pub fn dnp(self) -> Self {
        self.part.dnp = true;
        self
    }
    pub fn id(self) -> PartId {
        self.id
    }
}

impl Circuit {
    pub fn new(name: &str, title: &str, rev: &str) -> Self {
        Circuit {
            name: name.into(),
            title: title.into(),
            rev: rev.into(),
            company: String::new(),
            parts: vec![],
            nets: vec![],
            pwr_flags: vec![],
            pin_net: HashMap::new(),
            nc: HashSet::new(),
        }
    }

    pub fn net(&mut self, name: &str) -> NetId {
        if let Some(i) = self.nets.iter().position(|n| n.name == name) {
            return NetId(i);
        }
        self.nets.push(Net { name: name.into(), pins: vec![] });
        NetId(self.nets.len() - 1)
    }

    /// Add a part. The symbol is loaded now, so a bad lib id fails at this line.
    #[track_caller]
    pub fn part(&mut self, reference: &str, lib_id: &str, value: &str, footprint: &str) -> PartBuilder<'_> {
        if self.parts.iter().any(|p| p.reference == reference) {
            panic!("duplicate reference {reference}");
        }
        let symbol = symlib::load(lib_id).unwrap_or_else(|e| panic!("{reference}: {e:#}"));
        self.parts.push(Part {
            reference: reference.into(),
            lib_id: lib_id.into(),
            value: value.into(),
            footprint: footprint.into(),
            fields: vec![],
            block: "Misc".into(),
            rot: 0,
            in_bom: true,
            on_board: true,
            dnp: false,
            symbol,
        });
        let id = PartId(self.parts.len() - 1);
        PartBuilder { part: self.parts.last_mut().unwrap(), id }
    }

    pub fn find_part(&self, reference: &str) -> Option<PartId> {
        self.parts.iter().position(|p| p.reference == reference).map(PartId)
    }

    pub fn net_name(&self, n: NetId) -> &str {
        &self.nets[n.0].name
    }

    /// Pins by number ("3") or, failing that, by name ("EN").
    #[track_caller]
    pub fn pins(&self, part: PartId, key: &str) -> Vec<PinRef> {
        let p = &self.parts[part.0];
        let by = |f: &dyn Fn(&Pin) -> bool| -> Vec<PinRef> {
            p.symbol.pins.iter().enumerate().filter(|(_, x)| f(x)).map(|(i, _)| PinRef { part: part.0, pin: i }).collect()
        };
        let mut v = by(&|x| x.number == key);
        if v.is_empty() {
            v = by(&|x| x.name == key);
        }
        if v.is_empty() {
            let mut names: Vec<&str> = p.symbol.pins.iter().map(|x| x.name.as_str()).collect();
            names.sort();
            names.dedup();
            panic!("{} ({}) has no pin {key:?}; pins: {names:?}", p.reference, p.lib_id);
        }
        v
    }

    /// Names of all the part's pins (to mark every unused one nc).
    pub fn pin_names(&self, part: PartId) -> Vec<String> {
        self.parts[part.0].symbol.pins.iter().map(|p| p.name.clone()).collect()
    }

    /// Connect the given pins (numbers or names) of `part` to `net`.
    #[track_caller]
    pub fn connect(&mut self, net: NetId, part: PartId, keys: &[&str]) {
        for k in keys {
            for pr in self.pins(part, k) {
                self.attach(net, pr);
            }
        }
    }

    /// Mark pins unused (a no-connect flag in the schematic).
    #[track_caller]
    pub fn nc(&mut self, part: PartId, keys: &[&str]) {
        for k in keys {
            for pr in self.pins(part, k) {
                for q in self.colocated(pr) {
                    let key = self.key(q);
                    if let Some(&n) = self.pin_net.get(&key) {
                        panic!("{} is on net {}; cannot mark no-connect", self.describe(q), self.nets[n].name);
                    }
                    self.nc.insert(key);
                }
            }
        }
    }

    /// Declare that nets are driven from off-sheet (e.g. VBUS from the USB cable).
    pub fn pwr_flag(&mut self, nets: &[NetId]) {
        for n in nets {
            self.pwr_flags.push(self.nets[n.0].name.clone());
        }
    }

    pub fn pin(&self, pr: PinRef) -> &Pin {
        &self.parts[pr.part].symbol.pins[pr.pin]
    }

    pub fn describe(&self, pr: PinRef) -> String {
        let pin = self.pin(pr);
        format!("{}.{}({})", self.parts[pr.part].reference, pin.number, pin.name)
    }

    fn key(&self, pr: PinRef) -> (usize, String) {
        (pr.part, self.pin(pr).number.clone())
    }

    /// Pins stacked at one location are one electrical node in KiCad.
    fn colocated(&self, pr: PinRef) -> Vec<PinRef> {
        let at = self.pin(pr);
        let (x, y) = (at.x, at.y);
        self.parts[pr.part]
            .symbol
            .pins
            .iter()
            .enumerate()
            .filter(|(_, p)| p.x == x && p.y == y)
            .map(|(i, _)| PinRef { part: pr.part, pin: i })
            .collect()
    }

    #[track_caller]
    fn attach(&mut self, net: NetId, pr: PinRef) {
        for q in self.colocated(pr) {
            let key = self.key(q);
            if self.nc.contains(&key) {
                panic!("{} is marked no-connect but is being connected to {}", self.describe(q), self.nets[net.0].name);
            }
            match self.pin_net.get(&key) {
                Some(&n) if n == net.0 => continue,
                Some(&n) => panic!("{} is already on net {}; cannot also join {}", self.describe(q), self.nets[n].name, self.nets[net.0].name),
                None => {}
            }
            self.pin_net.insert(key, net.0);
            self.nets[net.0].pins.push(q);
        }
    }

    pub fn net_of(&self, part: usize, pin: &Pin) -> Option<&Net> {
        self.pin_net.get(&(part, pin.number.clone())).map(|&n| &self.nets[n])
    }

    pub fn is_nc(&self, part: usize, pin: &Pin) -> bool {
        self.nc.contains(&(part, pin.number.clone()))
    }

    /// Structural checks the circuit author must satisfy. Returns problems.
    pub fn check(&self) -> Vec<String> {
        let mut problems = vec![];
        for (i, part) in self.parts.iter().enumerate() {
            for p in &part.symbol.pins {
                if p.etype == "no_connect" {
                    continue;
                }
                if self.net_of(i, p).is_none() && !self.is_nc(i, p) {
                    problems.push(format!("{} pin {} ({}) is neither connected nor marked nc", part.reference, p.number, p.name));
                }
            }
        }
        for net in &self.nets {
            let refs: HashSet<&str> = net.pins.iter().map(|p| self.parts[p.part].reference.as_str()).collect();
            let flagged = self.pwr_flags.contains(&net.name);
            if net.pins.len() < 2 && !flagged {
                problems.push(format!("net {} has only {} pin(s)", net.name, net.pins.len()));
            } else if refs.len() < 2 && !flagged {
                problems.push(format!("net {} only touches {refs:?}", net.name));
            }
        }
        problems
    }
}
