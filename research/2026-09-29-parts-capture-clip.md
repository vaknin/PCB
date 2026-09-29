# capture-clip parts: JLCPCB/LCSC selection (2026-09-29)

Board: pocket voice-note recorder. ESP32-S3-WROOM-1-N16R8 (C2913202), USB-C
(HRO TYPE-C-31-M-12 C165948, H5VUT2U C20615824, SMF5.0A C19077497: already
verified, reused), single-cell LiPo charged from USB-C, deep sleep most of the
time (target: whole board < 20 µA asleep), Wi-Fi TX peaks ~355 mA at 3.3 V.

Nothing was ordered and no accounts were used.

## How this was checked

Same method as `research/2026-09-29-parts-starter.md`:

- **Live source:** JLCPCB's parts-search API, queried 2026-09-29:
  `POST https://jlcpcb.com/api/overseas-pcb-order/v1/shoppingCart/smtGood/selectSmtComponentList`.
  - Class: `base` = Basic, `expand` + `preferredComponentFlag` = Preferred Extended ("Pref-Ext"), otherwise Extended.
  - **Stock** is JLCPCB's assembly stock. **@10 USD** is the price tier covering qty 10.
  - Every shortlisted part was re-queried by its LCSC# at the end of the session; the tables show those figures.
- **Full Basic and Preferred lists:** enumerated live with an empty keyword and the filters
  `"componentLibraryType":"base"` and `"preferredComponentFlag":true`. That gave 351 Basic and 1,235 Pref-Ext parts, the same counts as the starter research.
  - **Nothing in those lists for:** MEMS microphones, 3.3 V LDOs with low Iq and ≥ 500 mA (the only Basic ≥ 500 mA LDO is AMS1117; XC6206 is 200 mA), battery protectors (DW01A-type), JST PH connectors, RGB LEDs, or 6×6 mm tactile switches.
  - **Present:** three TOPPOWER chargers (TP4054, TP4056, TP4057, all Pref-Ext), P-MOSFETs (AO3401A and SI2301CDS Basic), a dual N-MOSFET for protection (HJ8205, Pref-Ext), low-leakage Schottkys (RB160M-30, Pref-Ext), and all passives needed.
- **Datasheets:** downloaded from LCSC's mirror (`wmsc.lcsc.com/...`; the `www.lcsc.com/datasheet/...` links return an HTML page to curl, the wmsc form of the same file works) and read with `pdftotext -layout`, page by page. "DS p.N" = PDF page N.
- **KiCad:** footprints checked with `ls` in `/usr/share/kicad/footprints/`; symbol pin numbers read from `/usr/share/kicad/symbols/*.kicad_sym` (KiCad 10.0.6).
- Anything not read from a datasheet or a live listing is marked **UNVERIFIED**; design suggestions are marked **INFERRED**.

JLCPCB Economic PCBA charges **$3.07 per unique Extended part**; Basic and Pref-Ext parts have no fee.

---

## 1. Digital MEMS microphone

No microphone of any kind is Basic or Pref-Ext.

| function | manufacturer part | LCSC# | class | JLC stock | @10 USD | package | datasheet | source checked |
|---|---|---|---|---|---|---|---|---|
| I2S mic (pick) | **TDK InvenSense ICS-43434** | C5656610 | Extended | 1,285 | 3.7752 | LGA-6 3.5×2.65×0.98 mm, bottom port | https://wmsc.lcsc.com/wmsc/upload/file/pdf/v2/lcsc/2312010321_TDK-InvenSense-ICS-43434_C5656610.pdf | https://jlcpcb.com/partdetail/TDKInvenSense-ICS43434/C5656610 |
| PDM mic (alternative) | TDK InvenSense T3902 (MMICT390200012) | C3171752 | Extended | 7,928 | 1.2866 | LGA-5 3.5×2.65×0.98 mm, bottom port | https://wmsc.lcsc.com/wmsc/upload/file/pdf/v2/lcsc/2401291125_TDK-InvenSense-MMICT390200012_C3171752.pdf | https://jlcpcb.com/partdetail/TDKInvenSense-MMICT390200012/C3171752 |
| PDM mic (2nd alt.) | Infineon IM69D130V01 | C536262 | Extended | 607 | 2.6056 | LGA-5 4×3 mm | https://www.lcsc.com/datasheet/lcsc_datasheet_2010281905_Infineon-Technologies-IM69D130V01_C536262.pdf (datasheet downloaded, not read in detail) | https://jlcpcb.com/partdetail/InfineonTechnologies-IM69D130V01/C536262 |

ICS-43434 (DS rev 1.2, the LCSC mirror of TDK's DS-000069):
- **Port and hole:** "digital I²S output bottom port microphone" (DS p.1). The PCB needs a sound hole: "A minimum diameter of 0.5 mm is recommended" and it should be larger than the mic's port; keep paste off the hole (DS p.17).
- **Supply:** 1.65–3.63 V (DS p.4). Runs straight from the 3.3 V rail.
- **Current:** 490 µA typ / 550 max in high-performance mode, 230 / 300 µA in low-power mode (DS pp.4–5). **Sleep mode (fS < 3.125 kHz): 12 µA typ, 20 µA max** (DS p.4).
  - Standby: entered when SCK drops below ~200 kHz; stop both SCK and WS and pull them low, or current flows through WS's internal pull-down (DS p.12).
  - Don't clock the mic while its VDD is off; that drives current through its ESD diodes (DS p.12).
- **Pins (DS p.10, Table 8):** 1 WS, 2 LR, 3 GND, 4 SCK, 5 VDD, 6 SD.
  - **LR:** low = left channel, high = right. It has an internal 100 kΩ pull-down. If tied to VDD, VDD/100 kΩ (~33 µA) flows through it. So **tie LR to GND (left)**. The pin text is garbled in the text layer; the pull-down and the current note are readable.
  - **SD** tristates; put a 100 kΩ pull-down on it (DS p.10, p.12).
  - **VDD:** 0.1 µF to GND.
  - Format: I²S, 24-bit two's complement, 64 SCK per WS frame (DS p.12).
- **KiCad:** symbol `Sensor_Audio:ICS-43434` has pins 1 WS, 2 LR, 3 GND, 4 SCK, 5 VDD, 6 SD, which **matches the datasheet**.
  - Its default footprint `Sensor_Audio:InvenSense_ICS-43434-6_3.5x2.65mm` (exists) has pads 1–6, pad 3 as a custom GND ring, and a 0.5 mm NPTH sound hole at (0, 0.71). That meets the 0.5 mm minimum and is above the ≥ 0.3 mm free-hole limit in HARDWARE_LESSONS.
  - The land-pattern figure (DS p.17) is an image, so pad dimensions were not compared (**UNVERIFIED**, low risk: the footprint is named for this part).

T3902 (DS-000357 rev 1.0):
- **Port:** "Bottom Port PDM" (DS p.1). PCB hole 0.5–1 mm; the mic's port is 0.375 mm (DS p.18).
- **Supply:** 1.65–3.63 V (DS p.4).
- **Current:** 650 µA (high performance), 430 µA (standard), 185 µA (low power), typ. **Sleep (CLK < 200 kHz): 12 µA typ / 20 max** (DS pp.4–5).
- **Pins (DS p.10, Table 9):** 1 DATA, 2 SELECT (GND = right/DATA1, VDD = left/DATA2), 3 GND, 4 CLK, 5 VDD (0.1 µF X7R at pin 5). SELECT has no pull resistor mentioned; tie it hard.
- **KiCad:** there is no T3902 symbol. `Sensor_Audio:SPH0641LU4H-1` (1 DATA, 2 SEL, 3 GND, 4 CLOCK, 5 VDD) has **the same pin numbers and functions**.
  - Its footprint `Sensor_Audio:Knowles_LGA-5_3.5x2.65mm` matches every dimension in the T3902 land-pattern figure's text layer (DS p.18): pads 0.522×0.725 mm (4×), 1.675 mm column pitch, 0.822 mm row pitch, 1.252 mm from the pad row to the port ring, and a 0.5 mm NPTH port hole.
  - Which corner is pin 1 is only in the drawing (image). The footprint's pin-1 corner vs the T3902 is **UNVERIFIED**.

**Pick: ICS-43434 (I2S).** Why:
- Exact KiCad symbol and footprint, pins checked against the datasheet.
- I2S is the ESP32 path with the most known-good examples.
- 24-bit output needs no PDM decimation step.

The T3902 is a good fallback if ICS-43434 stock (1,285) runs out: $2.49 cheaper per board, 6× the stock, and ESP32-S3 has PDM RX on I2S0. It needs a borrowed symbol and footprint (dimensions match), and its pin-1 orientation still has to be checked. Both are Extended, so the loading fee is the same.

Rejected:
- **MSM261S4030H0R (C2840615):** 0 stock. Top-port I2S, 1.8–3.3 V (DS p.1, p.8).
- **Other in-stock MEMSensing/LinkMems/Goertek parts:** analog, or PDM parts without a KiCad footprint. LinkMems LMD4030T261-OA1 (C7587900) is a top-port PDM with 1.62–3.6 V supply (DS p.3); the "T/B" in these part names is top/bottom port.
- **SPH0645 / INMP441:** 0 stock at JLC.

Design notes (INFERRED):
- **The mic's 12–20 µA sleep current alone would use the whole 20 µA budget.** Switch its VDD off in deep sleep: feed it from a spare GPIO (490 µA is a light load) or through a small P-FET (AO3401A, below).
- **Stop and ground SCK/WS before removing VDD** (DS p.12).
- **Enclosure hole:** the port is on the bottom of the package, so the sound path is PCB hole → enclosure. Put the mic on the board face that sits against the enclosure wall, or add a gasket.

## 2. Single-cell Li-ion charger (5 V USB → 4.2 V)

| function | manufacturer part | LCSC# | class | JLC stock | @10 USD | package | datasheet | source checked |
|---|---|---|---|---|---|---|---|---|
| Charger (pick) | **TOPPOWER TP4057-42-SOT26-R** | C12044 | **Pref-Ext** | 33,251 | 0.1409 | SOT-23-6 | https://www.lcsc.com/datasheet/lcsc_datasheet_1811021622_TOPPOWER-Nanjing-Extension-Microelectronics-TP4057-42-SOT26-R_C12044.pdf | https://jlcpcb.com/partdetail/12599-TP4057_42_SOT26R/C12044 |
| Charger (alt.) | TOPPOWER TP4054-42-SOT25R | C32574 | **Pref-Ext** | 28,377 | 0.1326 | SOT-23-5 | https://www.lcsc.com/datasheet/lcsc_datasheet_1809261814_TOPPOWER-Nanjing-Extension-Microelectronics-TP4054-42-SOT25R_C32574.pdf | https://jlcpcb.com/partdetail/33539-TP4054_42SOT25R/C32574 |
| Charger (alt., 1 A) | TOPPOWER TP4056-42-ESOP8 | C16581 | Pref-Ext | 86,787 | 0.1843 | ESOP-8 | https://www.lcsc.com/datasheet/lcsc_datasheet_1809261820_TOPPOWER-Nanjing-Extension-Microelectronics-TP4056-42-ESOP8_C16581.pdf (not read) | https://jlcpcb.com/partdetail/17264-TP4056_42ESOP8/C16581 |
| Charger (reference) | Microchip MCP73831T-2ACI/OT | C424093 | Extended | 9,775 | 0.6319 | SOT-23-5 | https://www.lcsc.com/datasheet/lcsc_datasheet_1912111437_Microchip-Tech-MCP73831T-2ACI-OT_C424093.pdf | https://jlcpcb.com/partdetail/MicrochipTech-MCP73831T_2ACIOT/C424093 |

TP4057 (Chinese datasheet):
- **Charge:** 4.2 V float, 4.0–9.0 V input (DS p.3), up to 500 mA.
- **Setting the current:** RPROG 10 k → 100 mA, 5 k → 200, 4 k → 250, **3 k → 300**, 2 k → 400, 1.6 k → 500 mA (table, DS p.7). The DS p.3 table gives RPROG = 2 k → 380/400/420 mA and 1.6 k → 480/500/520 mA.
- **Charging behaviour:** trickle charge below 2.9 V; ends at C/10; recharges at ~4.05 V.
- **Battery drain with USB absent: "sleep mode, VCC = 0 V": −1 µA typ, −2 µA max** (DS p.3). Shutdown (PROG open) ±1/±2 µA.
- **Pins (DS p.5):** 1 CHRG (open drain, low while charging), 2 GND, 3 BAT, 4 VCC, 5 STDBY (open drain, low when charge is complete), 6 PROG.
- **Two status outputs** (charging / done), each ≤ 1 µA leakage (DS pp.3–4), so firmware can read both on GPIOs with pull-ups.
- **Reverse-battery protection** (DS p.2, p.7–8): a reversed cell draws < 0.8 mA through the chip and is not damaged; with a reversed cell, VCC should be about 5 V and must not exceed 8 V.
- **KiCad:** symbol `Battery_Management:TP4057` has pins 1 ~CHRG, 2 GND, 3 BAT, 4 V_CC, 5 STDBY, 6 PROG, which **matches the datasheet**.
  - Its default footprint is `Package_TO_SOT_SMD:TSOT-23-6`. The part is SOT-23-6 ("SOT26", DS p.9), so **set the footprint to `Package_TO_SOT_SMD:SOT-23-6`** (exists).

TP4054:
- **Pins (DS p.6):** 1 CHRG, 2 GND, 3 BAT, 4 VCC, 5 PROG.
- **Supply and current:** VCC 4.0–9.0 V; RPROG 10 k → 100 mA, 1.66 k → 400 mA (DS p.3). The formula differs above 150 mA (DS p.7).
- **Battery drain, VCC = 0 V: −1 µA typ / −2 max** (DS p.3). Only one status pin, and no reverse-battery protection mentioned.
- **KiCad:** no TP4054 symbol. `Battery_Management:LTC4054ES5-4.2` has 1 ~CHRG, 2 GND, 3 BAT, 4 V_CC, 5 PROG, which **matches**. Its default footprint is TSOT-23-5; use `Package_TO_SOT_SMD:SOT-23-5` for the TP4054.

MCP73831 (reference, DS pp.3–4, p.11):
- 3.75–6 V input.
- Battery discharge current with VDD < VBAT: 0.15 µA typ / 2 µA max.
- STAT is a tri-state output.
- Pin-for-pin with TP4054: 1 STAT, 2 VSS, 3 VBAT, 4 VDD, 5 PROG. KiCad `MCP73831-2-OT` has the same numbers.
- Costs the $3.07 fee.

**Pick TP4057 with RPROG = 3 kΩ (300 mA):**
- It is Pref-Ext, has an exact KiCad symbol and two status pins.
- It survives a reversed battery, which matters because JST PH polarity isn't standard (§6).
- 300 mA is ≤ 1C for any cell from §10. For a smaller cell, use 4 kΩ (250 mA) or 5 kΩ (200 mA); all of these are in the table.

## 3. Load sharing (USB when plugged, battery otherwise)

The classic circuit (INFERRED from the well-known Adafruit design, not re-read here):
- VBUS → Schottky → VSYS.
- VBAT → P-FET (drain at VBAT, source at VSYS, gate to VBUS with 100 kΩ to GND) → VSYS.

With USB present, the gate sits at 5 V and the source at ~4.6 V, so the FET is off and its body diode is reverse-biased. With USB absent, the gate is pulled to 0 V, Vgs ≈ −VBAT, and the FET is on.

No charger with a built-in power path is Basic or Pref-Ext (only TP4054/56/57 are), so it is discrete.

| function | manufacturer part | LCSC# | class | JLC stock | @10 USD | package | datasheet | source checked |
|---|---|---|---|---|---|---|---|---|
| P-FET (pick) | **Alpha & Omega AO3401A** | C15127 | **Basic** | 810,999 | 0.0941 | SOT-23 | https://www.lcsc.com/datasheet/lcsc_datasheet_2412061733_Alpha---Omega-Semicon-AO3401A_C15127.pdf | https://jlcpcb.com/partdetail/Alpha_OmegaSemicon-AO3401A/C15127 |
| P-FET (alt.) | Vishay SI2301CDS-T1-GE3 | C10487 | **Basic** | 214,282 | 0.0974 | SOT-23 | https://www.lcsc.com/datasheet/lcsc_datasheet_1808272021_Vishay-Intertech-SI2301CDS-T1-GE3_C10487.pdf | https://jlcpcb.com/partdetail/VishayIntertech-SI2301CDS_T1GE3/C10487 |
| Schottky (pick) | **hongjiacheng RB160M-30** | C7502715 | **Pref-Ext** | 184,780 | 0.0319 | SOD-123 | https://wmsc.lcsc.com/wmsc/upload/file/pdf/v2/lcsc/2309051412_hongjiacheng-RB160M-30_C7502715.pdf | https://jlcpcb.com/partdetail/hongjiacheng-RB160M30/C7502715 |
| Schottky (alt.) | MDD SS14 | C2480 | **Basic** | 1,205,485 | 0.0191 | SMA (DO-214AC) | https://www.lcsc.com/datasheet/lcsc_datasheet_2407101109_MDD-Microdiode-Semiconductor-SS14_C2480.pdf | https://jlcpcb.com/partdetail/MDD_Microdiode_Semiconductor-SS14/C2480 |

AO3401A (DS pp.1–2):
- **Ratings:** VDS −30 V; **VGS ±12 V** (fine: the gate never sees more than ~5 V against the source).
- **Vgs(th): −0.5 / −0.9 / −1.3 V** (min/typ/max).
- **Rds(on):** 47 typ / 60 max mΩ at −4.5 V; **60 typ / 85 max mΩ at −2.5 V**. At a 355 mA peak that is ≤ 30 mV of drop.
- Body diode 0.7 V typ at 1 A.
- **Pinout:** from the SOT-23 top-view drawing (D top, G bottom-left, S bottom-right) → 1 G, 2 S, 3 D (**INFERRED** from standard SOT-23 numbering; the drawing has no numbers).
- **KiCad:** `Transistor_FET:AO3401A` (extends TP0610T) has 1 G, 2 S, 3 D, footprint `Package_TO_SOT_SMD:SOT-23`. **Matches.**

SI2301CDS (DS pp.1–2):
- VDS −20 V; **VGS ±8 V** (OK at 5 V, but less margin than the AO3401A).
- Vgs(th) −0.4 to −1.0 V.
- Rds(on) 0.090/0.112 Ω at −4.5 V; **0.110/0.142 Ω at −2.5 V**.
- Pins **1 G, 2 S, 3 D, read from the numbered drawing (DS p.1)**.
- No KiCad symbol named for it; `Transistor_FET:AO3401A` has the same pin order.

RB160M-30 (hongjiacheng, DS pp.1–2):
- 30 V, 1 A.
- **VF 0.48 typ / 0.55 max V at 1 A**; 0.82/0.875 V at 3 A. No 0.5 A figure is tabulated: it is below the 1 A value, ~0.4 V read off the curve by eye (**UNVERIFIED**, graph only).
- **IR ≤ 40 µA at VR = 30 V, 25 °C.** No figure at the ~4 V this circuit sees; much lower is expected (**UNVERIFIED**, curve only).
- It is marketed "Low Reverse Current".

SS14 (MDD, DS p.1):
- **VF ≤ 0.55 V at 1 A.**
- **IR ≤ 0.3 mA at 40 V, 25 °C** (10 mA at 100 °C). Roughly 7× the RB160M's rated leakage.

Reverse leakage matters for standby (INFERRED):
- With USB absent, the Schottky is reverse-biased by VSYS ≈ VBAT. Its leakage flows into the VBUS node and out through the 100 kΩ gate pull-down (and the charger's VCC pin).
- The leak is self-limiting: as the VBUS node rises, the reverse voltage falls. Even 20 µA × 100 kΩ = 2 V on the gate still leaves Vgs ≈ −1.7 V, so the FET stays on.
- Keep the pull-down at ≤ 100 kΩ.

**KiCad:**
- RB160M-30 has no named symbol. Use `Device:D_Schottky` with `Diode_SMD:D_SOD-123` (pad 1 = K).
- SS14 has `Diode:SS14` (pins 1 K, 2 A), footprint `Diode_SMD:D_SMA`.

## 4. 3.3 V LDO (VSYS 3.0–5 V in, ≥ 500 mA, low Iq)

No suitable Basic or Pref-Ext part exists: AMS1117 draws ~5 mA quiescent and drops ~1.1 V; XC6206 is 200 mA.

| function | manufacturer part | LCSC# | class | JLC stock | @10 USD | package | max I | Iq | dropout | datasheet | source checked |
|---|---|---|---|---|---|---|---|---|---|---|---|
| LDO (pick) | **Richtek RT9080-33GJ5** | C841192 | Extended | 40,349 | 0.1199 | TSOT-23-5 | 600 mA | **2 µA typ / 4 max** | 0.31 typ / 0.53 max V @ 600 mA | https://www.lcsc.com/datasheet/lcsc_datasheet_2009192305_Richtek-Tech-RT9080-33GJ5_C841192.pdf | https://jlcpcb.com/partdetail/RichtekTech-RT908033GJ5/C841192 |
| LDO (alt.) | Torex XC6220B331MR-G | C86534 | Extended | 30,962 | 0.3704 | SOT-25 (SOT-23-5) | 1 A | 8 µA typ / 18 max (PS mode, ≤ 0.1 mA load); 50 µA typ in HS mode | 60 typ / 95 max mV @ 300 mA (read from a merged table cell, see note); ≤ 655 mV @ 1 A | https://www.lcsc.com/datasheet/lcsc_datasheet_1810010324_Torex-Semicon-XC6220B331MR-G_C86534.pdf | https://jlcpcb.com/partdetail/TorexSemicon-XC6220B331MRG/C86534 |

RT9080 (DS rev. as mirrored by LCSC):
- **Input 1.2–5.5 V; absolute max 6.5 V** (DS p.4).
- **Iq 2 µA typ / 4 µA max; shutdown 0.1 / 0.5 µA** (DS p.4).
- **Dropout at 600 mA (VOUT ≥ 3 V): 0.31 typ / 0.53 max V** (DS p.4). No 500 mA row; it is ≤ the 600 mA figure.
- Current limit 610 min / 1100 typ mA (DS p.5). θJA 230.6 °C/W (DS p.4).
- **Capacitors:** ≥ 1 µF input close to the pins. Output: "Any output capacitor meeting the minimum 1 mΩ ESR and larger than 1 µF requirement may be used", ceramic or tantalum; ≥ 1 µF *effective* after DC bias (DS p.3, p.11). So the 22 µF + 0.1 µF Espressif wants at the module is allowed (unlike SGM2212).
- **Pins, TSOT-23-5 (DS p.2):** 1 VIN, 2 GND, 3 EN, 4 NC (SNS on the adjustable RT9080N), 5 VOUT.
- **KiCad:** no RT9080 symbol. `Regulator_Linear:XC6220B331MR` (1 VIN, 2 GND, 3 CE, 4 NC, 5 VOUT), `ME6211C33M5`, `AP2112K-3.3` and `TLV75733PDBV` all use **the same pin numbers**. Use one of them and set the footprint to `Package_TO_SOT_SMD:TSOT-23-5` (exists).

XC6220B331MR-G (DS pp.1–5, table p.8):
- Operating input 1.6–6.0 V, abs. max 6.5 V.
- Ceramic output capacitor OK (DS p.1).
- Standby (CE low) 0.01 µA typ.
- Pins (SOT-25, DS p.2): 1 VIN, 2 VSS, 3 CE, 4 NC, 5 VOUT, matching KiCad `Regulator_Linear:XC6220B331MR` (footprint SOT-23-5).
- It switches between power-save mode (8 µA) and high-speed mode (50 µA) with load.
- The 3.3 V dropout values sit in merged table cells (DS p.8: "60 / 95" at 300 mA printed on the 3.50 V row, "655" at 1 A on the 4.00 V row). Which output voltages those cells cover is **UNVERIFIED**.

Rejected on quiescent current (JLC listing values, **UNVERIFIED** against datasheets):
- ME6211C33 (C82942): 40–60 µA
- TLV75733P (C485517): 25 µA
- AP2112K-3.3 (C51118): 55 µA
- RT9013-33: 25 µA
- TPS7A2033, LP5907: only 300 mA

Design notes (INFERRED):
- **On battery the rail follows the cell below ~3.4–3.6 V.** VBAT − FET drop − dropout falls under 3.3 V there. The ESP32-S3 module is rated 3.0–3.6 V (starter research), so it keeps running down to a cell of roughly 3.2–3.3 V at Wi-Fi peaks. Firmware should shut down at ~3.3 V on the ADC reading.
- **From USB,** VSYS is ~4.6–4.8 V, which is fine for the 5.5 V operating limit. A VBUS surge that the SMF5.0A clamps at 9.2 V would exceed the RT9080's 6.5 V absolute maximum. That is a surge risk shared by every small LDO; the Schottky and the charger soak some of it.
- **Heat:** 4.7 V → 3.3 V at an average of 100–150 mA is 0.14–0.21 W, a rise of about 32–48 °C at 231 °C/W. That is acceptable for bursts; sustained Wi-Fi upload on USB power will run warm.

## 5. Battery protection

| function | manufacturer part | LCSC# | class | JLC stock | @10 USD | package | datasheet | source checked |
|---|---|---|---|---|---|---|---|---|
| Protector IC | PUOLOP DW01A | C351410 | Extended | 116,652 | 0.0418 | SOT-23-6 | https://www.lcsc.com/datasheet/lcsc_datasheet_1912111437_PUOLOP-DW01A_C351410.pdf | https://jlcpcb.com/partdetail/PUOLOP-DW01A/C351410 |
| Dual N-FET | hongjiacheng HJ8205 (8205A-type) | C20069150 | **Pref-Ext** | 49,683 | 0.0369 | SOT-23-6 | https://wmsc.lcsc.com/wmsc/upload/file/pdf/v2/lcsc/2401111808_hongjiacheng-HJ8205_C20069150.pdf | https://jlcpcb.com/partdetail/hongjiacheng-HJ8205/C20069150 |
| Dual N-FET (alt.) | TECH PUBLIC FS8205A | C2830320 | Extended | 53,275 | 0.0496 | SOT-23-6 | https://www.lcsc.com/datasheet/lcsc_datasheet_2108132230_TECH-PUBLIC-FS8205A_C2830320.pdf (not read) | https://jlcpcb.com/partdetail/TECHPUBLIC-FS8205A/C2830320 |
| Single-chip protector (alt.) | XySemi XB5353A | C80628 | Extended | 2,288 | 0.0874 | SOT-23-5 | https://www.lcsc.com/datasheet/lcsc_datasheet_1811151533_XySemi-XB5353A_C80628.pdf | https://jlcpcb.com/partdetail/XySemi-XB5353A/C80628 |

DW01A (PUOLOP):
- **Current: 3.0 µA typ / 6.0 max at 3.6 V; power-down 4 µA max** (DS p.4).
- **Thresholds:** overcharge 4.25/4.30/4.35 V, overdischarge 2.40/2.50/2.60 V, overcurrent 130/150/170 mV across the FETs (DS p.4). With 2 × ~30 mΩ that is roughly 2.5 A (INFERRED).
- **Pins (DS p.2):** 1 OD, 2 VM (current sense; KiCad calls it CS), 3 OC, 4 TD, 5 VCC (through R1), 6 GND. `Battery_Management:DW01A` has 1 OD, 2 CS, 3 OC, 4 TD, 5 VCC, 6 GND, which **matches**.

HJ8205 (DS pp.1–2):
- 20 V, 4.3 A.
- Rds(on) < 30 mΩ at 4.0 V and < 46 mΩ at 2.5 V; Vgs(th) 0.45–1.0 V.
- **Pins: 1 S1, 2 D1/D2, 3 S2, 4 G2, 5 D1/D2, 6 G1** (DS p.1).
- **KiCad gap:** no stock symbol has this order. The `Q_Dual_NMOS_*` variants are S1G1S2G2D2D1, G1S2G2D2S1D1, S1G1D2S2G2D1 and an 8-pin one. It **needs a custom symbol**; the footprint `Package_TO_SOT_SMD:SOT-23-6` is fine.

XB5353A (DS pp.2–3):
- Integrated FETs, 54 mΩ typ / 63 max Rss(on).
- **2.8 µA typ / 6 max; power-down 1.5 µA.**
- Overcurrent 2.1/3/3.9 A.
- Pins: 1 VT (test), 2 GND, 3 VDD, 4 and 5 VM.
- No KiCad symbol; SOT-23-5 footprint.
- Only 2,288 in stock.

**Recommendation: require a protected cell and put no protector on the board.**
- Hobby single-cell LiPo packs with a JST PH lead normally carry a protection board. Adafruit's all do: "keeps the battery voltage from going too high (over-charging) or low (over-use) … cut out when completely dead at 3.0V … protect against output shorts" (adafruit.com/product/1578, read 2026-09-29).
- **On-board DW01A + HJ8205 would cost:** $3.07 per order (DW01A is Extended), about $0.08 per board, 3–6 µA of standby current, and a custom symbol.
- **A protected cell costs about nothing extra:** it is the normal product.
- Doubling up is the safest option, but the gain is small. **Buy only cells that state protection.**

## 6. Battery connector (JST PH 2.0 mm, 2-pin, SMD right angle)

| function | manufacturer part | LCSC# | class | JLC stock | @10 USD | package | datasheet | source checked |
|---|---|---|---|---|---|---|---|---|
| Battery conn. (pick) | **JST S2B-PH-SM4-TB(LF)(SN)** (genuine) | C295747 | Extended | 21,902 | 0.2362 | SMD right angle, 2 pins + 2 tabs | https://www.lcsc.com/datasheet/lcsc_datasheet_2102031704_JST-S2B-PH-SM4-TB-LF-SN_C295747.pdf (not text-read) | https://jlcpcb.com/partdetail/JST-S2B_PH_SM4_TB_LF_SN/C295747 |
| Battery conn. (alt.) | XUNPU WAFER-PH2.0-2PWB (clone) | C3029440 | Extended | 115,382 | 0.0639 | SMD right angle | https://wmsc.lcsc.com/wmsc/upload/file/pdf/v2/lcsc/2205271745_XUNPU-WAFER-PH2-0-2PWB_C3029440.pdf (not text-read) | https://jlcpcb.com/partdetail/XUNPU-WAFER_PH2_02PWB/C3029440 |

- **KiCad:** `Connector_JST:JST_PH_S2B-PH-SM4-TB_1x02-1MP_P2.00mm_Horizontal` is an exact match for the genuine part. The clone's land pattern was not compared (**UNVERIFIED**).
- **Polarity, by physical view:** looking at the mating face of the battery's plug, with the narrow polarizing bump on top, red (+) is on the right for both Adafruit and SparkFun packs (DigiKey forum answer, read 2026-09-29, a secondary source).
  - SparkFun: "pin 1 as +VBATT (red wire) and pin 2 connected to ground (black wire)", and "depending on the manufacturer they may be reversed!!!" (learn.sparkfun.com, single-cell-lipo-battery-care, read 2026-09-29).
  - Adafruit: "Polarity matches all Adafruit LiPoly/LiIon chargers and boards, other brands may have reverse polarity and can destroy your battery" (adafruit.com/product/1578).
- **Which KiCad pad (1 or 2) is + is UNVERIFIED.** SparkFun's "pin 1" is their own footprint's numbering. It must be tied to the JST drawing's circuit numbers and the KiCad footprint at schematic time, then written into HARDWARE_LESSONS.
- **Warning:** many AliExpress/Amazon packs are wired the other way.
  - The TP4057 survives a reversed cell (§2).
  - The rest of the board is INFERRED to tolerate one: with USB absent, the P-FET body diode is reverse-biased and the gate is at 0 V, so the FET stays off; the 1 MΩ divider injects only ~2 µA into the ADC pin's clamp.
  - Still, **check every new pack with a multimeter** before plugging it in.

## 7. Indicator LED (discrete RGB, common anode)

No RGB LED is Basic or Pref-Ext.

| function | manufacturer part | LCSC# | class | JLC stock | @10 USD | package | datasheet | source checked |
|---|---|---|---|---|---|---|---|---|
| RGB LED (pick) | **Lite-On LTST-C19HE1WT** | C458749 | Extended | 120,747 | 0.0727 | 1.6×1.6 mm, 4 pads | https://wmsc.lcsc.com/wmsc/upload/file/pdf/v2/lcsc/2204251545_Lite-On-LTST-C19HE1WT_C458749.pdf | https://jlcpcb.com/partdetail/LiteOn-LTSTC19HE1WT/C458749 |
| RGB LED (alt.) | XINGLIGHT XL-1615RGBC-RF | C965840 | Extended | 658,366 | 0.0195 | 1.6×1.5 mm, 4 pads | https://www.lcsc.com/datasheet/lcsc_datasheet_2410121441_XINGLIGHT-XL-1615RGBC-RF_C965840.pdf | https://jlcpcb.com/partdetail/XINGLIGHT-XL_1615RGBCRF/C965840 |

LTST-C19HE1WT:
- **Pins:** pin assignment 1 = red (AlInGaP), 2 = green (InGaN), 3 = blue (InGaN) (DS p.2). **Pin 4 = common anode** is from the package drawing, an image (**UNVERIFIED**; JLC's listing for this part doesn't state it).
- **Vf at 20 mA (DS p.5):** blue 2.80–3.90 V, green 2.80–3.90 V, red 1.80–2.40 V. Intensity: blue 28–180, green 112–450, red 45–180 mcd.
- **KiCad:** footprint `LED_SMD:LED_LiteOn_LTST-C19HE1WT` (exact; pads 1–4). Generic symbol `Device:LED_RGBA` (R, G, B, A) fits if pin 4 is the anode.

XL-1615RGBC-RF:
- "1615 七彩（共阳）" = common anode (DS p.1). Vf at 20 mA: R 1.8–2.4 V, G 2.6–3.4 V, B 2.6–3.4 V (DS p.4).
- Pin numbering is only in the drawing (image): **UNVERIFIED**. No KiCad footprint.

Driving it (INFERRED):
- **Blue and green need up to 3.9 V at 20 mA,** so a 3.3 V rail through a resistor is marginal (same problem as HARDWARE_LESSONS' green LED).
- **Tie the common anode to VSYS (3.7–4.7 V), not 3V3.** Sink each cathode into a GPIO through its own resistor (~1–2 mA is plenty for an indicator).
- **When a GPIO is high (3.3 V), the LED sees ≤ 1.4 V:** red may glow faintly on USB power (VSYS ~4.7 V − 3.3 V = 1.4 V < red Vf ~1.8 V, probably dark).
- **Alternatives:** switch cathodes with N-FETs (2N7002, Basic C8545), or run the anode from 3V3 at ≤ 1 mA and accept a dim blue.
- **In deep sleep, hold the cathode pins high or Hi-Z** so the LED draws nothing. A discrete LED has no idle current, unlike a WS2812/SK6805 (~0.3–1 mA per JLC listings).

## 8. Buttons

| function | manufacturer part | LCSC# | class | JLC stock | @10 USD | package | datasheet | source checked |
|---|---|---|---|---|---|---|---|---|
| Main button (pick) | **HYP 1TS009A-1800-5000-CT** | C319409 | Extended | 39,744 | 0.0943 | 6×6 mm, 5.0 mm tall, J-lead, 1.8 N | https://www.lcsc.com/datasheet/lcsc_datasheet_1811151231_HYP--Hongyuan-Precision-1TS009A-1800-5000-CT_C319409.pdf (image-only PDF) | https://jlcpcb.com/partdetail/300408-1TS009A_1800_5000CT/C319409 |
| Main button (alt., premium feel) | ALPS SKQGABE010 | C115351 | Extended | 55,550 | 0.1229 | 5.2×5.2 mm, 1.5 mm with stem | https://www.lcsc.com/datasheet/lcsc_datasheet_1811051111_ALPSALPINE-SKQGABE010_C115351.pdf | https://jlcpcb.com/partdetail/ALPSALPINE-SKQGABE010/C115351 |
| Reset (pick) | **XUNPU TS-1088-AR02016** | C720477 | **Basic** | 882,557 | 0.0537 | 3.9×3.0×2.0 mm | https://www.lcsc.com/datasheet/lcsc_datasheet_2304140030_XUNPU-TS-1088-AR02016_C720477.pdf | https://jlcpcb.com/partdetail/XUNPU-TS_1088AR02016/C720477 |
| Reset / main (Basic fallback) | XKB TS-1187A-B-A-B | C318884 | **Basic** | 522,657 | 0.0205 | 5.1×5.1×1.5 mm, 1.6 N | https://www.lcsc.com/datasheet/lcsc_datasheet_2304140030_XKB-Connection-TS-1187A-B-A-B_C318884.pdf | https://jlcpcb.com/partdetail/XKBConnection-TS_1187A_B_AB/C318884 |

- **1TS009A-1800-5000-CT:** 1.8 N, 5 mm tall, 100k cycles, −40–90 °C, 12 V 50 mA (JLC listing; the datasheet PDF is an image, so **UNVERIFIED** against it).
  - The 5 mm stem can poke through or meet a printed button cap in the enclosure.
  - **KiCad:** `Button_Switch_SMD:SW_Push_1TS009xxxx-xxxx-xxxx_6x6x5mm`, whose description links this exact LCSC datasheet. 2 pads (1, 2), so `Switch:SW_Push` maps directly.
  - Sister parts on the same footprint: 1TS009B-2500-5000-CT C255816 (2.5 N, 30,335 stock) for a firmer click; 1TS009B-2100-4300-CT (4.3 mm).
- **SKQGABE010 (DS p.1):** "with stem", 1.57 N, 0.25 mm travel, 500,000 cycles, 12 V 50 mA. Alps' crisp tactile feel.
  - **KiCad:** `Button_Switch_SMD:SW_SPST_SKQG_WithStem` (4 pads numbered 1,1,2,2).
  - Low profile, so the enclosure needs a plunger.
- **Side-actuated option** (button on the case edge): Panasonic EVQP7A01P C79167 (Extended, 9,049 stock, $0.195) with the exact KiCad `SW_SPST_EVQP7A`. Its datasheet was not read (**UNVERIFIED**).
- **Reset:** TS-1088 matches `Button_Switch_SMD:SW_SPST_TS-1088-xR020` (pads 1, 2; the footprint's description links this LCSC part). Already used on the starter.
- **Wiring (INFERRED):** put the main button on an RTC-capable GPIO so it can wake the chip from deep sleep. If it is on GPIO0, it can double as BOOT; check the ESP32-S3 RTC GPIO list and strapping rules at schematic time.

## 9. Battery voltage divider (all **Basic**)

| function | manufacturer part | LCSC# | class | JLC stock | @10 USD | package | datasheet | source checked |
|---|---|---|---|---|---|---|---|---|
| 1 MΩ 1% (×2) | UNI-ROYAL 0402WGF1004TCE | C26083 | Basic | 2,535,892 | 0.0026 | 0402 | https://www.lcsc.com/datasheet/lcsc_datasheet_2206010030_UNI-ROYAL-Uniroyal-Elec-0402WGF1004TCE_C26083.pdf | https://jlcpcb.com/partdetail/26826-0402WGF1004TCE/C26083 |
| 1 MΩ 1% (0805 option) | UNI-ROYAL 0805W8F1004T5E | C17514 | Basic | 2,453,389 | 0.0043 | 0805 | https://www.lcsc.com/datasheet/lcsc_datasheet_2206010200_UNI-ROYAL-Uniroyal-Elec-0805W8F1004T5E_C17514.pdf | https://jlcpcb.com/partdetail/18202-0805W8F1004T5E/C17514 |
| 100 nF across lower leg | Samsung CL05B104KO5NNNC (16 V X7R) | C1525 | Basic | 22,967,471 | 0.0045 | 0402 | https://www.lcsc.com/datasheet/lcsc_datasheet_2304140030_Samsung-Electro-Mechanics-CL05B104KO5NNNC_C1525.pdf | https://jlcpcb.com/partdetail/1877-CL05B104KO5NNNC/C1525 |
| 100 nF (0805 option) | YAGEO CC0805KRX7R9BB104 | C49678 | Basic | 19,622,840 | 0.0189 | 0805 | https://www.lcsc.com/datasheet/lcsc_datasheet_2304140030_YAGEO-CC0805KRX7R9BB104_C49678.pdf | https://jlcpcb.com/partdetail/YAGEO-CC0805KRX7R9BB104/C49678 |

Other Basic parts this board needs (all live-checked):
- 100 kΩ (P-FET gate pull-down, ICS-43434 SD pull-down): 0402 C25741 (9.17M) or 0805 C149504.
- 3 kΩ (TP4057 PROG, 300 mA): 0805 C17661 (1.68M) or 0603 C4211. 4.7 kΩ 0402 C25900.
- 5.1 kΩ, 10 kΩ, 1 kΩ, and 1 / 10 / 22 µF: see the starter research §10. Also 0402 100 nF 50 V C307331 and 0402 1 µF C52923.

Notes:
- The divider draws 4.2 V / 2 MΩ = **2.1 µA** permanently (INFERRED arithmetic). Halving it needs a switched divider (P-FET) and is not worth it at this budget.
- The 100 nF gives the ADC's sample capacitor a low-impedance source (1 MΩ ∥ 1 MΩ = 500 kΩ source, τ = 50 ms). Sample no faster than a few per second (INFERRED).
- 0402 is fine for JLC assembly; the starter board used 0805. Pick one size per board for consistency.

## 10. LiPo cell (bought separately, not on the board)

Target: fits a ~30×55 mm pocket case next to or under a board carrying an 18×25.5 mm module. Protected, with a JST PH lead.

| cell | size (mm) | capacity | protection | price | source |
|---|---|---|---|---|---|
| **Adafruit #3898** | 36 × 17 × 7.8 | 400 mAh (~1.5 Wh) | yes (over-charge, over-discharge at 3.0 V) | $6.95 | https://www.adafruit.com/product/3898 (read 2026-09-29); JST-PH, 25 mm lead; Adafruit says charge at ≤ 400 mA |
| Adafruit #2750 | 36 × 19.6 × 5.2 | 350 mAh (1.3 Wh) | yes, plus short-circuit | $6.95 | https://www.adafruit.com/product/2750 (read 2026-09-29); ~10 cm lead; charge ≤ 350 mA |
| Adafruit #1578 | 29 × 36 × 4.75 | 500 mAh (1.9 Wh) | yes, plus short-circuit | $7.95 | https://www.adafruit.com/product/1578 (read 2026-09-29); 102 mm lead. 29 mm wide is likely too wide inside a 30 mm case |
| Generic "502035"/"602035" pouch | ~5–6 × 20 × 35 | ~250–400 mAh | usually a PCM; must be checked | ~$2–4 | **UNVERIFIED** (AliExpress class; polarity varies, §6) |

- **Pick: #3898 (400 mAh, 17 mm wide),** or #2750 if a thinner case matters. The 300 mA charge setting (§2) is inside both cells' stated limits.
- **Runtime (INFERRED):** ~20 µA asleep drains 400 mAh in about 2 years. Real life is set by recording and Wi-Fi uploads.
- Shipping from Adafruit to Israel was not checked.

---

## Recommended pick per function

| # | Function | Pick | LCSC# | Class | KiCad |
|---|---|---|---|---|---|
| 1 | Microphone | TDK ICS-43434 (I2S, bottom port, 0.5 mm PCB hole) | C5656610 | Extended | exact symbol + footprint, pins match |
| 2 | Charger | TOPPOWER TP4057 (RPROG 3 kΩ = 300 mA) | C12044 | Pref-Ext | symbol `TP4057` matches; set footprint to `SOT-23-6` (default is TSOT-23-6) |
| 3 | Power path | AO3401A P-FET + RB160M-30 Schottky + 100 kΩ gate pull-down | C15127 / C7502715 | Basic / Pref-Ext | `AO3401A` matches; Schottky via `Device:D_Schottky` + `D_SOD-123` |
| 4 | LDO | Richtek RT9080-33GJ5 (2 µA Iq) | C841192 | Extended | no named symbol: use `XC6220B331MR`/`AP2112K-3.3` (same pins) + `TSOT-23-5` |
| 5 | Protection | none on board: require a protected cell. If wanted: DW01A + HJ8205 | C351410 / C20069150 | Extended / Pref-Ext | DW01A matches; HJ8205 needs a custom symbol |
| 6 | Battery connector | JST S2B-PH-SM4-TB(LF)(SN) | C295747 | Extended | exact footprint; + pad number UNVERIFIED |
| 7 | RGB LED | Lite-On LTST-C19HE1WT, anode to VSYS | C458749 | Extended | exact footprint; `Device:LED_RGBA`; pin 4 = anode UNVERIFIED |
| 8 | Buttons | main HYP 1TS009A-1800-5000-CT; reset TS-1088-AR02016 | C319409 / C720477 | Extended / Basic | both exact |
| 9 | Divider | 2 × 1 MΩ 0402 + 100 nF 0402 | C26083 / C1525 | Basic | generic R/C |
| 10 | Cell | Adafruit #3898, 400 mAh, 36×17×7.8 mm, protected, JST PH | — | — | — |

**Loading fees:** unique Extended parts on the whole board are the module, USB-C, mic, LDO, JST, RGB LED and main button: 7 × $3.07 = **$21.49 per assembly order**.
- Using the Basic TS-1187A as the main button saves $3.07, but it is 1.5 mm tall with a light feel.
- Adding on-board protection costs another $3.07 (DW01A).

**Standby budget (INFERRED; the ESP32-S3 deep-sleep figure was not read in this session):**

| Item | Current |
|---|---|
| RT9080 | 2–4 µA |
| TP4057 on battery | 1–2 µA |
| Divider | 2.1 µA |
| Schottky leakage | ≤ a few µA at 4 V (UNVERIFIED) |
| Mic | 0, power-gated (else 12–20 µA) |
| LED | 0 |
| **Board excluding the module** | **~6–10 µA** |

That leaves ~10 µA for the module in deep sleep. Check this against Espressif's deep-sleep figure before freezing.

## UNVERIFIED / to check at schematic time
- **JST PH:** which KiCad pad is + (§6).
- **LTST-C19HE1WT:** that pin 4 is the common anode.
- **T3902:** pin-1 orientation against the Knowles footprint.
- **ICS-43434:** land-pattern dimensions (a figure image).
- **RB160M-30:** leakage at 4 V and Vf at 0.5 A (curves only).
- **XC6220:** its 3.3 V dropout row.
- **AO3401A:** the pinout, inferred from an unnumbered drawing.
- **1TS009A and XL-1615:** their datasheets are images.
- **TP4056, FS8205A, IM69D130, EVQP7A01P:** datasheets not read.
- **ESP32-S3:** deep-sleep current, RTC-GPIO list and max GPIO source current (for powering the mic from a pin).

## What this means for the design (the research agent's summary, 2026-09-29)
- **Loading fees:** nothing in JLCPCB's Basic or Preferred lists covers the mic, the low-Iq LDO, a protector, JST PH, an RGB LED or a 6×6 button, so the board has **7 Extended parts, $21.49 in loading fees per assembly order**.
- **The mic must be power-switched in deep sleep:**
  - Its sleep mode draws 12 µA typ / 20 µA max, which alone is the whole R2 budget.
  - Its L/R select goes to GND, not 3V3 (tying it to 3V3 costs about 33 µA).
- **RGB LED:** the common anode goes on VSYS (battery/USB), not 3V3. Blue and green need up to 3.9 V. Each cathode sinks into a GPIO through its own resistor.
- **Schottky leakage counts against standby:** RB160M-30 ≤ 40 µA at 30 V (much less at 4 V, but only shown on a graph: UNVERIFIED); SS14 0.3 mA at 40 V, so avoid it.
- **On battery, 3V3 follows the cell below about 3.4–3.6 V** (LDO dropout). Firmware shuts down at about 3.3 V.
- **JST PH polarity isn't standard:**
  - Adafruit and SparkFun agree with each other; many other packs are reversed.
  - TP4057 survives a reversed cell (DS pp.7–8).
  - Mark + on the silk; which footprint pad is + is still to be checked.
- **Standby estimate:** about 6–10 µA for the board without the module, which leaves about 10 µA for the ESP32-S3 in deep sleep (Espressif's figure not read: UNVERIFIED).
- **PDM fallback mic:** TDK T3902 (C3171752), 7,928 in stock, $1.29. Its dimensions match KiCad's Knowles footprint; pin 1's corner is unconfirmed.
- **Battery protection:** require a protected cell (e.g. Adafruit #3898, 400 mAh, 36×17×7.8 mm, JST PH); none on the board.
