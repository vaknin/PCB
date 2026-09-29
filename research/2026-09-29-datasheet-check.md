# Starter board: independent datasheet check (2026-09-29)

Reviewer: independent pass over `boards/starter/circuit.py`. I did not build this design.

How this was checked:
- The datasheets were downloaded and read with `pdftotext`.
- Where a pinout exists only as a figure, I rendered the page with `pdftoppm` and read the image. These are marked "(figure)".
- The KiCad symbol pin numbers were parsed from `/usr/share/kicad/symbols` and the footprint pads from `/usr/share/kicad/footprints` (KiCad 10.0.6).
- The pad-to-net mapping was read from the generated `boards/starter/kicad/starter.kicad_pcb`, so every "design does" entry below is what actually gets soldered.

Sources used:
- **[ESP-M]** Espressif, *ESP32-S3-WROOM-1 & WROOM-1U Datasheet v1.8*: https://www.espressif.com/sites/default/files/documentation/esp32-s3-wroom-1_wroom-1u_datasheet_en.pdf
- **[ESP-C]** Espressif, *ESP32-S3 Series Datasheet v2.2*: https://www.espressif.com/sites/default/files/documentation/esp32-s3_datasheet_en.pdf
- **[SHT]** Sensirion, *SHT4x Datasheet, Version 2, July 2021*, LCSC mirror of the Sensirion PDF: https://wmsc.lcsc.com/wmsc/upload/file/pdf/v2/lcsc/2110211930_Sensirion-SHT40-AD1B-R2_C2909890.pdf
- **[LDL]** ST, *LDL1117 datasheet, DocID030319 Rev 3, July 2017*, LCSC mirror: https://datasheet.lcsc.com/datasheet/pdf/29f4e8f2c569cb1b438995f2c8fccc58.pdf?productCode=C435835
  - The st.com download timed out. A newer ST revision may exist.
- **[HRO]** HRO, *TYPE-C-31-M-12 drawing, 2020-12-08*, LCSC mirror: https://datasheet.lcsc.com/datasheet/pdf/9e56b777c022540fcce7c7f67825f55e.pdf?productCode=C165948
- **[H5]** Hongjiacheng, *H5VUT2U datasheet Rev 2.0*: https://wmsc.lcsc.com/wmsc/upload/file/pdf/v2/lcsc/2401261525_hongjiacheng-H5VUT2U_C20615824.pdf
- **[SMF]** Hongjiacheng, *SMF series datasheet Rev 2.2*: https://wmsc.lcsc.com/wmsc/upload/file/pdf/v2/lcsc/2312041000_hongjiacheng-SMF5-0A_C19077497.pdf
- **[PTC]** Jinrui, *JK-nSMD050-30, Edition A0*: https://datasheet.lcsc.com/datasheet/pdf/6f49f3a34f7d92e0807665ecdbe3a4f4.pdf?productCode=C720075
- **[QW]** SparkFun:
  - Qwiic page: https://www.sparkfun.com/qwiic (cable colours; board connector SM04B-SRSS-TB(LF)(SN))
  - SparkFun's own Qwiic Adapter Eagle schematic: https://github.com/sparkfunX/Qwiic_Adapter (`Hardware/SparkFun Qwiic Adapter.sch`, device `I2C_STANDARD`, variant `JS-1MM`)

## Results

| Part | Claim | Verdict | Source | Notes |
|---|---|---|---|---|
| ESP32-S3-WROOM-1 | Pad 1 GND, 2 3V3, 3 EN | VERIFIED | [ESP-M] Table 3-1, p.11 | PCB: U1 pad 1 = GND, 2 = +3V3, 3 = /EN. |
| ESP32-S3-WROOM-1 | Pad 13 = IO19 = USB_D-, pad 14 = IO20 = USB_D+ | VERIFIED | [ESP-M] Table 3-1 (pad 13 lists "USB_D-", pad 14 "USB_D+"); [ESP-C] pin table, chip pins 25/26 = USB_D-/USB_D+, and GPIO20 carries USB_PU (the D+ pull-up) | PCB: pad 13 = /USB_D-, pad 14 = /USB_D+. |
| ESP32-S3-WROOM-1 | Pad 25 = IO48 | VERIFIED | [ESP-M] Table 3-1 | Footnote c says IO48 is 1.8 V only on R16V modules. N16R8 is a 3.3 V part, so the 3.3 V LED drive is fine. |
| ESP32-S3-WROOM-1 | Pad 27 = IO0 (BOOT strap) | VERIFIED | [ESP-M] Table 3-1; Table 4-3 p.14 | GPIO0 = 1 gives SPI boot and 0 gives download mode. The design has a 10 kΩ pull-up plus a button to GND. Correct. |
| ESP32-S3-WROOM-1 | Pad 36 = RXD0, 37 = TXD0 | VERIFIED | [ESP-M] Table 3-1 p.12 | RXD0 is GPIO44 and TXD0 is GPIO43. PCB: 36 = /UART_RX, 37 = /UART_TX. |
| ESP32-S3-WROOM-1 | Pad 38 = IO2, 39 = IO1 | VERIFIED | [ESP-M] Table 3-1 p.12 | PCB: 38 = SCL, 39 = SDA. Both are plain GPIOs, not strapping pins. |
| ESP32-S3-WROOM-1 | Pad 40 GND, pad 41 EPAD = GND | VERIFIED | [ESP-M] Table 3-1 p.12 | The KiCad symbol names pin 41 "GND". Every footprint pad "41" (the EPAD plus its via array) is on GND in the PCB. [ESP-M] §9 says soldering the EPAD is optional but improves thermals. |
| ESP32-S3-WROOM-1 | EN RC = 10 kΩ / 1 µF | VERIFIED | [ESP-M] §9 Peripheral Schematics, p.41: "recommended setting for the RC delay circuit is usually R = 10 kΩ and C = 1 µF" | The design uses R4 10 k to 3V3 and C5 1 µF to GND, with the reset button across C5. Matches. |
| ESP32-S3-WROOM-1 | Strapping pins IO3, IO45, IO46 left unconnected is safe | VERIFIED (with default eFuses) | [ESP-M] Table 4-1 p.13, Tables 4-3/4-4/4-5 pp.14-15 | See the breakdown below this table. |
| ESP32-S3-WROOM-1 | USB D+/D- not swapped end-to-end | VERIFIED | [HRO] pin table; [ESP-M] Table 3-1; PCB netlist | J1 A6/B6 (DP1/DP2) → /USB_D+ → U1 pad 14 (USB_D+). J1 A7/B7 (DN1/DN2) → /USB_D- → U1 pad 13 (USB_D-). |
| ESP32-S3-WROOM-1 | 3V3 decoupling | VERIFIED, OK | [ESP-M] Fig. 9-1 shows 22 µF + 0.1 µF on 3V3 | The design has 22 µF (C2, at the LDO), 10 µF + 100 nF (C3, C4, at the module). The total meets the recommendation. Placement matters: C3/C4 must sit near pad 2. |
| SHT40 | 1 SDA, 2 SCL, 3 VDD, 4 VSS | VERIFIED | [SHT] §5.4 table, p.13 | The KiCad symbol `Sensor_Humidity:SHT4x` has the same numbers. PCB: U4 1 = SDA, 2 = SCL, 3 = +3V3, 4 = GND. |
| SHT40 | Footprint pad numbering matches the part | VERIFIED | [SHT] Fig. 11 (figure, transparent top view): 1 top-left, 2 bottom-left, 3 bottom-right, 4 top-right; Fig. 10: 1.4 mm × 0.8 mm pitch | KiCad pads: 1 (-0.7,-0.4), 2 (-0.7,0.4), 3 (0.7,0.4), 4 (0.7,-0.4), with Y down. That is the same counter-clockwise order and pitch. The pin-1 silk triangle is at top-left. |
| SHT40 | I2C address 0x44 | VERIFIED | [SHT] p.1 ordering list: "SHT40-AD1B … 0x44 I2C addr." | |
| SHT40 | Pull-ups and decoupling | VERIFIED, OK | [SHT] Fig. 1 p.3: 10 kΩ pull-ups, 100 nF VDD cap; Table p.7: R_pullup ≥ 390 Ω at VDD ≥ 1.62 V | The design uses 4.7 kΩ pull-ups (R7, R8) and 100 nF (C6), which is within spec. With a Qwiic board's own pull-ups in parallel (typically 2.2 k to 4.7 k, **inferred**), the total stays far above 390 Ω. |
| LDL1117S33R | Pin 1 GND, 2 OUT (and tab), 3 IN | VERIFIED | [LDL] Fig. 2 and Table 1, p.6: "The tab is connected to VOUT" | KiCad `LD1117S33TR_SOT223` extends `AP1117-15`: pin 1 GND, 2 VO, 3 VI. The footprint `SOT-223-3_TabPin2` has pads 1, 2, 2 (tab), 3. PCB: U2 1 = GND, 2 and tab = +3V3, 3 = +5V. Correct. |
| LDL1117S33R | Input cap ≥ 1 µF | VERIFIED, met | [LDL] §6.2 p.10 | C1 is 10 µF X5R on +5V. |
| LDL1117S33R | Output cap: 22 µF + 10 µF + 100 nF ceramic is stable | **UNVERIFIABLE (outside characterized range)** | [LDL] §6.2 p.10; Fig. 20/21 "Stability plan" p.14 (figure) | See the breakdown below this table. |
| LDL1117S33R | Dropout at 0.5 A | VERIFIED (typical only) | [LDL] Table 4 p.9: 350 mV typ / 600 mV max at 1.2 A; Fig. 12 p.12 (figure) | Fig. 12 shows about 200 mV at 600 mA and 25 °C, and about 290 mV at 125 °C. So at 0.5 A dropout is under about 0.3 V typical; this is interpolated. There is no guaranteed max at 0.5 A; the only guaranteed max is 600 mV at 1.2 A. |
| HRO TYPE-C-31-M-12 | VBUS A4/A9/B4/B9; GND A1/A12/B1/B12; CC1 A5; CC2 B5; D+ A6/B6; D- A7/B7; SBU A8/B8 | VERIFIED | [HRO] pin table (figure): A6 DP1, A7 DN1, B6 DP2, B7 DN2, A5 CC1, B5 CC2, etc.; "Recommend PCB layout" pad order (figure) | KiCad footprint pad X order: A1/B12, A4/B9, B8, A5, B7, A6, A7, B6, A8, B5, A9/B4, B1/A12. This is identical to HRO's layout drawing. PCB nets match: A6 and B6 both /USB_D+, A7 and B7 both /USB_D-, all four VBUS pads on VBUS, all four GND pads on GND, SBU unconnected. |
| HRO TYPE-C-31-M-12 | Shield | VERIFIED (as designed) | KiCad footprint: 4 × "SH" plated holes | All four SH pads are on GND. Tying the shield to GND directly is common practice and not wrong. |
| HRO TYPE-C-31-M-12 | 5.1 kΩ pull-down on each of CC1 and CC2, not shared | VERIFIED (design) | circuit.py R1/R2; PCB J1 A5 = /CC1, B5 = /CC2 (separate nets) | Two separate resistors to GND. 5.1 kΩ is the USB Type-C Rd value; I did not re-read the Type-C spec itself in this pass. |
| H5VUT2U | Pins 1 and 2 = I/O, pin 3 = GND | **VERIFIED** (was INFERRED) | [H5] p.2 Electrical Characteristics text: clamping "pin1 or pin2 to pin3", capacitance "I/O pin to GND"; p.1 function diagram (figure) | The figure shows pin 3 on the common rail. The steering diodes from pins 1/2 and the TVS all return to pin 3. PCB: U3 1 = D+, 2 = D-, 3 = GND. |
| H5VUT2U | KiCad symbol | OK (cosmetic) | KiCad `Device:D_TVS_Dual_AAC`: pin 1 A1, 2 A2, 3 common | This is a bidirectional-dual symbol standing in for a steering array. Only the pin numbers go to copper, and they match. The schematic drawing is not an accurate picture of the part. |
| H5VUT2U | Suitable for USB FS | OK | [H5] p.2: 0.6 pF typ / 0.8 pF max I/O-GND, VRWM 5 V | Fine for 12 Mbit/s full speed. |
| SMF5.0A | Cathode to VBUS; KiCad pad 1 = cathode on D_SOD-123F | VERIFIED | [SMF] p.2: unidirectional parts carry a cathode band; KiCad `D_SOD-123F.kicad_mod`: pad 1 at x = -1.4 with the closed silk bracket (x = -2.21) on the pad-1 side; `Diode:SM6T6V8A` graphics put the cathode bar on pin 1 | PCB: D3 1 = VBUS, 2 = GND. Correct orientation. Check the JLCPCB placement preview so the band lands on pad 1. |
| SMF5.0A | Ratings | VERIFIED | [SMF] p.1-2 | VRWM 5.0 V, VBR 6.4-7.0 V at 10 mA, VC 9.2 V at 21.7 A, 200 W (10/1000 µs), IR ≤ 400 µA at 5 V. |
| Qwiic (SM04B-SRSS-TB) | Pin 1 GND, 2 3.3 V, 3 SDA, 4 SCL | VERIFIED | [QW] SparkFun Qwiic Adapter Eagle schematic: device `I2C_STANDARD` / `JS-1MM` connects GND → pad 1, VCC → pad 2, SDA → pad 3, SCL → pad 4 on package `1X04_1MM_RA`; sparkfun.com/qwiic gives the colours (black GND, red 3.3 V, blue SDA, yellow SCL) | SparkFun's pad 1 is at the left with the mounting tabs behind the pins (top view). KiCad's `JST_SH_SM04B-SRSS-TB` has pad 1 at x = -1.5 and MP at y = +1.875: the same geometry, so physical pin 1 matches. PCB: J2 1 = GND, 2 = +3V3, 3 = SDA, 4 = SCL. SparkFun's web pages list colours but not pin numbers; the numbering comes from their design file. |
| Power LED D1 | 1 kΩ from +5V, green | OK | [LCSC listing KT-0805G] Vf 2.6-3.1 V (from research doc, not re-read) | This gives (5 − 3.1…2.6) / 1 k = 1.9-2.4 mA, a sensible brightness. KiCad LED pad 1 = K goes to GND. I did not open the LED datasheet to confirm the cathode mark (**inferred** from KiCad convention). |
| Status LED D2 | 1 kΩ from IO48, red | OK | ESP32-S3 GPIO drive is far above 2 mA | About 1.3-1.5 mA, which is visible. |
| Fuse F1 | 0.5 A hold PTC vs load | OK at room temperature; margin thin when hot (**inferred**) | [PTC] Table 2: I_hold 0.50 A, I_trip 1.00 A at 25 °C; R_typ 0.30 Ω, R1max 1.0 Ω; no derating table given. [ESP-M] p.28: TX 802.11b at 20.5 dBm = 355 mA; p.27: I_VDD supply ≥ 0.5 A | Peak load is about 0.36 A plus a few mA, below the 0.5 A hold at 25 °C. See Problems #2 for the heat and voltage-drop details. |
| Decoupling overall | | OK | | All three ICs are decoupled: LDO in/out, module 10 µF + 100 nF, sensor 100 nF. |
| Other | | | | IO35-37 (PSRAM on R8) are left unconnected, which is correct per [ESP-M] Table 3-1 note b. The BOOT button needs no RC. |

Strapping pins, detail ([ESP-M] Table 4-1 p.13 and Tables 4-3/4-4/4-5 pp.14-15):

- **GPIO46:** the internal weak pull-down gives 0. SPI boot ignores GPIO46, and download mode requires GPIO46 = 0, which is what it gets.
- **GPIO45:** the internal pull-down gives 0, which selects VDD_SPI = 3.3 V. That is the right voltage for N16R8, whose flash and PSRAM are 3.3 V.
- **GPIO3:** it floats with no internal pull. It only matters if EFUSE_STRAP_JTAG_SEL is burned; Table 4-5 lists GPIO3 as "Ignored" with factory eFuses. Leaving it unconnected is safe unless someone burns that eFuse.
- **Design:** all three are unconnected. Correct.

LDL1117 output capacitor, detail ([LDL] §6.2 p.10; Fig. 20/21 p.14):

- **What ST says:** the part is "designed to work with an output ceramic capacitor", and ST suggests 1 µF in / 4.7 µF out.
- **The stability plot stops at 22 µF.** The Fig. 20 (5 V) plot's x-axis runs from 1 to **22 µF nominal**. Fig. 21 (1.2 V) runs from 3.3 to 22 µF.
- **The ESR window:** the stable region is roughly ESR 0.01-0.02 Ω up to about 10 Ω at 100 kHz, read off a log plot, so approximate.
- **No limit is stated beyond the plot.** No maximum C_out is written anywhere. The datasheet simply does not characterise more than 22 µF nominal.
- **The board exceeds it.** The board has 22 + 10 + 0.1 = **32.1 µF nominal** on +3V3.
- **Effective capacitance is probably inside the range.** After DC-bias derating on 0805 X5R parts at 3.3 V, the effective total is probably under 22 µF (**inferred**; Samsung DC-bias curves not read).
- **Parallel ESR is a question.** Several MLCCs in parallel could also fall below the approximately 10 mΩ lower ESR edge; PCB traces add some back.

## Problems found

1. **The LDL1117 output capacitance is outside ST's published stability plot.**
   - The board has 32.1 µF nominal ceramic on +3V3 (C2 22 µF + C3 10 µF + C4 100 nF). ST's stability plan (Fig. 20/21) only characterises 1-22 µF nominal, with ESR down to about 10 mΩ.
   - It is probably fine once DC-bias derating is counted (**inferred**), but it is not verifiable from the datasheet.
   - Low-cost fix: change C2 from 22 µF to 10 µF. That gives 20.1 µF nominal, inside the plot, and C3 + C4 still sit at the module.
   - Otherwise, find a newer ST revision or an ST statement. Rev 3 (2017) was the only copy I could download; st.com timed out.
2. **The PTC fuse has thin margin when warm (inferred).**
   - The JK-nSMD050-30 holds 0.50 A only at 25 °C, and the datasheet gives no temperature derating table.
   - The R8 module is rated to 65 °C ambient. PTC hold current typically falls noticeably with temperature (**inferred**, not in this datasheet).
   - Wi-Fi TX peaks are 355 mA ([ESP-M]). This is not a pinout error, but in a warm enclosure a nuisance trip is possible.
   - The fuse's post-reflow resistance can be up to 1.0 Ω (R1max). That drops up to about 0.36 V at TX peak before the LDO. With VBUS at 4.75 V the LDO input is still about 4.4 V, which clears 3.3 V + ~0.3 V dropout. OK.
3. **The H5VUT2U schematic symbol is a stand-in (cosmetic).**
   - `Device:D_TVS_Dual_AAC` draws a bidirectional dual TVS, not a steering-diode array.
   - The copper is correct (1 = D+, 2 = D-, 3 = GND, now verified from the datasheet), but anyone reading the schematic sees the wrong device drawn.
4. **The LED cathode mark and the SMF5.0A band orientation depend on KiCad convention.**
   - The convention is pad 1 = cathode, and the netlist follows it.
   - I did not open the KT-0805G or NCD0805R1 datasheets to confirm their cathode marks.
   - Check all three polarised parts (D1, D2, D3) in the JLCPCB placement preview before ordering.
5. **SHT40 layout rule (not a pinout issue).**
   - Sensirion says "there shall be no copper under the sensor other than at the pin pads" ([SHT] Fig. 10, p.13).
   - Check that no pour, trace or via runs under U4 on the top layer. Keeping pour off the bottom layer under it too is better for heat isolation (**inferred**).

No pin-assignment errors were found. Every pad-to-net mapping listed in the brief matches the manufacturer documents.
