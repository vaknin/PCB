# capture-clip: independent datasheet check (2026-09-30, Phase E.2)

Reviewer: independent pass over `boards/capture-clip/src/circuit.rs` and `board.toml`. I did not build this design.

How this was checked:
- Datasheets downloaded (LCSC's `wmsc.lcsc.com` mirror, jst-mfg.com, espressif.com) and read with `pdftotext -layout`. Where a fact is only in a figure, the page was rendered with `pdftoppm` and read as an image: marked "(figure)".
- KiCad symbol pin numbers parsed from `/usr/share/kicad/symbols`, footprint pads from `/usr/share/kicad/footprints` (KiCad 10.0.6).
- "Design does" is what `circuit.rs` connects (pin name/number → net). There is no `.kicad_pcb` yet, so pad → net is symbol pin number = footprint pad number.
- Stock and class: JLCPCB's parts-search API, queried 2026-09-30.
- The circuit changed while this ran (R19 4.3 kΩ PROG → GPIO7 was added); that row is included.

Could not open or did not do:
- richtek.com's own RT9080 PDF (returned an HTML page); the LCSC mirror is Richtek's DS9080-05, June 2017, so a newer revision may exist.
- TP4057: only the Chinese datasheet. The package-dimension page shows a drawing labelled "SOT-23-5" (a datasheet error); body dimensions were not compared.
- The ICS-43434's own port diameter (not looked up); the AOS SOT-23 outline document; EasyEDA's footprints as a second opinion; any battery in hand.
- LR's internal pull-down text (DS p.10 text layer is garbled; Table 8 itself is readable).

Sources:
- **[TP]** TOPPOWER TP4057 datasheet (Chinese, 12 pp.): https://wmsc.lcsc.com/wmsc/upload/file/pdf/v2/lcsc/1811021622_TOPPOWER-Nanjing-Extension-Microelectronics-TP4057-42-SOT26-R_C12044.pdf
- **[RT]** Richtek RT9080 DS9080-05, June 2017: https://wmsc.lcsc.com/wmsc/upload/file/pdf/v2/lcsc/2009192305_Richtek-Tech-RT9080-33GJ5_C841192.pdf
- **[AO]** AOS AO3401A Rev 3.1, Dec 2023: https://wmsc.lcsc.com/wmsc/upload/file/pdf/v2/lcsc/2412061733_Alpha---Omega-Semicon-AO3401A_C15127.pdf
- **[RB]** Hongjiacheng RB160M-30 Rev 2.1: https://wmsc.lcsc.com/wmsc/upload/file/pdf/v2/lcsc/2309051412_hongjiacheng-RB160M-30_C7502715.pdf
- **[JST]** JST PH catalogue: https://www.jst-mfg.com/product/pdf/eng/ePH.pdf ; **[JST-L]** the older JST PH sheet LCSC mirrors: https://wmsc.lcsc.com/wmsc/upload/file/pdf/v2/lcsc/2102031704_JST-S2B-PH-SM4-TB-LF-SN_C295747.pdf
- **[ADA]** Adafruit Feather ESP32-S3 board file (Eagle): https://github.com/adafruit/Adafruit-Feather-ESP32-S3-PCB (`Adafruit ESP32-S3 8MB No PSRAM.brd`, package `JSTPH2_BATT`, element X1); **[ADA-B]** https://www.adafruit.com/product/3898
- **[ICS]** TDK InvenSense ICS-43434 DS-000069 rev 1.2: https://wmsc.lcsc.com/wmsc/upload/file/pdf/v2/lcsc/2312010321_TDK-InvenSense-ICS-43434_C5656610.pdf
- **[LED]** Lite-On LTST-C19HE1WT DS22-2008-0044 rev C: https://wmsc.lcsc.com/wmsc/upload/file/pdf/v2/lcsc/2204251545_Lite-On-LTST-C19HE1WT_C458749.pdf
- **[SW1]** HYP 1TS009A drawing: https://wmsc.lcsc.com/wmsc/upload/file/pdf/v2/lcsc/1811151231_HYP--Hongyuan-Precision-1TS009A-1800-5000-CT_C319409.pdf ; **[SW2]** XUNPU TS-1088: https://wmsc.lcsc.com/wmsc/upload/file/pdf/v2/lcsc/2304140030_XUNPU-TS-1088-AR02016_C720477.pdf
- **[SMF]** Hongjiacheng SMF series Rev 2.2 (as in the starter check).
- **[ESP-M]** ESP32-S3-WROOM-1 datasheet v1.8; **[ESP-C]** ESP32-S3 series datasheet v2.2 (URLs as in `2026-09-29-datasheet-check.md`).

## Results

| Part | Claim | Verdict | Source | Notes |
|---|---|---|---|---|
| RB160M-30 D2 / `board.toml` sleep line | Reverse leakage at ~4 V is 3 µA | **WRONG** | [RB] Fig. 2 p.2 (figure, typical curves); table p.2: max 40 µA at 30 V, 25 °C only | Read off the 25 °C curve: about 5–6 µA at 4 V (20 µA at 30 V). The 100 °C curve is about 0.45 mA at 4 V. Interpolated on the log scale (**INFERRED**): ~13 µA at 40 °C, ~45 µA at 60 °C. No maximum at 4 V exists. With 5.5 µA the sleep sum is 8 + 4 + 2 + 1.4 + 5.5 = **20.9 µA, over the 20 µA budget**. See Problem 1. |
| Power path: R6 100 kΩ gate pull-down | Holds VBUS node (Q1 gate, VBUS_SENSE) near 0 V with USB absent | **WRONG when warm** | [RB] Fig. 2; [AO] p.2 Vgs(th) −0.5/−0.9/−1.3 V; [ESP-C] Table 5-4 VIH 0.75·VDD, VIL 0.25·VDD | D2's leakage flows into R6 ∥ (R16 + R17) = 71 kΩ. Node: ~0.4 V at 25 °C, ~0.9 V at 40 °C, ~3 V at 60 °C (**INFERRED** from the typical curves; the TP4057's VCC pin also loads the node by an unspecified amount). At 3 V: Q1's Vgs is only about −1 V (partly off, VSYS falls onto the body diode) and VBUS_SENSE sits at 1.8 V, between VIL 0.83 V and VIH 2.48 V (false USB wake possible). Fix: **R6 → 10 kΩ**. See Problem 1. |
| TP4057 CHRG on IO4 through R5 100 kΩ | "internal pull-up only while VBUS_SENSE is high" (`board.toml` pin CHRG; `circuit.rs` comment) | **WRONG** | [ESP-C] Table 5-4: R_PU 45 kΩ typ, VIL ≤ 0.25·VDD = 0.83 V; [TP] p.3: V_CHRG low 0.3 typ / 0.6 max | With the 45 kΩ pull-up on, a low CHRG gives 0.3 + 3.0 × 100/145 = **2.4 V at the pin: never a valid low**. The pull-up is not needed: D1 + R4 pull CHRG to VBUS when not charging, and R5 passes that to the pin. Fix: **no internal pull-up on IO4** (firmware + the `board.toml` note). STDBY on IO5 (direct) does need its pull-up and is fine. |
| TP4057 | 1 CHRG, 2 GND, 3 BAT, 4 VCC, 5 STDBY, 6 PROG | VERIFIED | [TP] p.3 package figure (figure), p.5 pin text | KiCad `Battery_Management:TP4057` has the same numbers. Design: 1 CHRG_N, 2 GND, 3 +BATT, 4 VBUS, 5 STDBY_N, 6 CHG_PROG. |
| TP4057 | SOT-23-6 footprint, pad order | VERIFIED (order), UNVERIFIED (body dimensions) | [TP] p.3 figure: pins 1-2-3 on one side, 6 opposite 1; JLCPCB lists "SOT-23-6" | KiCad `SOT-23-6`: pads 1–3 left top-down, 4–6 right bottom-up: same. The datasheet's dimension page is mislabelled SOT-23-5, so dimensions were not compared. |
| TP4057 | RPROG 10 kΩ → 100 mA | VERIFIED | [TP] p.3 table: 90/100/110 mA at RPROG = 10 k; p.7 table | |
| TP4057 | Capacitors: C1 1 µF on VCC, C2 10 µF on BAT | VERIFIED values; note on type | [TP] p.5 (VCC: "at least 1 µF"), p.12 circuits (1 µF / 10 µF), p.12 note 2, p.9, p.10 | The datasheet recommends electrolytic or tantalum: p.10 warns that ceramic input capacitors can ring to a high voltage on hot-plug; p.9 asks for 1 Ω in series with a large low-ESR ceramic on BAT when no battery is attached. With a cell attached the loop is stable without it. Both are ceramic here: see Problem 2. |
| TP4057 | CHRG / STDBY behaviour | VERIFIED | [TP] p.8 table, p.3–4: open drain, low 0.3/0.6 V at 5 mA, leakage ≤ 1 µA at 5 V in standby; CHRG abs. max 10 V | Charging: CHRG low. Full: STDBY low. Reversed cell or under-voltage: both off. **No battery with 10 µF on BAT: CHRG pulses every 0.5–2 s and STDBY is low** (firmware should expect that). D1 current into CHRG ~3 mA, under the 10 mA it can sink (p.10). |
| TP4057 | CHRG/STDBY leakage with VCC at 0 V | UNVERIFIED | [TP] gives leakage only in standby with VCC = 5 V | Settle by measuring IO4/IO5 current asleep. With pull-ups off and the VBUS node near 0 V there is nothing to drive current. |
| TP4057 | Battery-pin drain with USB absent: 2 µA max | VERIFIED | [TP] p.3: sleep mode, VCC = 0 V: −1 typ / −2 max µA | The `board.toml` line is right. |
| TP4057 + R19 (4.3 kΩ PROG → GPIO7, open drain) | Firmware-switchable 100 / ~300 mA | VERIFIED with conditions | [TP] p.3, p.5, p.7, p.9, p.10; [ESP-C] Tables 2-2, 5-4; [ADA-B] | See "Switchable charge current" below. |
| RT9080-33GJ5 | TSOT-23-5: 1 VIN, 2 GND, 3 EN, 4 NC, 5 VOUT | VERIFIED | [RT] p.2 pin figure and table | KiCad `XC6220B331MR`: 1 VIN, 2 GND, 3 CE, 4 NC, 5 VOUT: same numbers. `TSOT-23-5` pads 1–3 left, 4–5 right: standard. Design: 1 and 3 on VSYS, 2 GND, 5 +3V3, 4 not connected. |
| RT9080 | EN tied to VIN | VERIFIED | [RT] p.5: VIH ≥ 0.9 V, VIL ≤ 0.4 V (at VIN 5 V), EN current ≤ 0.1 µA; abs. max 6.5 V | |
| RT9080 | Iq 2 typ / 4 max µA | VERIFIED in regulation; UNVERIFIED in dropout | [RT] p.4, condition "VIN ≥ VOUT + VDROP" | No dropout ground-current figure or graph. Below a cell of ~3.4 V the LDO is in dropout. Settle by measuring sleep current at 3.3–3.5 V. |
| RT9080 | 600 mA, dropout 0.31 typ / 0.53 max V at 600 mA, limit 610 min / 1100 typ mA | VERIFIED | [RT] p.1, p.4, p.5 | 600 mA needs VIN ≥ 2.3 V. No figure at 355 mA (less than the 600 mA one). |
| RT9080 | Capacitors: C3 10 µF in, C4 1 µF + module's 22 µF + 100 nF out | VERIFIED | [RT] p.3, p.6: CIN 1 µF, COUT ≥ 1 µF effective, ceramic OK; no maximum stated | |
| RT9080 vs SMF5.0A | TVS keeps the LDO inside its 6.5 V absolute maximum | **UNVERIFIED (not protected by the numbers)** | [RT] p.4: VIN abs. max 6.5 V, operating 5.5 V; [SMF] p.2: VBR 6.4–7.0 V at 10 mA, VC 9.2 V at 21.7 A | Only D2 (0.2–0.5 V) sits between VBUS and the LDO. Normal USB (≤ 5.5 V) is fine: VSYS ≤ 5.3 V. See Problem 2. |
| AO3401A Q1 | SOT-23: 1 G, 2 S, 3 D | VERIFIED (figure) | [AO] p.1 top-view figure: D on the single-lead side; with D up, G is lower-left (next to the pin-1 dot), S lower-right | KiCad `SOT-23`: pads 1 (−0.94, −0.95) and 2 (−0.94, +0.95) on one side, pad 3 alone on the other; turned so pad 3 is up, pad 1 is lower-left and pad 2 lower-right: 1 G, 2 S, 3 D. The drawing has no pin numbers; this rests on the figure plus standard SOT-23 numbering. |
| AO3401A | Threshold and Rds for the power path | VERIFIED | [AO] p.2: Vgs(th) −0.5/−0.9/−1.3 V; Rds(on) ≤ 85 mΩ at −2.5 V, ≤ 60 mΩ at −4.5 V; VGS ±12 V | On the cell Vgs = −3.3…−4.2 V: under 30 mV drop at 355 mA. |
| Power path as a whole | Wired the right way round | VERIFIED (by analysis, **INFERRED** behaviour) | circuit.rs: Q1 G = VBUS, S = VSYS, D = +BATT; D2 A = VBUS, K = VSYS; [AO] p.1 body diode drain → source for a P-FET | USB present: VSYS = VBUS − Vf, Vgs ≥ 0, FET off, body diode reverse-biased, cell only charges. USB absent: gate to 0 V through R6, FET on. Unplug: see Problem 1 (slow gate discharge). Reversed cell: body diode blocks, gate and source both near 0 V so the FET stays off; [TP] p.7–8 covers the charger (BAT abs. max −4.2 V). |
| RB160M-30 D2 | Cathode = `D_SOD-123` pad 1 | VERIFIED | [RB] p.1: "Cathode line denotes the cathode end"; KiCad `Device:D_Schottky` pin 1 = K, pad 1 at x = −1.65 | Design: K on VSYS, A on VBUS. Check the band in JLCPCB's placement preview. |
| RB160M-30 | Vf | VERIFIED at 1 A only | [RB] p.2: 0.48 typ / 0.55 max V at 1 A; Fig. 1 (figure): ~0.3 V at 0.1 A, ~0.4 V at 0.5 A | |
| JST S2B-PH-SM4-TB J2 | KiCad pad 1 = JST circuit No. 1 | VERIFIED | [JST] p.4 side-entry figure (figure): front view with the board below, "Mark of No. 1 circuit" at the left; [JST-L] p.1 SM4 side-entry land pattern "viewed from component side" (figure): with the mating face up, Circuit No. 1 is the right-hand pad | KiCad footprint: pads at y = −2.85, tabs (mating side) at y = +2.9, pad 1 at x = −1. Looking into the mouth with the board below, x = −1 is on the left: circuit 1. Both JST figures agree. |
| JST PH J2 | + of an Adafruit-style LiPo lead lands on pad 1 | VERIFIED (from Adafruit's board file, not from a battery in hand) | [ADA] package `JSTPH2_BATT`: pad 2 at x = +1, pads at y = −3.7, tabs at y = +1.5 (Eagle, y up), "+" silk beside pad 2; element X1 pad 2 = VBAT, pad 1 = GND. [ADA-B]: "Polarity matches all Adafruit LiPoly/LiIon chargers and boards" | Turned 180° into KiCad's orientation, Adafruit's + pad is at x = −1: **KiCad pad 1 = + = JST circuit 1**. The design has +BATT on pad 1, GND on pad 2: correct. Adafruit's page does not give wire colours. Still meter every new pack before plugging it in, and put "+" on the silk at pad 1. |
| ICS-43434 MK1 | 1 WS, 2 LR, 3 GND, 4 SCK, 5 VDD, 6 SD | VERIFIED | [ICS] p.10 Fig. 3 (figure) and Table 8 | KiCad symbol matches. Design: LR to GND (left channel), 100 nF on VDD, 100 kΩ on SD (Table 8 asks for exactly that). |
| ICS-43434 | Land pattern vs `InvenSense_ICS-43434-6_3.5x2.65mm` | VERIFIED | [ICS] p.17 Fig. 13 (figure): pads 0.522 × 0.600, 0.9 pitch, 0.822 between rows, 1.252 to the port centre, ring Ø1.025 / Ø1.625; p.10 Fig. 3 for which pad is which | Footprint: pads 0.6 × 0.522 at x = −0.9 / 0 / 0.9, y = −1.364 / −0.542; ring centre at y = 0.71 (0.71 + 0.542 = 1.252), radius 0.6625, width 0.3. Pad positions equal Fig. 3 turned 180° (no mirror). |
| ICS-43434 | Sound hole | VERIFIED | [ICS] p.17: "minimum diameter of 0.5 mm is recommended", larger than the mic's port; no paste on the hole | Footprint: 0.5 mm unplated hole at the ring centre. The mic's own port diameter was not looked up. |
| ICS-43434 | Supply from GPIO6 | VERIFIED range; **INFERRED** pin drop | [ICS] p.4, p.10: 1.65–3.63 V; abs. max 3.63 V; [ESP-C] Table 5-4: VOH ≥ 0.8·VDD at full drive | At 0.5 mA the pin is within millivolts of the rail (inferred; no low-current VOH figure). Supply current 490/550 µA is specified at VDD = 1.8 V only; at 3.3 V it is not given. |
| ICS-43434 | Start-up | VERIFIED | [ICS] p.12: "less than 20 ms"; Table 5: wake-up 20 ms | Discard the first ≥ 20 ms after power and clocks. Inputs must not exceed VDD + 0.3 V (p.8): keep SCK/WS low while MIC_PWR is low (already in `board.toml`). |
| LTST-C19HE1WT D4 | 1 red, 2 green, 3 blue cathodes; **4 = common anode** | VERIFIED | [LED] p.1 package drawing (figure): "POLARITY 4 +" with three diodes to 1 Red, 2 Green, 3 Blue; pin table | `Device:LED_RGBA`: 1 RK, 2 GK, 3 BK, 4 A. Design: 4 on VSYS. |
| LTST-C19HE1WT | Pad positions vs footprint | VERIFIED | [LED] p.1 top view (figure): 1 top-left, 2 top-right, 3 bottom-right, 4 bottom-left; p.7 pads 0.65 × 0.85, gaps 0.2 / 0.6 | KiCad pads: 1 (−0.425, −0.725), 2 (+, −), 3 (+, +), 4 (−, +), 0.65 × 0.85: same. |
| LTST-C19HE1WT | Vf and current with 1 kΩ | VERIFIED limits; currents **INFERRED** from the typical curve | [LED] p.4: Vf at 20 mA red 1.8–2.4 V, green/blue 2.8–3.9 V; p.6 Fig. 2 (figure): red ~2.0 V and green/blue ~2.5–2.6 V at 1–2 mA | Red: ~1.3 mA at VSYS 3.3 V, ~2.7 mA on USB. Green/blue: ~0.7 mA at 3.3 V, ~1.6 mA at 4.2 V, ~2.1 mA on USB (typical part). A high-Vf green/blue part may be dark on a nearly flat cell. The `board.toml` 8 mA load line is a safe over-estimate. |
| LTST-C19HE1WT | Cathode voltage on a GPIO | VERIFIED OK on the cell; marginal for red on USB (**INFERRED**) | [ESP-C] Table 5-4: VIH max VDD + 0.3 V; Table 5-1 lists no pin abs. max; [LED] p.10: a good LED has Vf > 1.4 V (red) / > 2.0 V (green, blue) at 0.1 mA | Pin driven high: it holds 3.3 V, no issue. Pin hi-Z (reset, boot): green/blue cathodes reach at most VSYS − 2.0 = 3.05 V. Red on USB: VSYS up to 5.05 V − ~1.5 V = ~3.5 V, at the VDD + 0.3 V edge, with microamps through 1 kΩ. On the cell (≤ 4.2 V): ≤ 2.8 V. Side effect on USB: red has up to 1.75 V across it with the pin high, so it may glow faintly at high VBUS. Check at bring-up. |
| 1TS009A-1800-5000-CT SW1 | Pads 1 / 2 | VERIFIED | [SW1] drawing (figure): two terminals ① ②, SPST; land pattern 2.0 × 2.0 pads, 4.0 inner / 8.0 outer | KiCad pads 2 × 2 at x = ±3. Non-polar. |
| TS-1088-AR02016 SW2 | Pads 1 / 2 | VERIFIED | [SW2] drawing (figure): ① ②; land pattern 5.50 outer, 3.40 inner, 2.00 tall | KiCad pads 1.05 × 2.0 at x = ±2.225: exact. |
| ESP32-S3-WROOM-1 | Pad numbers for the pins used | VERIFIED | [ESP-M] Table 3-1 pp.11–12: IO4 4, IO5 5, IO6 6, IO7 7, IO8 12, IO9 17, IO10 18, IO11 19, IO12 20, IO13 21, IO21 23, IO0 27, RXD0 36, TXD0 37, IO2 38, IO1 39 | KiCad `RF_Module:ESP32-S3-WROOM-1` has the same numbers for every pin. |
| ESP32-S3 | Strapping pins | VERIFIED | [ESP-M] Tables 4-1..4-5 (as in the starter check) | IO0: 10 kΩ pull-up + button to GND, no capacitor. IO3, IO45, IO46 are in the not-connected list. |
| ESP32-S3 | ADC1 on GPIO1/2; GPIO19/20 USB; no GPIO35–37 | VERIFIED | [ESP-C] pin tables: GPIO1 = ADC1_CH0, GPIO2 = ADC1_CH1, GPIO19/20 = USB_D−/D+; [ESP-M] Table 3-1 note (octal PSRAM) | All signal pins used (0–13, 21) are RTC GPIOs. I2S on 11–13 are ADC2 pins, used as digital only. |
| ESP32-S3 | Power-up glitches | VERIFIED (note for firmware) | [ESP-C] Table 2-2 p.18: GPIO1–14 drive low for ~60 µs at chip power-up | Harmless here: LEDs blink 60 µs, IO7 asks for 300 mA for 60 µs (the charger's soft start is 20 ms). But IO1 discharges C9: the battery reading is low until C9 refills through 667 kΩ (τ = 67 ms). **Wait ≥ 0.35 s after a cold power-up before the first battery reading.** |
| Battery divider R13/R14/R15 + C9 | 2 MΩ over 1 MΩ, 100 nF, ADC1_CH0 | VERIFIED; accuracy note | [ESP-C] Table 5-5: ATTEN2 range 0–1600 mV (4.35 V / 3 = 1.45 V); Table 5-4: input leakage ≤ 50 nA | 50 nA into 667 kΩ is up to 33 mV at the pin = **0.1 V of cell voltage, worst case**. Calibrate per board or accept. Draw 4.2 V / 3 MΩ = 1.4 µA. |
| VBUS sense R16/R17 | 100 k over 150 k | VERIFIED | arithmetic; [ESP-C] VIH 2.475 V | 5.5 V → 3.30 V (≤ VDD + 0.3 V); 5.0 V → 3.0 V; reads high for VBUS ≥ 4.13 V. |
| Sleep table: module 8 µA | Deep sleep, RTC peripherals on | VERIFIED (typical only) | [ESP-M] Table 6-7 p.30 | No maximum. Whether the module figure includes flash/PSRAM standby is not stated (footnote 1's PSRAM adder is attached to light sleep). Measure. |
| Sleep table: RT9080 4 µA, TP4057 2 µA, divider 1.4 µA, "0" line | | VERIFIED | rows above | LEDs with pins held high on the cell: VSYS − 3.3 V ≤ 0.9 V, below any Vf. |
| LCSC numbers | Exact part, package, in stock | VERIFIED | JLCPCB parts API, 2026-09-30 | See the stock table. |

### Switchable charge current (R3 10 kΩ PROG–GND, R19 4.3 kΩ PROG–GPIO7)

- **PROG voltage:** 1.0 V (0.9–1.1) in constant current ([TP] p.3 VPROG), about 0.1 V in trickle, falling toward 0.1 V through constant voltage (termination at 100 mV, p.7). With R3 always fitted, the pin's 2–2.5 µA pull-up adds 25 mV at most. So a hi-Z GPIO7 never sees more than ~1.1 V: no back-feed, no stress. PROG with VCC at 0 V is not specified (UNVERIFIED; R3 holds it at GND).
- **Current:** 10 k ∥ 4.3 k = 3.007 kΩ. The datasheet disagrees with itself: the p.7 table says 3 k → 300 mA; formula 1 (I ≤ 0.3 A) gives 1000 / 3007 = 333 mA; formula 2 gives 325 mA. **Budget 333 mA, not 300.** 10 kΩ → 100 mA (90–110). PROG current 0.33 mA, under the 0.8 mA absolute maximum. Chip maximum 500 mA.
- **Changing it mid-charge:** not addressed in the datasheet (**INFERRED** OK: the loop holds PROG at 1 V, so the current follows the resistor; p.8 Fig. 1 switches RPROG with a FET). Termination is C/10 of the *active* setting: switching 100 → 300 mA late in the constant-voltage phase (below ~30 mA) ends the charge at once; harmless.
- **Stability:** p.9 wants RPROG ≤ 1 / (2π · 10⁵ · C_PROG): up to ~159 pF at 10 kΩ. A GPIO (2 pF) behind 4.3 kΩ is fine. Keep the PROG trace short.
- **GPIO7 states:** input leakage ≤ 50 nA against 100 µA of programme current: 0.05 %. Deep sleep or reset = hi-Z = 100 mA. 60 µs low glitch at power-up: harmless. **Never enable IO7's internal pulls:** the 45 kΩ pull-down would give ~120 mA; the pull-up would inject ~47 µA and roughly halve the charge current and upset termination. Never drive it high: 3.3 V through 4.3 k / 10 k puts PROG at 2.3 V, which stops the charge (harmless, but the shutdown thresholds in the datasheet are inconsistent: table 3.4–3.6 V, text 2.7 V).
- **Heat:** [TP] p.10: P = (VCC − VBAT) × I, thermal regulation at 120 °C, example θJA 150 °C/W. At 5 V and 333 mA: 0.67 W at a 3.0 V cell (limits above ~20 °C ambient), 0.43 W at 3.7 V (limits above ~55 °C). So the first part of a fast charge will be throttled by the chip itself, and the chip runs at up to 120 °C beside the LDO, above the cell. It protects itself, but give pin 2 (GND) copper and keep it away from the cell pocket.
- **Cell:** [ADA-B]: "at a rate of 400mA or less"; "100 to 400mA is a good rate". 333 mA is 0.83C of 400 mAh: inside.
- **USB budget:** 355 mA Wi-Fi peak + 333 mA is over 500 mA. Fast charge only with the radio off, and `board.toml`'s charger load should say 333 mA for that state.

## Problems found

1. **D2's leakage is higher than budgeted and R6 is too weak for it (WRONG).**
   - Fix A, `boards/capture-clip/src/circuit.rs`: `R6` `"100k"` → `"10k"` (C17414, already on the board). Then the VBUS node stays under ~0.5 V even at 45 µA, so Q1 stays fully on and VBUS_SENSE stays low when warm. Cost: 0.5 mA on USB only.
   - Same fix shortens the unplug gap (**INFERRED**): with 100 kΩ, C1 (1 µF) holds the gate up for roughly 50–100 ms after unplugging, and during that time VSYS is fed through Q1's body diode at VBAT − 0.6 V. On a low cell during a Wi-Fi burst that can brown the module out. With 10 kΩ it is ~10 ms.
   - Fix B, `boards/capture-clip/board.toml`, `[[power.sleep_load]]` "Schottky D2 reverse leakage": `ua = 3` → `ua = 6`, source "typical at 25 °C from RB160M-30 Fig. 2; about doubles every 12 °C". The sleep sum is then 21.4 µA against the 20 µA limit, so either the limit moves or D2 changes to a diode with a tabulated low leakage at 5 V (none was researched here).
   - The leakage is a real drain on the cell: VSYS → D2 → R6 → GND.
2. **Nothing keeps VSYS under the RT9080's 6.5 V absolute maximum during a VBUS surge or hot-plug ring (UNVERIFIED, hardware risk).**
   - The SMF5.0A starts conducting at 6.4–7.0 V and clamps at 9.2 V; D2 then peak-charges C3 to within ~0.3 V of that. The TP4057 (9 V) survives; the LDO is outside its rating above 6.5 V.
   - The TP4057 datasheet itself warns about ceramic input capacitors ringing on hot-plug (p.10).
   - Settle it: scope VSYS while hot-plugging a 1–2 m cable from a stiff 5 V source at bring-up. Or remove the question with an LDO rated ≥ 8 V (not researched here), or accept it as most small USB boards do.
3. **No internal pull-up on IO4 (CHRG) (WRONG in `board.toml` and the firmware plan).** With R5 = 100 kΩ the pin cannot go low against a 45 kΩ pull-up. Leave IO4 floating-input; D1/R4 are its pull-up.
4. **Firmware notes that follow from the datasheets:** no internal pulls on IO7 and open-drain only; wait ≥ 0.35 s after cold power-up before reading the battery; no battery = CHRG pulsing with STDBY low; discard the mic's first 20 ms.
5. **Red may glow faintly on USB with its pin "off"** and its hi-Z cathode sits at the edge of VDD + 0.3 V (INFERRED, low risk). Look at bring-up.

## Stock and class (JLCPCB parts API, 2026-09-30)

| LCSC | Part | Package | Class | Stock |
|---|---|---|---|---|
| C12044 | TP4057-42-SOT26-R | SOT-23-6 | Preferred Extended | 33,194 |
| C841192 | RT9080-33GJ5 | TSOT-23-5 | Extended | 40,327 |
| C15127 | AO3401A | SOT-23 | Basic | 800,444 |
| C7502715 | RB160M-30 (hongjiacheng) | SOD-123 | Preferred Extended | 184,760 |
| C295747 | S2B-PH-SM4-TB(LF)(SN) | SMD right angle | Extended | 21,199 |
| C5656610 | ICS-43434 | LGA-6 3.5 × 2.65 | Extended | **1,281** (the thinnest stock on the board) |
| C458749 | LTST-C19HE1WT | 1.6 × 1.6 | Extended | 112,719 |
| C319409 | 1TS009A-1800-5000-CT | 6 × 6 | Extended | 39,679 |
| C720477 | TS-1088-AR02016 | 3.9 × 3 | Basic | 794,525 |
| C2913202 | ESP32-S3-WROOM-1-N16R8 | module | Extended | 29,985 |
| C165948 | TYPE-C-31-M-12 | SMD | Extended | 439,513 |
| C20615824 | H5VUT2U | SOT-23 | Preferred Extended | 129,691 |
| C19077497 | SMF5.0A | SOD-123FL | Preferred Extended | 165,441 |
| C84256 | NCD0805R1 (red) | 0805 | Basic | 4,850,039 |
| C17667 | 4.3 kΩ 0805W8F4301T5E | 0805 | Extended in the API's class field; 172,996 in stock (the circuit's comment says Preferred Extended: the flag was not re-read here) | 172,996 |
| C17477, C17414, C27834, C17513, C149504, C17470, C17514 | 0 Ω, 10 k, 5.1 k, 1 k, 100 k, 150 k, 1 M (UNI-ROYAL 0805W8F…) | 0805 | Basic | 1.0 M – 60 M each |
| C49678, C28323, C15850, C45783 | 100 nF 50 V X7R, 1 µF 50 V X7R, 10 µF 25 V X5R, 22 µF 25 V X5R | 0805 | Basic | 2.3 M – 19 M each |

Every number is the part and package the circuit names. Seven Extended parts carry the loading fee (module, USB-C, mic, LDO, JST, RGB LED, main button), as the parts research said.

No pin-assignment or polarity error was found. The three WRONG rows are a leakage figure, a resistor value and a firmware pull-up.
