# capture-clip: power-path fix for D2 leakage and LDO input over-voltage (2026-09-30)

Follows `research/2026-09-30-datasheet-check-capture-clip.md` Problems 1 and 2. Nothing was ordered; no file other than this one was changed.

## Recommendation

| # | Problem | Change | Part | LCSC | Class / stock (JLCPCB API, 2026-09-30) | Footprint |
|---|---|---|---|---|---|---|
| 1 | D2 leaks ~6 µA and more when warm | D2: RB160M-30 → **ROHM RB168MM-40TR** | 40 V, 1 A "super low IR" Schottky | **C509936** | Extended, 39,670, $0.1174 | `Diode_SMD:D_SOD-123F` (ROHM PMDU = SOD-123FL), pad 1 = cathode |
| 2 | RT9080 abs. max 6.5 V under a 9.2 V clamp | U4: RT9080-33GJ5 → **HEERMICR HE9073A33M5R**; C4 1 µF → 10 µF | 3.3 V, 500 mA, 0.3 µA LDO, abs. max input 9 V | **C723789** (C4: C15850, Basic) | Extended, 55,430, $0.1156 | `Package_TO_SOT_SMD:SOT-23-5`, same pin numbers as today |

Also keep the datasheet check's Fix A: **R6 100 kΩ → 10 kΩ** (C17414). It is no longer needed for leakage, only for the unplug gap (C1 discharging through R6).

Effects:
- **Sleep sum:** 8 + 0.7 + 2 + 1.4 + 0.55 = **12.65 µA** (was 21.4 µA with the corrected D2 figure), against the 20 µA limit.
- **Money (owner's OK needed):** D2 moves from Preferred Extended to Extended, so the board has **8 Extended parts instead of 7: +$3.07 per assembly order**. Per board: D2 +$0.086, LDO −$0.004, C4 +$0.06.
- **Cost on the battery side:** the new LDO drops more. Wi-Fi bursts need a cell of about 3.55 V under load instead of about 3.35 V (INFERRED, see below). Recording and sleep are unaffected down to 3.4 V.
- **Not done by this change:** a scope check of VSYS on hot-plug at bring-up is still worth doing, but the numbers no longer depend on it.

## How this was checked

- Stock, class and price: JLCPCB parts API (`selectSmtComponentList`), queried 2026-09-30. `base` = Basic, `expand` + `preferredComponentFlag` = Preferred Extended, otherwise Extended.
- Full Basic and Preferred Extended Schottky lists were enumerated the same way (keyword "Schottky" with each filter).
- Datasheets from LCSC's `wmsc.lcsc.com` mirror, read with `pdftotext -layout`; figures rendered with `pdftoppm` and read as images: marked "(figure)".
- Could not open: **Holtek HT7833** datasheet (every LCSC/Holtek URL returned HTML). It is not assessed from a datasheet.

Sources:
- **[RB168]** ROHM RB168MM-40, Rev.002, 2019/05/28: https://wmsc.lcsc.com/wmsc/upload/file/pdf/v2/lcsc/2009111637_ROHM-Semicon-RB168MM-40TR_C509936.pdf
- **[HE]** HEERMICR HE9073, Ver1.3, Jan 2024: https://wmsc.lcsc.com/wmsc/upload/file/pdf/v2/lcsc/2304140030_HEERMICR-HE9073A33M5R_C723789.pdf
- **[ME]** Microne ME6231, V03 (Chinese): https://wmsc.lcsc.com/wmsc/upload/file/pdf/v2/lcsc/2010181634_MICRONE-Nanjing-Micro-One-Elec-ME6231C33M5G_C610253.pdf
- **[AO]**, **[RT]**, **[TP]**, **[SMF]**, **[RB]**: as in the datasheet check.

## Problem 1: D2

### RB168MM-40TR, from the datasheet

| Fact | Value | Source |
|---|---|---|
| Reverse current, max | **0.55 µA at 40 V, 25 °C** (0.05 typ) | [RB168] p.1 table |
| Reverse current at 5 V, 25 °C | ~0.008 µA typ | [RB168] p.2 VR-IR (figure) |
| Reverse current at 5 V, 75 °C | ~0.9 µA typ | [RB168] p.2 VR-IR (figure) |
| Reverse current at 5 V, 60 °C | ~0.2 µA typ; ~2.5 µA worst | **INFERRED**: log interpolation between the 25 and 75 °C curves; worst = typ × 11 (the table's max/typ ratio). No hot maximum is tabulated |
| Forward voltage | 0.60 typ / 0.65 max V at 1 A | [RB168] p.1 table |
| Forward voltage at lower current, 25 °C | ~0.48 V at 0.1 A, ~0.55 V at 0.355 A, ~0.58 V at 0.7 A | [RB168] p.2 VF-IF (figure) |
| Ratings | 40 V, 1 A average, 40 A surge | [RB168] p.1 |
| Package | SOD-123FL (PMDU); marking bar = cathode; land 1.2 × 0.85 mm pads at 3.05 mm pitch | [RB168] p.5 (figure) |

- **Footprint:** KiCad `D_SOD-123F` has 1.1 × 1.1 mm pads at ±1.4 mm, which covers the leads (overall length 3.5 mm). It is the footprint D3 already uses, and its 3D model is already in `lib/3dmodels`.
- **Current:** the charger's VCC is on VBUS, ahead of D2. D2 carries only the board's current (355 mA peak plus about 8 mA of lights), not 0.7 A. The 1 A rating covers both readings.
- **USB headroom:** 4.75 V − 0.65 V = 4.10 V at VSYS. The new LDO needs 3.3 + 0.46 = 3.76 V at 355 mA. At 4.5 V (far end of a lossy cable) the margin is about 0.1 V; below that the rail sags slightly during Wi-Fi bursts and stays above the module's 3.0 V.
- **Charger:** unaffected by the diode's drop.
- **VBUS node lift:** 2.5 µA × 9.6 kΩ (R6 10 k ∥ 250 k) = 24 mV worst at 60 °C. With R6 left at 100 kΩ it would be 0.18 V: also fine.

### Why not a cheaper or fee-free part

- **No Basic or Preferred Extended Schottky fits.** Of 6 Basic and about 190 Preferred parts, every one rated ≥ 0.5 A lists 20 µA to 1 mA. The low-leak ones (BAS40, BAS70, BAT43, RB751: 0.1–0.5 µA) are 70–200 mA parts.
- **Second AO3401A used as a diode (gate tied to source; Basic, no fee):**
  - Leakage is tabulated: IDSS ≤ 1 µA at −30 V, 25 °C and ≤ 5 µA at 55 °C ([AO] p.2).
  - But the body diode drops 0.7 typ / 1.0 max V at 1 A ([AO] p.2). VSYS on USB would be 3.75–4.05 V, under the 3.76 V the LDO needs at the low end. Rejected.
- **Ideal-diode IC, TI LM66100DCKR (C2869734, Extended, 35,867, $0.23):** absolute maximum input is 6 V (INFERRED from memory, datasheet not read today), so it would bring Problem 2 back on a second part. Same loading fee as the diode. Rejected.
- **Other RB168 variants:** RB168MM-30TR has 0 stock; RB168MM-60TR 59; RB168VAM-60TR 4.

## Problem 2: LDO input over-voltage

### Why the other two options don't close it

- **Lower-clamp TVS:** the SMF5.0A does not start conducting until 6.4–7.0 V at 10 mA ([SMF] p.2). Any TVS that stands off 5.5 V breaks down at or above the RT9080's 6.5 V limit, so no TVS of this kind can protect it. Sleep and cost: none, but no fix.
- **Series resistor or ferrite:**
  - A 1 m cable is roughly 1 µH (INFERRED). Against C1 + C3 = 11 µF, critical damping needs 2·√(L/C) ≈ 0.6 Ω in the VBUS line: **0.42 V lost at 0.7 A**, and the charger sees it too.
  - A ferrite adds inductance at the ring frequency (tens of kHz), not loss; it can make the ring worse (INFERRED).
  - A snubber across VBUS (1 Ω + 4.7 µF) costs no drop, but pushes VBUS capacitance past USB's 10 µF inrush limit.
  - All of these only damp the hot-plug ring ([TP] p.10: ceramic input capacitors can produce high transients on hot-plug). None bounds a real surge, where the TVS sits at up to 9.2 V for longer than C3 takes to charge. Sleep cost: none. Not a fix by the numbers.

### HE9073A33M5R, from the datasheet

| Fact | Value | Source |
|---|---|---|
| Input, absolute maximum | **9.0 V** | [HE] p.4 |
| Input, operating | 2.0–7.0 V | [HE] p.4, p.5 |
| Quiescent current | **0.3 typ / 0.7 max µA** | [HE] p.5 |
| Quiescent current below 3.3 V in | flat ~0.3 µA from 1.6 V to 7 V, no load | [HE] p.6 Fig. 2 (figure, typical) |
| Output current | 500 mA typ at VIN − VOUT = 0.5 V; limit 550 mA typ; abs. max 550 mA | [HE] p.4, p.5 |
| Dropout, 3.3 V | 100 typ / 120 max mV at 100 mA; 200 / 250 mV at 200 mA | [HE] p.5 |
| Dropout at 355 mA | ~0.46 V typ | [HE] p.6 Fig. 5 (figure) |
| EN | high ≥ 1.2 V, low ≤ 0.4 V; tie to VIN allowed | [HE] p.5, p.8 |
| Capacitors | application circuit ≥ 4.7 µF in and out; text asks for 10 µF each, close to the pins | [HE] p.1, p.8 |
| Pins, SOT23-5 | VIN, GND, CE on the three-pin side (dot at VIN); VOUT above VIN, NC above CE | [HE] p.2 (figure, no pin numbers) |
| Package | SOT23-5, 2.9 × 1.6 mm body, height 1.05–1.25 mm | [HE] p.9 |

- **Pin numbers:** 1 VIN, 2 GND, 3 CE, 4 NC, 5 VOUT: **INFERRED** from the dot and standard SOT-23-5 numbering. It equals the RT9080 and the KiCad `XC6220B331MR` symbol, so no net changes.
- **Over-voltage by the numbers:** the TVS clamps at ≤ 9.2 V at its full 21.7 A pulse ([SMF] p.2). D2 drops at least 0.3 V even at 1 mA ([RB168] p.2, figure). VSYS ≤ 8.9 V, under 9.0 V. The margin at that extreme is thin (0.1 V). For a hot-plug ring (a few amps, TVS near 7–7.5 V, INFERRED) it is 1.5–2 V.
- **Current:** 355 mA peak against 550 mA typ. No minimum is given for the limit (**UNVERIFIED**); the RT9080 guaranteed 610 mA. Check at bring-up: 3V3 must hold during a Wi-Fi burst on USB.
- **Start-up:** short-circuit foldback is 90 mA ([HE] p.5). Charging about 33 µF to 3.3 V at 90 mA takes ~1.2 ms, inside the EN delay (R7/C7, 10 ms) (INFERRED).
- **Output capacitor:** C4 becomes 10 µF (C15850, already on the board as C2/C3) so the requirement is met at the regulator itself, not only by C5 at the module.
- **Height:** 1.25 mm max against the TSOT's 0.9 mm. The module and button are taller, so the case is unaffected (INFERRED; re-run the case check).
- **3D model:** `lib/3dmodels` has `TSOT-23-5.step` but no `SOT-23-5.step`; it must be added to `MANIFEST.json`.
- **Vendor:** HEERMICR is a small Chinese vendor and its datasheet is thin. That is the main weakness of this pick.

### Dropout: the price on battery

| At 355 mA | RT9080 | HE9073 |
|---|---|---|
| Dropout | ~0.18 V typ, 0.31 V max (INFERRED, scaled from 0.31 / 0.53 V at 600 mA, [RT] p.4) | ~0.46 V typ ([HE] Fig. 5) |
| 3V3 rail at a 3.4 V cell (Q1 drops 0.03 V) | 3.06–3.19 V | ~2.91 V |
| Cell needed for a 3.0 V rail during a Wi-Fi burst | ~3.2–3.35 V | **~3.5–3.55 V** |

- The module's minimum is 3.0 V. The firmware should stop Wi-Fi uploads below about 3.55 V measured under load, and keep recording (under 100 mA, ~0.1 V dropout) down to 3.4 V.
- A LiPo has roughly 5–10 % of its charge left between 3.55 V and 3.35 V (INFERRED). That part becomes record-only.
- On USB nothing changes.

### LDOs rejected

| Part | LCSC | Class / stock | Why not |
|---|---|---|---|
| Microne ME6231C33M5G | C610253 | Extended, 43,150, $0.0755 | 20 V abs. max, 1.8 / 3.6 µA, same pins ([ME] p.2–4), same ~0.46 V dropout at 355 mA (Fig. 3). But **ground current rises to ~420 µA when the input is under 3.3 V** ([ME] p.5 Fig. 4, figure): a cell below 3.3 V asleep would be drained at 20× the budget. Operating minimum is 3 V. |
| Holtek HT7833 (SOT-23-5) | C164106 | Extended, **2,842**, $0.34 | Datasheet could not be opened. Listing: 8 V, 4 µA, 360 mV at 500 mA. Thin stock; pinout **UNVERIFIED** (believed not to match). |
| TI TPS7A2633DRVR | C2876256 | Extended, 383, $3.44 | 18 V, 2 µA, but WSON-6, 590 mV at 500 mA (listing), thin stock, expensive. |
| Torex XC6220, ME6217, XC6227, XC6210, AP7366, RT9078, LP5912 | | | Input limit 5.5–6.5 V, or 25–100 µA quiescent, or 300 mA (JLCPCB listing values, **UNVERIFIED** against datasheets). |

No part found at JLCPCB has all of: ≥ 8 V rating, ≤ 5 µA, ≥ 500 mA and the RT9080's dropout.

## Exact edits

### `boards/capture-clip/src/circuit.rs`

1. Header comment, line 4: `a 2 µA LDO` → `a 0.3 µA LDO`.
2. Line 193:
   `r(&mut c, "R6", "10k", vbus, gnd, path); // gate pull-down; 10k empties C1 in ~10 ms on unplug`
3. Lines 194–199 (D2):
   ```rust
   // RB168MM-40 Schottky (<= 0.55 µA at 40 V), SOD-123FL, pad 1 = cathode
   let d2 = c
       .part("D2", "Device:D_Schottky", "RB168MM-40", "Diode_SMD:D_SOD-123F")
       .lcsc("C509936")
       .mpn("RB168MM-40TR")
   ```
4. Lines 235–241 (U4): comment to `HE9073A33M5R (SOT-23-5): 1 VIN, 2 GND, 3 CE, 4 NC, 5 VOUT; input abs. max 9 V`, and
   ```rust
   .part("U4", "Regulator_Linear:XC6220B331MR", "HE9073A33M5R", "Package_TO_SOT_SMD:SOT-23-5")
   .lcsc("C723789")
   .mpn("HE9073A33M5R")
   ```
5. Line 249: `cap(&mut c, "C4", "10u", v3, gnd, reg); // HE9073 asks for 10 µF at the output`
6. Line 248 comment: `// 10 µF at the input (HE9073 DS p.8)`. Value unchanged.
7. Line 114 comment: `the LDO to ~1.1 A` → `the LDO to ~0.55 A`.

Nets and pin numbers do not change. `layout.rs` and the `.kicad_pcb` need regenerating for the two footprints; add `Package_TO_SOT_SMD.3dshapes/SOT-23-5.step` to `lib/3dmodels/MANIFEST.json`.

### `boards/capture-clip/board.toml`

- Both `[[power.source.limit]]` blocks named "RT9080 LDO output":
  ```toml
  name = "HE9073 LDO output"
  ma = 500
  source = "HE9073 datasheet p.5: 500 mA typical at 0.5 V dropout; limit 550 mA typical, no minimum given (research/2026-09-30-power-path-fix.md)"
  ```
  Sums still pass: USB 468 mA, battery 364 mA.
- Battery source: `what = "single-cell protected LiPo (JST PH) through the AO3401A power path and the HE9073 LDO"`.
- Load "USB sense divider and gate pull-down": `ma = 0.55`, source `"5.25 V / 250k + 5.25 V / 10k, from the circuit"`.
- Load "LDO's own current and the battery divider": source `"HE9073 Iq 0.7 µA max (datasheet p.5) + 4.2 V / 3 M; rounded up"`.
- Sleep load, LDO:
  ```toml
  name = "HE9073 LDO quiescent current"
  ua = 0.7
  source = "VERIFIED maximum (0.3 typical): HE9073 datasheet p.5. In dropout: flat in Fig. 2 p.6 (typical, figure only)"
  ```
- Sleep load, D2:
  ```toml
  name = "Schottky D2 reverse leakage into the 10k pull-down"
  ua = 0.55
  source = "VERIFIED maximum at 40 V, 25 °C: RB168MM-40 datasheet p.1. At 5 V: ~0.008 µA at 25 °C, ~0.9 µA at 75 °C typical (Fig. p.2); ~0.2 typical / ~2.5 worst at 60 °C is INFERRED"
  ```
- Requirement R2 `covered_by`: add `"part:D2"` (optional).

## Still to check at bring-up

- 3V3 during a Wi-Fi burst on USB and on a cell near 3.6 V (the unguaranteed current limit and the higher dropout).
- VSYS on a scope while hot-plugging a 1–2 m cable.
- Sleep current at 3.3–3.5 V cell (dropout behaviour is from a typical figure only).
- D2's band orientation and U4's pin 1 in JLCPCB's placement preview.
