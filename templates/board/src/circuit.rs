//! __NAME__, Rev 0: the circuit. Made by `pcb new` from templates/board; edit freely.
//!
//! A minimal ESP32-S3 board from pcbgen's blocks (the starter board's checked parts):
//! USB-C (power + native USB) -> PTC fuse -> 3.3 V LDO -> ESP32-S3-WROOM-1-N16R8,
//! RESET and BOOT buttons, a power light (from 5 V) and a status light on IO48.
//!
//! GPIO choices (module pads; HARDWARE_LESSONS.md has the pinout):
//!   USB D-/D+  IO19/IO20  (pads 13/14, fixed by the chip)
//!   BOOT       IO0        (pad 27, strapping: low at reset = download mode)
//!   Status LED IO48       (pad 25)
//! Avoid: strapping pins IO3/IO45/IO46, IO35-37 (octal PSRAM on R8 modules).
//! Every GPIO the circuit uses needs a [[pin]] in board.toml (the BOARD.TOML gate).

use pcbgen::blocks::{self, Esp32s3Core, LED0805, Ldo3v3, UsbCPower, resistor};
use pcbgen::circuit::Circuit;

pub fn build() -> Circuit {
    let mut c = Circuit::new("__NAME__", "__NAME__", "0");
    let (vbus, v5, v3, gnd) = (c.net("VBUS"), c.net("+5V"), c.net("+3V3"), c.net("GND"));
    let (dp, dm) = (c.net("USB_D+"), c.net("USB_D-"));

    // USB-C input: J1, CC pull-downs R1/R2, ESD U3, TVS D3 after the 0.75 A fuse F1
    blocks::usb_c_power(&mut c, &UsbCPower::new(vbus, gnd, dp, dm).fused(v5));

    // 3.3 V regulator: U2 (LDL1117S33R), C1 in, C2 out; power light R3/D1 from 5 V
    // (green Vf is up to 3.1 V, too close to 3.3 V)
    let reg = "3.3 V regulator";
    blocks::ldo_3v3(&mut c, &Ldo3v3::new(v5, v3, gnd));
    let pwr_led = c.net("PWR_LED");
    resistor(&mut c, "R3", "1k", v5, pwr_led, reg);
    let d1 = c.part("D1", "Device:LED", "green", LED0805).lcsc("C2297").block(reg).id();
    c.connect(pwr_led, d1, &["A"]);
    c.connect(gnd, d1, &["K"]);

    // ESP32-S3 module: U1, C3/C4 on 3V3, EN R4/C5, RESET SW1, BOOT R5/SW2
    let (en, boot) = (c.net("EN"), c.net("BOOT"));
    let u1 = blocks::esp32s3_core(&mut c, &Esp32s3Core::new(v3, gnd, en).usb(dp, dm).boot(boot)).module;

    // Status light: IO48 high = on, red LED through 1k (~1.5 mA)
    let (status, led) = (c.net("STATUS"), c.net("STATUS_LED"));
    c.connect(status, u1, &["IO48"]);
    resistor(&mut c, "R6", "1k", status, led, "Status LED");
    let d2 = c.part("D2", "Device:LED", "red", LED0805).lcsc("C84256").block("Status LED").id();
    c.connect(led, d2, &["A"]);
    c.connect(gnd, d2, &["K"]);

    // ... the board's own parts go here ...

    blocks::nc_unconnected(&mut c, u1); // last: every module pin not used above

    for i in 1..=4 {
        c.part(&format!("H{i}"), "Mechanical:MountingHole", "M3", "MountingHole:MountingHole_3.2mm_M3")
            .block("Mounting holes")
            .not_in_bom();
    }

    c.pwr_flag(&[vbus, v5, gnd]);
    c
}
