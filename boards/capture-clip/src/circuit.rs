//! Capture clip, revision A: the circuit (docs/plan.md Phase E.1, D-024, D-025).
//!
//! USB-C (power + native USB) and a single-cell LiPo feed VSYS through a discrete power path;
//! a 0.3 µA LDO makes 3.3 V for the ESP32-S3-WROOM-1. A TP4057 charges the cell from USB at
//! 100 mA. One I2S microphone (powered from a GPIO so it draws nothing asleep), one RGB light,
//! one button that also wakes the chip, a reset button, test points.
//!
//! Sources: research/2026-09-29-parts-capture-clip.md (parts, pins, LCSC numbers),
//! research/2026-09-29-parts-starter.md §10 (0805 passives), research/2026-09-29-esp32-firmware.md
//! §1 (pin table) and §7 (dividers, charger-pin leakage). Anything marked UNVERIFIED or INFERRED
//! below is for the datasheet check (Phase E.2) before layout.
//!
//! Pin map (all RTC GPIOs, none a strapping pin except the button; firmware research §1):
//!   BUTTON_N    IO0   (pad 27)  main button to GND, 10k pull-up; ext1 wake; doubles as BOOT
//!   BAT_ADC     IO1   (pad 39)  battery / 3, ADC1_CH0
//!   VBUS_SENSE  IO2   (pad 38)  VBUS x 0.6, ext0 wake
//!   CHRG_N      IO4   (pad 4)   TP4057 CHRG through 100k
//!   STDBY_N     IO5   (pad 5)   TP4057 STDBY, direct
//!   MIC_PWR     IO6   (pad 6)   the microphone's VDD
//!   LED_R/G/B_N IO8/9/10 (pads 12/17/18) RGB cathodes through 1k
//!   I2S SCK/WS/SD IO11/12/13 (pads 19/20/21)
//!   USB D-/D+   IO19/IO20 (pads 13/14, fixed by the chip)
//!   UART0       TXD0/RXD0 (pads 37/36) to test points only
//!   CHG_FAST_N  IO7   (pad 7)   second PROG resistor (4.3k): low = ~333 mA charge, hi-Z = 100 mA
//!   spare       IO21 to a test point (rework)
//! Avoided: strapping pins IO3/IO45/IO46, IO35-37 (octal PSRAM), IO39-42 (pad JTAG).

use pcbgen::circuit::{Circuit, NetId};

const R0805: &str = "Resistor_SMD:R_0805_2012Metric";
const C0805: &str = "Capacitor_SMD:C_0805_2012Metric";
const LED0805: &str = "LED_SMD:LED_0805_2012Metric";
const TP: &str = "TestPoint:TestPoint_Pad_D1.5mm";

/// LCSC part numbers, all JLCPCB Basic 0805 (one size on the board; the 3D models exist).
/// 0 Ω, 10k, 5.1k, 1k and the capacitors: research/2026-09-29-parts-starter.md §10 (as on the
/// starter). 100k, 1M: research/2026-09-29-parts-capture-clip.md §9.
/// 150k is in neither research file: C17470 (UNI-ROYAL 0805W8F1503T5E, Basic, 1.0M in stock)
/// was read from JLCPCB's parts search on 2026-09-30.
fn lcsc(value: &str) -> &'static str {
    match value {
        "0" => "C17477",
        "10k" => "C17414",
        "5.1k" => "C27834",
        "1k" => "C17513",
        "100k" => "C149504",
        "150k" => "C17470",
        // 4.3k: C17667 (UNI-ROYAL 0805W8F4301T5E), read from JLCPCB's parts API on 2026-09-30:
        // Preferred Extended (no loading fee, like Basic), 172,996 in stock. No Basic 0805 4.3k was found.
        "4.3k" => "C17667",
        "1M" => "C17514",
        "100n" => "C49678",
        "1u" => "C28323",
        "10u" => "C15850",
        "22u" => "C45783",
        _ => panic!("no LCSC part for {value}"),
    }
}

/// An 0805 resistor from net `a` (pin 1) to net `b` (pin 2).
fn r(c: &mut Circuit, reference: &str, value: &str, a: NetId, b: NetId, block: &str) {
    let r = c.part(reference, "Device:R", value, R0805).lcsc(lcsc(value)).block(block).rot(90).id();
    c.connect(a, r, &["1"]);
    c.connect(b, r, &["2"]);
}

/// An 0805 capacitor from net `a` (pin 1) to net `b` (pin 2).
fn cap(c: &mut Circuit, reference: &str, value: &str, a: NetId, b: NetId, block: &str) {
    let k = c.part(reference, "Device:C", value, C0805).lcsc(lcsc(value)).block(block).id();
    c.connect(a, k, &["1"]);
    c.connect(b, k, &["2"]);
}

pub fn build() -> Circuit {
    let mut c = Circuit::new("capture-clip", "Capture clip", "A");
    // VBUS: USB 5 V. +BATT: the cell. VSYS: whichever is present (USB through D2, else the cell
    // through Q1), 3.0-4.8 V. +3V3: the LDO's output.
    let (vbus, vbat, vsys, v3, gnd) = (c.net("VBUS"), c.net("+BATT"), c.net("VSYS"), c.net("+3V3"), c.net("GND"));
    let (dp, dm) = (c.net("USB_D+"), c.net("USB_D-"));

    // --- USB-C input (as on the starter) -------------------------------------
    let usb = "USB-C input";
    let j1 = c
        .part("J1", "Connector:USB_C_Receptacle_USB2.0_16P", "USB-C", "Connector_USB:USB_C_Receptacle_HRO_TYPE-C-31-M-12")
        .lcsc("C165948")
        .mpn("TYPE-C-31-M-12")
        .block(usb)
        .id();
    c.connect(vbus, j1, &["VBUS"]);
    c.connect(gnd, j1, &["GND"]);
    c.connect(gnd, j1, &["SHIELD"]);
    c.connect(dp, j1, &["D+"]);
    c.connect(dm, j1, &["D-"]);
    c.nc(j1, &["SBU1", "SBU2"]);
    // 5.1k pull-downs on each CC line: "I'm a USB device, give me 5 V" (no USB-PD)
    let (cc1, cc2) = (c.net("CC1"), c.net("CC2"));
    c.connect(cc1, j1, &["CC1"]);
    c.connect(cc2, j1, &["CC2"]);
    r(&mut c, "R1", "5.1k", cc1, gnd, usb);
    r(&mut c, "R2", "5.1k", cc2, gnd, usb);

    // ESD: 0.6 pF clamp on D+/D- (pins 1, 2 = I/O, 3 = GND) and a 200 W TVS on VBUS; both
    // verified on the starter (HARDWARE_LESSONS). The KiCad symbol is a generic dual TVS.
    let esd = c
        .part("U3", "Device:D_TVS_Dual_AAC", "H5VUT2U", "Package_TO_SOT_SMD:SOT-23")
        .lcsc("C20615824")
        .mpn("H5VUT2U")
        .block(usb)
        .id();
    c.connect(dp, esd, &["1"]);
    c.connect(dm, esd, &["2"]);
    c.connect(gnd, esd, &["3"]);
    // No fuse here, unlike the starter (INFERRED to be enough; for the reviewer): the charger
    // limits itself to 100 mA and the LDO to ~0.55 A, and a PTC is one more Extended part.
    let d3 = c
        .part("D3", "Diode:SMF5V0A", "SMF5.0A", "Diode_SMD:D_SOD-123F")
        .lcsc("C19077497")
        .mpn("SMF5.0A")
        .block(usb)
        .rot(90)
        .id();
    c.connect(vbus, d3, &["1"]); // cathode
    c.connect(gnd, d3, &["2"]);

    // VBUS sense for the firmware: 100k over 150k, 5.0 V -> 3.0 V (firmware research §7).
    // Draws only while USB is present.
    let vbus_sense = c.net("VBUS_SENSE");
    r(&mut c, "R16", "100k", vbus, vbus_sense, usb);
    r(&mut c, "R17", "150k", vbus_sense, gnd, usb);

    // --- Charger ---------------------------------------------------------------
    // TP4057: 1 CHRG (open drain, low while charging), 2 GND, 3 BAT, 4 VCC, 5 STDBY (open
    // drain, low when full), 6 PROG (parts research §2, DS p.5; the KiCad symbol matches).
    // The part is SOT-23-6, not the symbol's default TSOT-23-6.
    let chg = "Charger";
    let u2 = c
        .part("U2", "Battery_Management:TP4057", "TP4057", "Package_TO_SOT_SMD:SOT-23-6")
        .lcsc("C12044")
        .mpn("TP4057-42-SOT26-R")
        .block(chg)
        .id();
    c.connect(vbus, u2, &["4"]);
    c.connect(gnd, u2, &["2"]);
    c.connect(vbat, u2, &["3"]);
    let prog = c.net("CHG_PROG");
    c.connect(prog, u2, &["6"]);
    // 10 kΩ = 100 mA (DS p.7 table), not the plan's 3 kΩ = 300 mA: this power path has no
    // input-current limit, so on USB the charge current adds to the board's own, and
    // 355 mA (Wi-Fi peak) + 300 mA is over a USB 2.0 port's 500 mA (board.toml [power]).
    // A 400 mAh cell fills in about 5 hours. 3 kΩ would be C17661.
    r(&mut c, "R3", "10k", prog, gnd, chg);
    // Fast charge, switched by the firmware (owner's request, 2026-09-30): a second PROG resistor
    // to GPIO7. Pin low (open drain, no pulls): 10k || 4.3k = 3.0k = ~333 mA (datasheet check). Pin released (hi-Z: asleep, in
    // reset, unprogrammed): R3 alone = 100 mA, the safe default. The firmware releases it whenever
    // Wi-Fi is on, so 355 + 333 mA never add up on the USB port. INFERRED to work: PROG sits near
    // 1 V while charging, so the pin only ever sinks ~0.23 mA; never drive it high (3.3 V into
    // PROG). For the datasheet check.
    let chg_fast = c.net("CHG_FAST_N");
    r(&mut c, "R19", "4.3k", prog, chg_fast, chg);
    // capacitor values INFERRED (the usual 1 µF in / 10 µF at the cell; the research did not
    // read the datasheet's application circuit). 1 µF keeps VBUS + VSYS near USB's 10 µF inrush limit.
    cap(&mut c, "C1", "1u", vbus, gnd, chg);
    cap(&mut c, "C2", "10u", vbat, gnd, chg);
    // Charge light between VBUS and CHRG (the datasheet circuit): on while charging. From 5 V,
    // (5 - ~2 V) / 1k = ~3 mA. Red 0805, as the starter's status LED.
    let (chrg, chrg_led, chrg_sense) = (c.net("CHRG_N"), c.net("CHRG_LED"), c.net("CHRG_SENSE"));
    c.connect(chrg, u2, &["1"]);
    r(&mut c, "R4", "1k", vbus, chrg_led, chg);
    let d1 = c.part("D1", "Device:LED", "red", LED0805).lcsc("C84256").block(chg).id();
    c.connect(chrg_led, d1, &["A"]);
    c.connect(chrg, d1, &["K"]);
    // The LED pulls CHRG toward VBUS when not charging, above the GPIO's 3.6 V rating: 100k in
    // series limits that to a few µA into the pin's clamp (firmware research §7).
    // Firmware: never an internal pull-up on this pin (with R5 in series a low would read as
    // ~2.4 V; datasheet check 2026-09-30). The LED path is its pull-up.
    r(&mut c, "R5", "100k", chrg, chrg_sense, chg);
    let stdby = c.net("STDBY_N");
    c.connect(stdby, u2, &["5"]);

    // --- Power path --------------------------------------------------------------
    // USB present: D2 feeds VSYS (~4.6 V); Q1's gate is at 5 V, so it is off and the cell only
    // charges. USB absent: R6 pulls the gate to 0 V and Q1 connects the cell to VSYS
    // (parts research §3; INFERRED from the well-known Adafruit circuit).
    let path = "Power path";
    // AO3401A P-FET: 1 G, 2 S, 3 D (KiCad symbol; the datasheet drawing has no pin numbers: UNVERIFIED)
    let q1 = c
        .part("Q1", "Transistor_FET:AO3401A", "AO3401A", "Package_TO_SOT_SMD:SOT-23")
        .lcsc("C15127")
        .mpn("AO3401A")
        .block(path)
        .id();
    c.connect(vbus, q1, &["G"]);
    c.connect(vsys, q1, &["S"]);
    c.connect(vbat, q1, &["D"]);
    // Gate pull-down. 10k, not 100k (datasheet check, 2026-09-30): D2's reverse leakage into
    // 100k could lift VBUS enough when warm to half turn Q1 off and fake a USB detect.
    r(&mut c, "R6", "10k", vbus, gnd, path);
    // RB168MM-40 Schottky (<= 0.55 µA at 40 V), SOD-123FL, pad 1 = cathode
    // (research/2026-09-30-power-path-fix.md: the RB160M-30 leaked ~6 µA, over the sleep budget)
    let d2 = c
        .part("D2", "Device:D_Schottky", "RB168MM-40", "Diode_SMD:D_SOD-123F")
        .lcsc("C509936")
        .mpn("RB168MM-40TR")
        .block(path)
        .id();
    c.connect(vbus, d2, &["A"]);
    c.connect(vsys, d2, &["K"]);

    // Battery connector: JST PH 2-pin, side entry. No protection IC on the board: a protected
    // cell is required (D-024; Adafruit #3898).
    // UNVERIFIED: which pad is +. Pad 1 = + here, INFERRED from the parts research §6 (plug
    // face with the bump up: red on the right; SparkFun: "pin 1 as +VBATT") and the footprint
    // (mouth toward +y, pad 1 at x = -1). The datasheet check must tie it to JST's drawing
    // before layout; swapping it is these two lines. A reversed cell does not damage the
    // TP4057 (DS pp.7-8).
    let j2 = c
        .part(
            "J2",
            "Connector_Generic_MountingPin:Conn_01x02_MountingPin",
            "LiPo",
            "Connector_JST:JST_PH_S2B-PH-SM4-TB_1x02-1MP_P2.00mm_Horizontal",
        )
        .lcsc("C295747")
        .mpn("S2B-PH-SM4-TB(LF)(SN)")
        .block(path)
        .id();
    c.connect(vbat, j2, &["1"]);
    c.connect(gnd, j2, &["2"]);
    c.nc(j2, &["MP"]);

    // Battery voltage for the ADC: 2 x 1M over 1M = cell / 3 (4.35 V -> 1.45 V, inside the
    // ADC's 6 dB range), 100 nF so the ADC sees a low impedance. Draws 1.4 µA always.
    let (bat_mid, bat_adc) = (c.net("BAT_DIV"), c.net("BAT_ADC"));
    r(&mut c, "R13", "1M", vbat, bat_mid, path);
    r(&mut c, "R14", "1M", bat_mid, bat_adc, path);
    r(&mut c, "R15", "1M", bat_adc, gnd, path);
    cap(&mut c, "C9", "100n", bat_adc, gnd, path);

    // --- 3.3 V regulator -----------------------------------------------------------
    // HE9073A33M5R (SOT-23-5): 1 VIN, 2 GND, 3 CE, 4 NC, 5 VOUT; input abs. max 9 V, above the
    // TVS's 9.2 V clamp less D2's drop (research/2026-09-30-power-path-fix.md). Pin numbers
    // INFERRED from the datasheet's dot and standard SOT-23-5 numbering (no numbers in its figure).
    // KiCad has no HE9073 symbol; XC6220B331MR has the same pin numbers.
    let reg = "3.3 V regulator";
    let u4 = c
        .part("U4", "Regulator_Linear:XC6220B331MR", "HE9073A33M5R", "Package_TO_SOT_SMD:SOT-23-5")
        .lcsc("C723789")
        .mpn("HE9073A33M5R")
        .block(reg)
        .id();
    c.connect(vsys, u4, &["1"]);
    c.connect(gnd, u4, &["2"]);
    c.connect(vsys, u4, &["3"]); // always enabled: the board sleeps instead of switching off
    c.connect(v3, u4, &["5"]);
    cap(&mut c, "C3", "10u", vsys, gnd, reg); // 10 µF at the input (HE9073 DS p.8)
    cap(&mut c, "C4", "10u", v3, gnd, reg); // HE9073 asks for 10 µF at the output

    // --- ESP32-S3 module -----------------------------------------------------------
    let mcu = "ESP32-S3";
    let u1 = c
        .part("U1", "RF_Module:ESP32-S3-WROOM-1", "ESP32-S3-WROOM-1-N16R8", "pcbgen:ESP32-S3-WROOM-1_EPAD-Drill0.3")
        .lcsc("C2913202")
        .mpn("ESP32-S3-WROOM-1-N16R8")
        .block(mcu)
        .id();
    c.connect(v3, u1, &["3V3"]);
    c.connect(gnd, u1, &["GND"]);
    cap(&mut c, "C5", "22u", v3, gnd, mcu);
    cap(&mut c, "C6", "100n", v3, gnd, mcu);
    c.connect(dm, u1, &["USB_D-"]);
    c.connect(dp, u1, &["USB_D+"]);

    let en = c.net("EN");
    c.connect(en, u1, &["EN"]);
    r(&mut c, "R7", "10k", v3, en, mcu); // EN pull-up
    cap(&mut c, "C7", "1u", en, gnd, mcu); // EN power-on delay (Espressif: 10k/1uF)

    // --- Buttons -------------------------------------------------------------------
    let btn = "Buttons";
    // Main button: 6x6 mm, 5 mm tall, for a printed cap. To GND, 10k pull-up, no capacitor
    // (the strap must settle within 3 ms of EN rising; firmware research §1).
    let button = c.net("BUTTON_N");
    c.connect(button, u1, &["IO0"]);
    r(&mut c, "R8", "10k", v3, button, btn);
    let sw1 = c
        .part("SW1", "Switch:SW_Push", "BUTTON", "Button_Switch_SMD:SW_Push_1TS009xxxx-xxxx-xxxx_6x6x5mm")
        .lcsc("C319409")
        .mpn("1TS009A-1800-5000-CT")
        .block(btn)
        .id();
    c.connect(button, sw1, &["1"]);
    c.connect(gnd, sw1, &["2"]);
    // Reset: small, behind a pinhole in the case
    let sw2 = c
        .part("SW2", "Switch:SW_Push", "RESET", "Button_Switch_SMD:SW_SPST_TS-1088-xR020")
        .lcsc("C720477")
        .mpn("TS-1088-AR02016")
        .block(btn)
        .id();
    c.connect(en, sw2, &["1"]);
    c.connect(gnd, sw2, &["2"]);

    // --- Power and charger signals to the module --------------------------------------
    c.connect(bat_adc, u1, &["IO1"]);
    c.connect(vbus_sense, u1, &["IO2"]);
    c.connect(chrg_sense, u1, &["IO4"]);
    // STDBY straight to its pin. Firmware: an internal pull-up on IO5 only while VBUS_SENSE is
    // high, off before sleep (it can leak into the unpowered charger; firmware research §7).
    // IO4 (CHRG) gets no pull-up at all.
    c.connect(stdby, u1, &["IO5"]);

    // --- Microphone ------------------------------------------------------------------
    // ICS-43434: 1 WS, 2 LR, 3 GND, 4 SCK, 5 VDD, 6 SD (parts research §1, DS p.10; the KiCad
    // symbol matches). VDD comes from a GPIO (0.5 mA), so the mic draws nothing asleep.
    // R18 (0 Ω) sits between the GPIO and the mic's VDD: a rework point (workflow step 6) if
    // powering a mic from a pin turns out noisy; it can take a small resistor or be lifted to
    // feed VDD from elsewhere.
    let mic = "Microphone";
    let (mic_pwr, mic_vdd) = (c.net("MIC_PWR"), c.net("MIC_VDD"));
    let (sck, ws, sd) = (c.net("I2S_SCK"), c.net("I2S_WS"), c.net("I2S_SD"));
    r(&mut c, "R18", "0", mic_pwr, mic_vdd, mic);
    let mk1 = c
        .part("MK1", "Sensor_Audio:ICS-43434", "ICS-43434", "Sensor_Audio:InvenSense_ICS-43434-6_3.5x2.65mm")
        .lcsc("C5656610")
        .mpn("ICS-43434")
        .block(mic)
        .id();
    c.connect(mic_vdd, mk1, &["VDD"]);
    c.connect(gnd, mk1, &["GND"]);
    c.connect(gnd, mk1, &["LR"]); // left channel; tied to VDD it would draw ~33 µA
    c.connect(ws, mk1, &["WS"]);
    c.connect(sck, mk1, &["SCK"]);
    c.connect(sd, mk1, &["SD"]);
    cap(&mut c, "C8", "100n", mic_vdd, gnd, mic);
    r(&mut c, "R9", "100k", sd, gnd, mic); // SD tristates: pull-down (DS p.10, p.12)
    c.connect(mic_pwr, u1, &["IO6"]);
    c.connect(sck, u1, &["IO11"]);
    c.connect(ws, u1, &["IO12"]);
    c.connect(sd, u1, &["IO13"]);

    // --- Colour light -----------------------------------------------------------------
    // LTST-C19HE1WT: 1 red, 2 green, 3 blue cathodes (DS p.2), 4 common anode (UNVERIFIED:
    // only in the package drawing). Anode on VSYS: blue and green need up to 3.9 V. Each
    // cathode sinks into a GPIO through 1k: about 0.5-2.5 mA depending on colour and cell
    // voltage (INFERRED; the firmware balances the colours with PWM).
    let light = "Colour light";
    let d4 = c
        .part("D4", "Device:LED_RGBA", "RGB", "LED_SMD:LED_LiteOn_LTST-C19HE1WT")
        .lcsc("C458749")
        .mpn("LTST-C19HE1WT")
        .block(light)
        .id();
    c.connect(vsys, d4, &["4"]);
    for (i, (colour, pin, io)) in [("R", "1", "IO8"), ("G", "2", "IO9"), ("B", "3", "IO10")].into_iter().enumerate() {
        let (k, gpio) = (c.net(&format!("LED_{colour}_K")), c.net(&format!("LED_{colour}_N")));
        c.connect(k, d4, &[pin]);
        r(&mut c, &format!("R{}", 10 + i), "1k", k, gpio, light);
        c.connect(gpio, u1, &[io]);
    }

    // --- Test points (rework aids, workflow step 6) and mounting holes -------------------
    let (tx, rx) = (c.net("UART_TX"), c.net("UART_RX"));
    c.connect(tx, u1, &["TXD0"]);
    c.connect(rx, u1, &["RXD0"]);
    c.connect(chg_fast, u1, &["IO7"]);
    let spare = c.net("SPARE_IO21");
    c.connect(spare, u1, &["IO21"]);

    let used = [
        "GND", "3V3", "EN", "IO0", "IO1", "IO2", "IO4", "IO5", "IO6", "IO7", "IO8", "IO9", "IO10", "IO11", "IO12", "IO13", "IO21",
        "USB_D-", "USB_D+", "TXD0", "RXD0",
    ];
    let mut unused: Vec<String> = c.pin_names(u1).into_iter().filter(|n| !used.contains(&n.as_str())).collect();
    unused.sort();
    unused.dedup();
    c.nc(u1, &unused.iter().map(String::as_str).collect::<Vec<_>>());

    for (i, net) in [vbus, vsys, vbat, v3, gnd, tx, rx, spare].into_iter().enumerate() {
        let name = c.net_name(net).to_string();
        let tp = c.part(&format!("TP{}", i + 1), "Connector:TestPoint", &name, TP).block("Test points").not_in_bom().id();
        c.connect(net, tp, &["1"]);
    }
    for i in 1..=2 {
        // Ø2.5 hole for an M2 screw: its courtyard (r 2.75) holds the case's boss (r 2.7: M2
        // clearance hole 2.4 + 2 x 1.5 mm wall); the 2.2 mm footprint's (r 2.45) does not.
        c.part(&format!("H{i}"), "Mechanical:MountingHole", "M2", "MountingHole:MountingHole_2.5mm")
            .block("Mounting holes")
            .not_in_bom();
    }

    // VBUS and GND come from the cable; VSYS is fed through a diode or a FET and MIC_VDD from
    // a GPIO through R18, which ERC does not count as power sources. +BATT is driven by the charger's BAT
    // pin and +3V3 by the regulator.
    c.pwr_flag(&[vbus, gnd, vsys, mic_vdd]);
    c
}
