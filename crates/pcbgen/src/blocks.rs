//! Reusable circuit blocks for ESP32-S3 boards, taken from the starter board (parts and pins
//! checked against their datasheets: HARDWARE_LESSONS.md, research/2026-09-29-datasheet-check.md).
//!
//! Each block takes the nets it connects to and the references to use, adds its parts in a
//! fixed order and returns their ids. Parts are added in the same order as the starter board
//! always did, so a board converted to blocks writes the same schematic.
//!
//! ```ignore
//! let (vbus, v5, v3, gnd) = (c.net("VBUS"), c.net("+5V"), c.net("+3V3"), c.net("GND"));
//! let (dp, dm) = (c.net("USB_D+"), c.net("USB_D-"));
//! blocks::usb_c_power(&mut c, &UsbCPower::new(vbus, gnd, dp, dm).fused(v5));
//! blocks::ldo_3v3(&mut c, &Ldo3v3::new(v5, v3, gnd));
//! let (en, boot) = (c.net("EN"), c.net("BOOT"));
//! let core = blocks::esp32s3_core(&mut c, &Esp32s3Core::new(v3, gnd, en).usb(dp, dm).boot(boot));
//! // ... the board's own GPIOs ...
//! blocks::nc_unconnected(&mut c, core.module);
//! ```

use crate::circuit::{Circuit, NetId, PartId};

pub const R0805: &str = "Resistor_SMD:R_0805_2012Metric";
pub const C0805: &str = "Capacitor_SMD:C_0805_2012Metric";
pub const LED0805: &str = "LED_SMD:LED_0805_2012Metric";

/// (LCSC number, MPN) of the 0805 resistors and capacitors the starter uses, all JLCPCB Basic
/// (research/2026-09-29-parts-starter.md §10). MPNs copied from JLCPCB's listing of each code
/// (`pcb parts show`, 2026-10-01); the cost stage's PARTS gate checks them against the live
/// listing.
pub fn lcsc_0805(value: &str) -> Option<(&'static str, &'static str)> {
    Some(match value {
        "10k" => ("C17414", "0805W8F1002T5E"),
        "5.1k" => ("C27834", "0805W8F5101T5E"),
        "4.7k" => ("C17673", "0805W8F4701T5E"),
        "1k" => ("C17513", "0805W8F1001T5E"),
        "2.2k" => ("C17520", "0805W8F2201T5E"),
        "100n" => ("C49678", "CC0805KRX7R9BB104"),
        "1u" => ("C28323", "CL21B105KBFNNNE"),
        "10u" => ("C15850", "CL21A106KAYNNNE"),
        "22u" => ("C45783", "CL21A226MAQNNNE"),
        _ => return None,
    })
}

/// An 0805 resistor from net `a` (pin 1) to net `b` (pin 2), drawn upright. Panics on a value
/// with no LCSC number in [`lcsc_0805`].
#[track_caller]
pub fn resistor(c: &mut Circuit, reference: &str, value: &str, a: NetId, b: NetId, block: &str) -> PartId {
    let (lcsc, mpn) = lcsc_0805(value).unwrap_or_else(|| panic!("{reference}: no LCSC part for {value}"));
    let r = c.part(reference, "Device:R", value, R0805).lcsc(lcsc).mpn(mpn).block(block).rot(90).id();
    c.connect(a, r, &["1"]);
    c.connect(b, r, &["2"]);
    r
}

/// An 0805 capacitor from net `a` (pin 1) to net `b` (pin 2). Panics on a value with no LCSC
/// number in [`lcsc_0805`].
#[track_caller]
pub fn capacitor(c: &mut Circuit, reference: &str, value: &str, a: NetId, b: NetId, block: &str) -> PartId {
    let (lcsc, mpn) = lcsc_0805(value).unwrap_or_else(|| panic!("{reference}: no LCSC part for {value}"));
    let k = c.part(reference, "Device:C", value, C0805).lcsc(lcsc).mpn(mpn).block(block).id();
    c.connect(a, k, &["1"]);
    c.connect(b, k, &["2"]);
    k
}

/// Mark every pin of `part` that is on no net as unused (no-connect). Call it last, after
/// the board has connected all the pins it uses.
pub fn nc_unconnected(c: &mut Circuit, part: PartId) {
    let p = &c.parts[part.0];
    let mut unused: Vec<String> =
        p.symbol.pins.iter().filter(|pin| c.net_of(part.0, pin).is_none()).map(|pin| pin.name.clone()).collect();
    unused.sort();
    unused.dedup();
    c.nc(part, &unused.iter().map(String::as_str).collect::<Vec<_>>());
}

// --- USB-C power input ----------------------------------------------------------------------

/// References of the USB-C input's parts.
#[derive(Clone, Copy, Debug)]
pub struct UsbCRefs {
    pub connector: &'static str,
    pub cc1: &'static str,
    pub cc2: &'static str,
    pub esd: &'static str,
    pub tvs: &'static str,
    pub fuse: &'static str,
}

impl Default for UsbCRefs {
    fn default() -> Self {
        UsbCRefs { connector: "J1", cc1: "R1", cc2: "R2", esd: "U3", tvs: "D3", fuse: "F1" }
    }
}

/// USB-C receptacle (HRO TYPE-C-31-M-12), 5.1k CC pull-downs (a USB device asking for
/// default 5 V power, no PD), a 0.6 pF ESD clamp on D+/D- and a 200 W TVS on the 5 V rail.
/// With [`fused`](Self::fused), a 0.75 A PTC fuse sits between VBUS and the fused rail and the
/// TVS goes after it (the starter); without, the TVS is on VBUS (capture-clip).
#[derive(Clone, Copy, Debug)]
pub struct UsbCPower {
    pub vbus: NetId,
    pub gnd: NetId,
    pub dp: NetId,
    pub dm: NetId,
    /// The rail after the fuse; None: no fuse.
    pub fused: Option<NetId>,
    pub block: &'static str,
    pub refs: UsbCRefs,
}

impl UsbCPower {
    pub fn new(vbus: NetId, gnd: NetId, dp: NetId, dm: NetId) -> Self {
        UsbCPower { vbus, gnd, dp, dm, fused: None, block: "USB-C input", refs: UsbCRefs::default() }
    }
    /// Add the PTC fuse, feeding `out`.
    pub fn fused(mut self, out: NetId) -> Self {
        self.fused = Some(out);
        self
    }
}

pub struct UsbCParts {
    pub connector: PartId,
    pub esd: PartId,
    pub tvs: PartId,
    pub fuse: Option<PartId>,
    /// The CC1 and CC2 nets.
    pub cc: (NetId, NetId),
}

#[track_caller]
pub fn usb_c_power(c: &mut Circuit, o: &UsbCPower) -> UsbCParts {
    let (usb, refs, gnd) = (o.block, &o.refs, o.gnd);
    let j1 = c
        .part(refs.connector, "Connector:USB_C_Receptacle_USB2.0_16P", "USB-C", "Connector_USB:USB_C_Receptacle_HRO_TYPE-C-31-M-12")
        .lcsc("C165948")
        .mpn("TYPE-C-31-M-12")
        .block(usb)
        .id();
    c.connect(o.vbus, j1, &["VBUS"]);
    c.connect(gnd, j1, &["GND"]);
    c.connect(gnd, j1, &["SHIELD"]);
    c.connect(o.dp, j1, &["D+"]);
    c.connect(o.dm, j1, &["D-"]);
    c.nc(j1, &["SBU1", "SBU2"]);
    // 5.1k pull-downs on each CC line tell the charger "I'm a USB device, give me 5 V"
    let (cc1, cc2) = (c.net("CC1"), c.net("CC2"));
    c.connect(cc1, j1, &["CC1"]);
    c.connect(cc2, j1, &["CC2"]);
    resistor(c, refs.cc1, "5.1k", cc1, gnd, usb);
    resistor(c, refs.cc2, "5.1k", cc2, gnd, usb);

    // ESD: 0.6 pF clamp on D+/D- (pins 1, 2 = I/O, 3 = GND) and a 200 W TVS on 5 V.
    // Both JLCPCB Preferred Extended (no loading fee); see DECISIONS.md D-008.
    // The KiCad symbol is a generic dual TVS: only the pin numbers matter.
    let esd = c
        .part(refs.esd, "Device:D_TVS_Dual_AAC", "H5VUT2U", "Package_TO_SOT_SMD:SOT-23")
        .lcsc("C20615824")
        .mpn("H5VUT2U")
        .block(usb)
        .id();
    c.connect(o.dp, esd, &["1"]);
    c.connect(o.dm, esd, &["2"]);
    c.connect(gnd, esd, &["3"]);
    let tvs = c
        .part(refs.tvs, "Diode:SMF5V0A", "SMF5.0A", "Diode_SMD:D_SOD-123F")
        .lcsc("C19077497")
        .mpn("SMF5.0A")
        .block(usb)
        .rot(90)
        .id();
    // after the fuse when there is one: a faulty charger trips the fuse instead of burning the TVS
    c.connect(o.fused.unwrap_or(o.vbus), tvs, &["1"]); // cathode
    c.connect(gnd, tvs, &["2"]);

    // 0.75 A hold: Bourns publishes a derating table (0.61 A at 50 C, 0.52 A at 60 C); a
    // 0.5 A PTC holds only ~0.35-0.40 A in a warm box, no margin over the ~0.35 A load (D-015)
    let fuse = o.fused.map(|out| {
        let f1 = c
            .part(refs.fuse, "Device:Polyfuse", "750mA", "Fuse:Fuse_1206_3216Metric")
            .lcsc("C89653")
            .mpn("MF-NSMF075-2")
            .block(usb)
            .rot(90)
            .id();
        c.connect(o.vbus, f1, &["1"]);
        c.connect(out, f1, &["2"]);
        f1
    });
    UsbCParts { connector: j1, esd, tvs, fuse, cc: (cc1, cc2) }
}

// --- 3.3 V regulator ----------------------------------------------------------------------------

/// Which 3.3 V LDO.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ldo {
    /// ST LDL1117S33R, SOT-223, up to 1.2 A, 0.35 V dropout, ceramic-stable (D-007); the
    /// starter's. Datasheet-checked (HARDWARE_LESSONS.md). 10 µF in, 1 µF out: with the
    /// module's 22 µF the total on 3V3 stays inside ST's 1-22 µF stability plot.
    Ldl1117s33,
    /// HE9073A33M5R, SOT-23-5, 0.3 µA quiescent, for battery boards (capture-clip's). Its pin
    /// numbers are INFERRED (capture-clip's circuit.rs); CE is tied to the input. 10 µF in and out.
    He9073a33,
}

#[derive(Clone, Copy, Debug)]
pub struct LdoRefs {
    pub ldo: &'static str,
    pub c_in: &'static str,
    pub c_out: &'static str,
}

impl Default for LdoRefs {
    fn default() -> Self {
        LdoRefs { ldo: "U2", c_in: "C1", c_out: "C2" }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Ldo3v3 {
    pub input: NetId,
    pub output: NetId,
    pub gnd: NetId,
    pub part: Ldo,
    pub block: &'static str,
    pub refs: LdoRefs,
}

impl Ldo3v3 {
    pub fn new(input: NetId, output: NetId, gnd: NetId) -> Self {
        Ldo3v3 { input, output, gnd, part: Ldo::Ldl1117s33, block: "3.3 V regulator", refs: LdoRefs::default() }
    }
}

pub struct LdoParts {
    pub ldo: PartId,
    pub c_in: PartId,
    pub c_out: PartId,
}

#[track_caller]
pub fn ldo_3v3(c: &mut Circuit, o: &Ldo3v3) -> LdoParts {
    let (reg, refs) = (o.block, &o.refs);
    match o.part {
        Ldo::Ldl1117s33 => {
            // KiCad has no LDL1117 symbol; LD1117S33TR_SOT223 has the same SOT-223 pinout
            // (1 GND, 2 OUT/tab, 3 IN).
            let u = c
                .part(refs.ldo, "Regulator_Linear:LD1117S33TR_SOT223", "LDL1117S33R", "Package_TO_SOT_SMD:SOT-223-3_TabPin2")
                .lcsc("C435835")
                .mpn("LDL1117S33R")
                .block(reg)
                .id();
            c.connect(o.input, u, &["VI"]);
            c.connect(o.output, u, &["VO"]);
            c.connect(o.gnd, u, &["GND"]);
            let c_in = capacitor(c, refs.c_in, "10u", o.input, o.gnd, reg);
            // 1u at the regulator, 22u at the module (Espressif Fig. 9-1): keeps the total on 3V3
            // (~23 uF nominal, less at 3.3 V bias) at the edge of ST's 1-22 uF stability plot
            let c_out = capacitor(c, refs.c_out, "1u", o.output, o.gnd, reg);
            LdoParts { ldo: u, c_in, c_out }
        }
        Ldo::He9073a33 => {
            // 1 VIN, 2 GND, 3 CE, 4 NC, 5 VOUT (INFERRED from the datasheet's dot and standard
            // SOT-23-5 numbering). KiCad has no HE9073 symbol; XC6220B331MR has the same pin numbers.
            let u = c
                .part(refs.ldo, "Regulator_Linear:XC6220B331MR", "HE9073A33M5R", "Package_TO_SOT_SMD:SOT-23-5")
                .lcsc("C723789")
                .mpn("HE9073A33M5R")
                .block(reg)
                .id();
            c.connect(o.input, u, &["1"]);
            c.connect(o.gnd, u, &["2"]);
            c.connect(o.input, u, &["3"]); // always enabled
            c.connect(o.output, u, &["5"]);
            let c_in = capacitor(c, refs.c_in, "10u", o.input, o.gnd, reg);
            let c_out = capacitor(c, refs.c_out, "10u", o.output, o.gnd, reg);
            LdoParts { ldo: u, c_in, c_out }
        }
    }
}

// --- ESP32-S3 module and what it needs ---------------------------------------------------------

/// An ESP32-S3 module: the part that goes on the board.
#[derive(Clone, Copy, Debug)]
pub struct Esp32Module {
    pub lib_id: &'static str,
    pub value: &'static str,
    pub footprint: &'static str,
    pub lcsc: &'static str,
    pub mpn: &'static str,
}

impl Esp32Module {
    /// ESP32-S3-WROOM-1-N16R8 (16 MB flash, 8 MB octal PSRAM, PCB antenna), JLCPCB Extended.
    /// The footprint is pcbgen's copy of KiCad's with 0.3 mm EPAD holes (D-015: smaller holes
    /// cost extra at JLCPCB).
    pub const WROOM1_N16R8: Esp32Module = Esp32Module {
        lib_id: "RF_Module:ESP32-S3-WROOM-1",
        value: "ESP32-S3-WROOM-1-N16R8",
        footprint: "pcbgen:ESP32-S3-WROOM-1_EPAD-Drill0.3",
        lcsc: "C2913202",
        mpn: "ESP32-S3-WROOM-1-N16R8",
    };
}

/// A push button between a net (pin 1) and GND (pin 2).
#[derive(Clone, Copy, Debug)]
pub struct Button {
    pub reference: &'static str,
    pub value: &'static str,
    pub footprint: &'static str,
    pub lcsc: &'static str,
    pub mpn: Option<&'static str>,
    pub block: &'static str,
}

impl Button {
    /// XKB TS-1187A, 5.1 x 5.1 mm SMD (the starter's two buttons; JLCPCB Basic).
    pub const fn ts1187a(reference: &'static str, value: &'static str) -> Button {
        Button { reference, value, footprint: "Button_Switch_SMD:SW_Push_1P1T_XKB_TS-1187A", lcsc: "C318884", mpn: Some("TS-1187A-B-A-B"), block: "Buttons" }
    }
}

/// The BOOT strap on IO0: a 10k pull-up and, optionally, a button to GND.
#[derive(Clone, Copy, Debug)]
pub struct Boot {
    pub net: NetId,
    pub pull_up: &'static str,
    pub button: Option<Button>,
}

#[derive(Clone, Copy, Debug)]
pub struct CoreRefs {
    pub module: &'static str,
    /// 22 µF bulk capacitor at the 3V3 pad.
    pub c_bulk: &'static str,
    /// 100 nF decoupling capacitor at the 3V3 pad.
    pub c_decouple: &'static str,
    pub r_en: &'static str,
    pub c_en: &'static str,
}

impl Default for CoreRefs {
    fn default() -> Self {
        CoreRefs { module: "U1", c_bulk: "C3", c_decouple: "C4", r_en: "R4", c_en: "C5" }
    }
}

/// The module with 22 µF + 100 nF on 3V3 (Espressif Fig. 9-1), the EN power-on delay
/// (10k pull-up, 1 µF; WROOM-1 §9), optionally native USB on IO19/IO20, a RESET button on EN
/// and the BOOT strap on IO0. Every other pin is left for the board; call [`nc_unconnected`]
/// once the board has connected the GPIOs it uses.
#[derive(Clone, Copy, Debug)]
pub struct Esp32s3Core {
    pub v3: NetId,
    pub gnd: NetId,
    pub en: NetId,
    /// (D+, D-) to the module's USB_D+/USB_D- pins.
    pub usb: Option<(NetId, NetId)>,
    pub reset: Option<Button>,
    pub boot: Option<Boot>,
    pub module: Esp32Module,
    pub block: &'static str,
    pub refs: CoreRefs,
}

impl Esp32s3Core {
    /// The starter's core: WROOM-1-N16R8, RESET button SW1 (TS-1187A); add `.usb()` and `.boot()`.
    pub fn new(v3: NetId, gnd: NetId, en: NetId) -> Self {
        Esp32s3Core {
            v3,
            gnd,
            en,
            usb: None,
            reset: Some(Button::ts1187a("SW1", "RESET")),
            boot: None,
            module: Esp32Module::WROOM1_N16R8,
            block: "ESP32-S3",
            refs: CoreRefs::default(),
        }
    }
    pub fn usb(mut self, dp: NetId, dm: NetId) -> Self {
        self.usb = Some((dp, dm));
        self
    }
    /// The starter's BOOT strap: 10k pull-up R5 and button SW2 (TS-1187A) on `net`.
    pub fn boot(mut self, net: NetId) -> Self {
        self.boot = Some(Boot { net, pull_up: "R5", button: Some(Button::ts1187a("SW2", "BOOT")) });
        self
    }
}

pub struct CoreParts {
    pub module: PartId,
    pub reset: Option<PartId>,
    pub boot: Option<PartId>,
}

#[track_caller]
fn button(c: &mut Circuit, b: &Button, net: NetId, gnd: NetId) -> PartId {
    let mut pb = c.part(b.reference, "Switch:SW_Push", b.value, b.footprint).lcsc(b.lcsc);
    if let Some(mpn) = b.mpn {
        pb = pb.mpn(mpn);
    }
    let sw = pb.block(b.block).id();
    c.connect(net, sw, &["1"]);
    c.connect(gnd, sw, &["2"]);
    sw
}

#[track_caller]
pub fn esp32s3_core(c: &mut Circuit, o: &Esp32s3Core) -> CoreParts {
    let (mcu, refs, m, v3, gnd) = (o.block, &o.refs, &o.module, o.v3, o.gnd);
    let u1 = c.part(refs.module, m.lib_id, m.value, m.footprint).lcsc(m.lcsc).mpn(m.mpn).block(mcu).id();
    c.connect(v3, u1, &["3V3"]);
    c.connect(gnd, u1, &["GND"]);
    capacitor(c, refs.c_bulk, "22u", v3, gnd, mcu);
    capacitor(c, refs.c_decouple, "100n", v3, gnd, mcu);
    if let Some((dp, dm)) = o.usb {
        c.connect(dm, u1, &["USB_D-"]);
        c.connect(dp, u1, &["USB_D+"]);
    }

    c.connect(o.en, u1, &["EN"]);
    resistor(c, refs.r_en, "10k", v3, o.en, mcu); // EN pull-up
    capacitor(c, refs.c_en, "1u", o.en, gnd, mcu); // EN power-on delay (Espressif: 10k/1uF)
    let reset = o.reset.map(|b| button(c, &b, o.en, gnd));
    let boot = o.boot.and_then(|b| {
        c.connect(b.net, u1, &["IO0"]);
        let block = b.button.map_or(mcu, |s| s.block);
        resistor(c, b.pull_up, "10k", v3, b.net, block);
        b.button.map(|s| button(c, &s, b.net, gnd))
    });
    CoreParts { module: u1, reset, boot }
}
