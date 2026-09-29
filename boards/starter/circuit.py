"""ESP32-S3 starter board, Rev 0: the circuit.

USB-C (power + native USB) -> PTC fuse -> 3.3 V LDO -> ESP32-S3-WROOM-1.
SHT40 temperature/humidity sensor and a Qwiic connector share one I2C bus.
EN (reset) and BOOT buttons, power LED, status LED, test points.

GPIO choices (pins are module pads, see HARDWARE_LESSONS.md pinout table):
  USB D-/D+  IO19/IO20  (pads 13/14, fixed by the chip)
  I2C SDA    IO1        (pad 39, right edge, next to the sensor)
  I2C SCL    IO2        (pad 38)
  Status LED IO48       (pad 25, bottom row)
  BOOT       IO0        (pad 27, strapping: low at reset = download mode)
  UART0      TXD0/RXD0  (pads 37/36) to test points only
Avoided: strapping pins IO3/IO45/IO46, and IO35-37 (used by octal PSRAM on R8 modules).
"""

from pcbgen.circuit import Circuit

R0805 = "Resistor_SMD:R_0805_2012Metric"
C0805 = "Capacitor_SMD:C_0805_2012Metric"
LED0805 = "LED_SMD:LED_0805_2012Metric"
TP = "TestPoint:TestPoint_Pad_D1.5mm"

# LCSC part numbers (JLCPCB Basic unless noted); see research/2026-09-29-parts-starter.md
LCSC = {
    "10k": "C17414", "5.1k": "C27834", "4.7k": "C17673", "1k": "C17513", "2.2k": "C17520",
    "100n": "C49678", "1u": "C28323", "10u": "C15850", "22u": "C45783",
}


def build() -> Circuit:
    c = Circuit("starter", title="ESP32-S3 starter board", rev="0")
    vbus, v5, v3, gnd = c.net("VBUS"), c.net("+5V"), c.net("+3V3"), c.net("GND")
    dp, dm = c.net("USB_D+"), c.net("USB_D-")
    sda, scl = c.net("I2C_SDA"), c.net("I2C_SCL")

    def R(ref, val, a, b, block, **kw):
        r = c.part(ref, "Device:R", val, R0805, lcsc=LCSC[val], block=block, rot=90, **kw)
        a += r[1]; b += r[2]
        return r

    def C(ref, val, a, b, block):
        k = c.part(ref, "Device:C", val, C0805, lcsc=LCSC[val], block=block)
        a += k[1]; b += k[2]
        return k

    # --- USB-C input -------------------------------------------------------
    j1 = c.part("J1", "Connector:USB_C_Receptacle_USB2.0_16P", "USB-C",
                "Connector_USB:USB_C_Receptacle_HRO_TYPE-C-31-M-12",
                lcsc="C165948", mpn="TYPE-C-31-M-12", block="USB-C input")
    vbus += j1["VBUS"]
    gnd += j1["GND"]
    gnd += j1["SHIELD"]
    dp += j1["D+"]
    dm += j1["D-"]
    j1.nc("SBU1", "SBU2")
    # 5.1k pull-downs on each CC line tell the charger "I'm a USB device, give me 5 V"
    cc1, cc2 = c.net("CC1"), c.net("CC2")
    cc1 += j1["CC1"]
    cc2 += j1["CC2"]
    R("R1", "5.1k", cc1, gnd, "USB-C input")
    R("R2", "5.1k", cc2, gnd, "USB-C input")

    # ESD: 0.6 pF clamp on D+/D- (pins 1, 2 = I/O, 3 = GND) and a 200 W TVS on +5V (after the fuse).
    # Both JLCPCB Preferred Extended (no loading fee); see DECISIONS.md D-008.
    # The KiCad symbol is a generic dual TVS: only the pin numbers matter.
    esd = c.part("U3", "Device:D_TVS_Dual_AAC", "H5VUT2U", "Package_TO_SOT_SMD:SOT-23",
                 lcsc="C20615824", mpn="H5VUT2U", block="USB-C input")
    dp += esd["1"]
    dm += esd["2"]
    gnd += esd["3"]
    d3 = c.part("D3", "Diode:SMF5V0A", "SMF5.0A", "Diode_SMD:D_SOD-123F",
                lcsc="C19077497", mpn="SMF5.0A", block="USB-C input", rot=90)
    # after the fuse (on +5V): a faulty charger trips the fuse instead of burning the TVS
    v5 += d3["1"]        # cathode
    gnd += d3["2"]

    f1 = c.part("F1", "Device:Polyfuse", "500mA", "Fuse:Fuse_1206_3216Metric",
                lcsc="C720075", mpn="JK-nSMD050-30", block="USB-C input", rot=90)
    vbus += f1[1]
    v5 += f1[2]

    # --- 3.3 V regulator ---------------------------------------------------
    # LDL1117S33R: ceramic-stable, 0.35 V dropout (D-007). KiCad has no LDL1117 symbol;
    # LD1117S33TR_SOT223 has the same SOT-223 pinout (1 GND, 2 OUT/tab, 3 IN).
    u2 = c.part("U2", "Regulator_Linear:LD1117S33TR_SOT223", "LDL1117S33R",
                "Package_TO_SOT_SMD:SOT-223-3_TabPin2", lcsc="C435835", mpn="LDL1117S33R",
                block="3.3 V regulator")
    v5 += u2["VI"]
    v3 += u2["VO"]
    gnd += u2["GND"]
    C("C1", "10u", v5, gnd, "3.3 V regulator")
    # 1u at the regulator, 22u at the module (Espressif Fig. 9-1): keeps the total on 3V3
    # (~23 uF nominal, less at 3.3 V bias) at the edge of ST's 1-22 uF stability plot
    C("C2", "1u", v3, gnd, "3.3 V regulator")
    pwr_led = c.net("PWR_LED")
    # green Vf is up to 3.1 V, too close to 3.3 V: feed it from 5 V (about 2 mA)
    R("R3", "1k", v5, pwr_led, "3.3 V regulator")
    d1 = c.part("D1", "Device:LED", "green", LED0805, lcsc="C2297", block="3.3 V regulator")
    pwr_led += d1["A"]
    gnd += d1["K"]

    # --- ESP32-S3 module ---------------------------------------------------
    u1 = c.part("U1", "RF_Module:ESP32-S3-WROOM-1", "ESP32-S3-WROOM-1-N16R8",
                "RF_Module:ESP32-S3-WROOM-1", lcsc="C2913202", mpn="ESP32-S3-WROOM-1-N16R8",
                block="ESP32-S3")
    v3 += u1["3V3"]
    gnd += u1["GND"]
    C("C3", "22u", v3, gnd, "ESP32-S3")
    C("C4", "100n", v3, gnd, "ESP32-S3")
    dm += u1["USB_D-"]
    dp += u1["USB_D+"]

    en, boot = c.net("EN"), c.net("BOOT")
    en += u1["EN"]
    R("R4", "10k", v3, en, "ESP32-S3")          # EN pull-up
    C("C5", "1u", en, gnd, "ESP32-S3")          # EN power-on delay (Espressif: 10k/1uF)
    sw1 = c.part("SW1", "Switch:SW_Push", "RESET", "Button_Switch_SMD:SW_Push_1P1T_XKB_TS-1187A",
                 lcsc="C318884", block="Buttons")
    en += sw1[1]; gnd += sw1[2]
    boot += u1["IO0"]
    R("R5", "10k", v3, boot, "Buttons")
    sw2 = c.part("SW2", "Switch:SW_Push", "BOOT", "Button_Switch_SMD:SW_Push_1P1T_XKB_TS-1187A",
                 lcsc="C318884", block="Buttons")
    boot += sw2[1]; gnd += sw2[2]

    sda += u1["IO1"]
    scl += u1["IO2"]
    status, led = c.net("STATUS"), c.net("STATUS_LED")
    status += u1["IO48"]
    R("R6", "1k", status, led, "Status LED")
    d2 = c.part("D2", "Device:LED", "red", LED0805, lcsc="C84256", block="Status LED")
    led += d2["A"]; gnd += d2["K"]

    tx, rx = c.net("UART_TX"), c.net("UART_RX")
    tx += u1["TXD0"]; rx += u1["RXD0"]

    used = {"GND", "3V3", "EN", "IO0", "IO1", "IO2", "IO48", "USB_D-", "USB_D+", "TXD0", "RXD0"}
    u1.nc(*sorted({p.name for p in u1.symbol.pins} - used))

    # --- Sensor + Qwiic ----------------------------------------------------
    u4 = c.part("U4", "Sensor_Humidity:SHT4x", "SHT40-AD1B",
                "Sensor_Humidity:Sensirion_DFN-4_1.5x1.5mm_P0.8mm_SHT4x_NoCentralPad",
                lcsc="C2909890", mpn="SHT40-AD1B-R2", block="Sensor + Qwiic")
    v3 += u4["VDD"]; gnd += u4["VSS"]; sda += u4["SDA"]; scl += u4["SCL"]
    C("C6", "100n", v3, gnd, "Sensor + Qwiic")
    R("R7", "4.7k", v3, sda, "Sensor + Qwiic")
    R("R8", "4.7k", v3, scl, "Sensor + Qwiic")
    # Qwiic pinout (SparkFun): 1 GND, 2 3.3V, 3 SDA, 4 SCL
    j2 = c.part("J2", "Connector_Generic_MountingPin:Conn_01x04_MountingPin", "Qwiic",
                "Connector_JST:JST_SH_SM04B-SRSS-TB_1x04-1MP_P1.00mm_Horizontal",
                lcsc="C160404", mpn="SM04B-SRSS-TB", block="Sensor + Qwiic")
    gnd += j2["1"]; v3 += j2["2"]; sda += j2["3"]; scl += j2["4"]
    j2.nc("MP")

    # --- Test points and mounting holes -------------------------------------
    for i, net in enumerate([v5, v3, gnd, tx, rx, sda, scl], 1):
        tp = c.part(f"TP{i}", "Connector:TestPoint", net.name, TP, block="Test points", in_bom=False)
        net += tp[1]
    for i in range(1, 5):
        c.part(f"H{i}", "Mechanical:MountingHole", "M3", "MountingHole:MountingHole_3.2mm_M3",
               block="Mounting holes", in_bom=False)

    c.pwr_flag(vbus, v5, gnd)
    return c
