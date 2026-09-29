//! ESP32-S3 starter board, Rev 0: the circuit.
//!
//! USB-C (power + native USB) -> PTC fuse -> 3.3 V LDO -> ESP32-S3-WROOM-1.
//! SHT40 temperature/humidity sensor and a Qwiic connector share one I2C bus.
//! EN (reset) and BOOT buttons, power LED, status LED, test points.
//!
//! GPIO choices (pins are module pads, see HARDWARE_LESSONS.md pinout table):
//!   USB D-/D+  IO19/IO20  (pads 13/14, fixed by the chip)
//!   I2C SDA    IO1        (pad 39, right edge, next to the sensor)
//!   I2C SCL    IO2        (pad 38)
//!   Status LED IO48       (pad 25, bottom row)
//!   BOOT       IO0        (pad 27, strapping: low at reset = download mode)
//!   UART0      TXD0/RXD0  (pads 37/36) to test points only
//! Avoided: strapping pins IO3/IO45/IO46, and IO35-37 (used by octal PSRAM on R8 modules).

use pcbgen::circuit::{Circuit, NetId};

const R0805: &str = "Resistor_SMD:R_0805_2012Metric";
const C0805: &str = "Capacitor_SMD:C_0805_2012Metric";
const LED0805: &str = "LED_SMD:LED_0805_2012Metric";
const TP: &str = "TestPoint:TestPoint_Pad_D1.5mm";

/// LCSC part numbers (JLCPCB Basic unless noted); see research/2026-09-29-parts-starter.md
fn lcsc(value: &str) -> &'static str {
    match value {
        "10k" => "C17414",
        "5.1k" => "C27834",
        "4.7k" => "C17673",
        "1k" => "C17513",
        "2.2k" => "C17520",
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
    let mut c = Circuit::new("starter", "ESP32-S3 starter board", "0");
    let (vbus, v5, v3, gnd) = (c.net("VBUS"), c.net("+5V"), c.net("+3V3"), c.net("GND"));
    let (dp, dm) = (c.net("USB_D+"), c.net("USB_D-"));
    let (sda, scl) = (c.net("I2C_SDA"), c.net("I2C_SCL"));

    // --- USB-C input -------------------------------------------------------
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
    // 5.1k pull-downs on each CC line tell the charger "I'm a USB device, give me 5 V"
    let (cc1, cc2) = (c.net("CC1"), c.net("CC2"));
    c.connect(cc1, j1, &["CC1"]);
    c.connect(cc2, j1, &["CC2"]);
    r(&mut c, "R1", "5.1k", cc1, gnd, usb);
    r(&mut c, "R2", "5.1k", cc2, gnd, usb);

    // ESD: 0.6 pF clamp on D+/D- (pins 1, 2 = I/O, 3 = GND) and a 200 W TVS on +5V (after the fuse).
    // Both JLCPCB Preferred Extended (no loading fee); see DECISIONS.md D-008.
    // The KiCad symbol is a generic dual TVS: only the pin numbers matter.
    let esd = c
        .part("U3", "Device:D_TVS_Dual_AAC", "H5VUT2U", "Package_TO_SOT_SMD:SOT-23")
        .lcsc("C20615824")
        .mpn("H5VUT2U")
        .block(usb)
        .id();
    c.connect(dp, esd, &["1"]);
    c.connect(dm, esd, &["2"]);
    c.connect(gnd, esd, &["3"]);
    let d3 = c
        .part("D3", "Diode:SMF5V0A", "SMF5.0A", "Diode_SMD:D_SOD-123F")
        .lcsc("C19077497")
        .mpn("SMF5.0A")
        .block(usb)
        .rot(90)
        .id();
    // after the fuse (on +5V): a faulty charger trips the fuse instead of burning the TVS
    c.connect(v5, d3, &["1"]); // cathode
    c.connect(gnd, d3, &["2"]);

    // 0.75 A hold: Bourns publishes a derating table (0.61 A at 50 C, 0.52 A at 60 C); a
    // 0.5 A PTC holds only ~0.35-0.40 A in a warm box, no margin over the ~0.35 A load (D-015)
    let f1 = c
        .part("F1", "Device:Polyfuse", "750mA", "Fuse:Fuse_1206_3216Metric")
        .lcsc("C89653")
        .mpn("MF-NSMF075-2")
        .block(usb)
        .rot(90)
        .id();
    c.connect(vbus, f1, &["1"]);
    c.connect(v5, f1, &["2"]);

    // --- 3.3 V regulator ---------------------------------------------------
    // LDL1117S33R: ceramic-stable, 0.35 V dropout (D-007). KiCad has no LDL1117 symbol;
    // LD1117S33TR_SOT223 has the same SOT-223 pinout (1 GND, 2 OUT/tab, 3 IN).
    let reg = "3.3 V regulator";
    let u2 = c
        .part("U2", "Regulator_Linear:LD1117S33TR_SOT223", "LDL1117S33R", "Package_TO_SOT_SMD:SOT-223-3_TabPin2")
        .lcsc("C435835")
        .mpn("LDL1117S33R")
        .block(reg)
        .id();
    c.connect(v5, u2, &["VI"]);
    c.connect(v3, u2, &["VO"]);
    c.connect(gnd, u2, &["GND"]);
    cap(&mut c, "C1", "10u", v5, gnd, reg);
    // 1u at the regulator, 22u at the module (Espressif Fig. 9-1): keeps the total on 3V3
    // (~23 uF nominal, less at 3.3 V bias) at the edge of ST's 1-22 uF stability plot
    cap(&mut c, "C2", "1u", v3, gnd, reg);
    let pwr_led = c.net("PWR_LED");
    // green Vf is up to 3.1 V, too close to 3.3 V: feed it from 5 V (about 2 mA)
    r(&mut c, "R3", "1k", v5, pwr_led, reg);
    let d1 = c.part("D1", "Device:LED", "green", LED0805).lcsc("C2297").block(reg).id();
    c.connect(pwr_led, d1, &["A"]);
    c.connect(gnd, d1, &["K"]);

    // --- ESP32-S3 module ---------------------------------------------------
    let mcu = "ESP32-S3";
    let u1 = c
        .part("U1", "RF_Module:ESP32-S3-WROOM-1", "ESP32-S3-WROOM-1-N16R8", "pcbgen:ESP32-S3-WROOM-1_EPAD-Drill0.3")
        .lcsc("C2913202")
        .mpn("ESP32-S3-WROOM-1-N16R8")
        .block(mcu)
        .id();
    c.connect(v3, u1, &["3V3"]);
    c.connect(gnd, u1, &["GND"]);
    cap(&mut c, "C3", "22u", v3, gnd, mcu);
    cap(&mut c, "C4", "100n", v3, gnd, mcu);
    c.connect(dm, u1, &["USB_D-"]);
    c.connect(dp, u1, &["USB_D+"]);

    let (en, boot) = (c.net("EN"), c.net("BOOT"));
    c.connect(en, u1, &["EN"]);
    r(&mut c, "R4", "10k", v3, en, mcu); // EN pull-up
    cap(&mut c, "C5", "1u", en, gnd, mcu); // EN power-on delay (Espressif: 10k/1uF)
    let sw1 = c.part("SW1", "Switch:SW_Push", "RESET", "Button_Switch_SMD:SW_Push_1P1T_XKB_TS-1187A").lcsc("C318884").block("Buttons").id();
    c.connect(en, sw1, &["1"]);
    c.connect(gnd, sw1, &["2"]);
    c.connect(boot, u1, &["IO0"]);
    r(&mut c, "R5", "10k", v3, boot, "Buttons");
    let sw2 = c.part("SW2", "Switch:SW_Push", "BOOT", "Button_Switch_SMD:SW_Push_1P1T_XKB_TS-1187A").lcsc("C318884").block("Buttons").id();
    c.connect(boot, sw2, &["1"]);
    c.connect(gnd, sw2, &["2"]);

    c.connect(sda, u1, &["IO1"]);
    c.connect(scl, u1, &["IO2"]);
    let (status, led) = (c.net("STATUS"), c.net("STATUS_LED"));
    c.connect(status, u1, &["IO48"]);
    r(&mut c, "R6", "1k", status, led, "Status LED");
    let d2 = c.part("D2", "Device:LED", "red", LED0805).lcsc("C84256").block("Status LED").id();
    c.connect(led, d2, &["A"]);
    c.connect(gnd, d2, &["K"]);

    let (tx, rx) = (c.net("UART_TX"), c.net("UART_RX"));
    c.connect(tx, u1, &["TXD0"]);
    c.connect(rx, u1, &["RXD0"]);

    let used = ["GND", "3V3", "EN", "IO0", "IO1", "IO2", "IO48", "USB_D-", "USB_D+", "TXD0", "RXD0"];
    let mut unused: Vec<String> = c.pin_names(u1).into_iter().filter(|n| !used.contains(&n.as_str())).collect();
    unused.sort();
    unused.dedup();
    c.nc(u1, &unused.iter().map(String::as_str).collect::<Vec<_>>());

    // --- Sensor + Qwiic ----------------------------------------------------
    let sens = "Sensor + Qwiic";
    let u4 = c
        .part("U4", "Sensor_Humidity:SHT4x", "SHT40-AD1B", "Sensor_Humidity:Sensirion_DFN-4_1.5x1.5mm_P0.8mm_SHT4x_NoCentralPad")
        .lcsc("C2909890")
        .mpn("SHT40-AD1B-R2")
        .block(sens)
        .id();
    c.connect(v3, u4, &["VDD"]);
    c.connect(gnd, u4, &["VSS"]);
    c.connect(sda, u4, &["SDA"]);
    c.connect(scl, u4, &["SCL"]);
    cap(&mut c, "C6", "100n", v3, gnd, sens);
    r(&mut c, "R7", "4.7k", v3, sda, sens);
    r(&mut c, "R8", "4.7k", v3, scl, sens);
    // Qwiic pinout (SparkFun): 1 GND, 2 3.3V, 3 SDA, 4 SCL
    let j2 = c
        .part(
            "J2",
            "Connector_Generic_MountingPin:Conn_01x04_MountingPin",
            "Qwiic",
            "Connector_JST:JST_SH_SM04B-SRSS-TB_1x04-1MP_P1.00mm_Horizontal",
        )
        .lcsc("C160404")
        .mpn("SM04B-SRSS-TB")
        .block(sens)
        .id();
    c.connect(gnd, j2, &["1"]);
    c.connect(v3, j2, &["2"]);
    c.connect(sda, j2, &["3"]);
    c.connect(scl, j2, &["4"]);
    c.nc(j2, &["MP"]);

    // --- Test points and mounting holes -------------------------------------
    for (i, net) in [v5, v3, gnd, tx, rx, sda, scl].into_iter().enumerate() {
        let name = c.net_name(net).to_string();
        let tp = c.part(&format!("TP{}", i + 1), "Connector:TestPoint", &name, TP).block("Test points").not_in_bom().id();
        c.connect(net, tp, &["1"]);
    }
    for i in 1..=4 {
        c.part(&format!("H{i}"), "Mechanical:MountingHole", "M3", "MountingHole:MountingHole_3.2mm_M3")
            .block("Mounting holes")
            .not_in_bom();
    }

    c.pwr_flag(&[vbus, v5, gnd]);
    c
}
