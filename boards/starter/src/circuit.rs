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

use pcbgen::blocks::{self, Esp32s3Core, LED0805, Ldo3v3, UsbCPower, capacitor as cap, resistor as r};
use pcbgen::circuit::Circuit;

const TP: &str = "TestPoint:TestPoint_Pad_D1.5mm";

pub fn build() -> Circuit {
    let mut c = Circuit::new("starter", "ESP32-S3 starter board", "0");
    let (vbus, v5, v3, gnd) = (c.net("VBUS"), c.net("+5V"), c.net("+3V3"), c.net("GND"));
    let (dp, dm) = (c.net("USB_D+"), c.net("USB_D-"));
    let (sda, scl) = (c.net("I2C_SDA"), c.net("I2C_SCL"));

    // --- USB-C input: J1, CC pull-downs R1/R2, ESD U3, TVS D3 after the fuse F1 ----------
    blocks::usb_c_power(&mut c, &UsbCPower::new(vbus, gnd, dp, dm).fused(v5));

    // --- 3.3 V regulator: U2 (LDL1117S33R), C1 in, C2 out ------------------------------
    let reg = "3.3 V regulator";
    blocks::ldo_3v3(&mut c, &Ldo3v3::new(v5, v3, gnd));
    let pwr_led = c.net("PWR_LED");
    // green Vf is up to 3.1 V, too close to 3.3 V: feed it from 5 V (about 2 mA)
    r(&mut c, "R3", "1k", v5, pwr_led, reg);
    let d1 = c.part("D1", "Device:LED", "green", LED0805).lcsc("C2297").mpn("KT-0805G").block(reg).id();
    c.connect(pwr_led, d1, &["A"]);
    c.connect(gnd, d1, &["K"]);

    // --- ESP32-S3 module: U1, C3/C4 on 3V3, EN R4/C5, RESET SW1, BOOT R5/SW2 ---------------
    let (en, boot) = (c.net("EN"), c.net("BOOT"));
    let u1 = blocks::esp32s3_core(&mut c, &Esp32s3Core::new(v3, gnd, en).usb(dp, dm).boot(boot)).module;

    c.connect(sda, u1, &["IO1"]);
    c.connect(scl, u1, &["IO2"]);
    let (status, led) = (c.net("STATUS"), c.net("STATUS_LED"));
    c.connect(status, u1, &["IO48"]);
    r(&mut c, "R6", "1k", status, led, "Status LED");
    let d2 = c.part("D2", "Device:LED", "red", LED0805).lcsc("C84256").mpn("NCD0805R1").block("Status LED").id();
    c.connect(led, d2, &["A"]);
    c.connect(gnd, d2, &["K"]);

    let (tx, rx) = (c.net("UART_TX"), c.net("UART_RX"));
    c.connect(tx, u1, &["TXD0"]);
    c.connect(rx, u1, &["RXD0"]);

    blocks::nc_unconnected(&mut c, u1);

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
