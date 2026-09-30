# capture-clip: independent datasheet check of U4 and D2 (2026-09-30)

Scope: the two parts swapped in by `research/2026-09-30-power-path-fix.md`. Read-only check; no build, no other file changed.

**Result: no WRONG pin, pad or rotation found. Both parts are connected correctly as built.** Open items are limits the datasheets do not state.

## Sources

- **[HE]** HEERMICR HE9073, Ver1.3, 2024-01-15, 12 pages (LCSC copy; the vendor's own site did not answer, so no second copy from the manufacturer): https://wmsc.lcsc.com/wmsc/upload/file/pdf/v2/lcsc/2304140030_HEERMICR-HE9073A33M5R_C723789.pdf
- **[EE-U4]** EasyEDA/LCSC library part for C723789 (symbol + footprint `SOT-23-5_L3.0-W1.7-P0.95-LS2.8-BL`): `https://easyeda.com/api/products/C723789/components`
- **[RB2]** ROHM RB168MM-40, Rev.002, 2019-05-28 (LCSC copy): https://wmsc.lcsc.com/wmsc/upload/file/pdf/v2/lcsc/2009111637_ROHM-Semicon-RB168MM-40TR_C509936.pdf
- **[RB3]** ROHM RB168MM-40, Rev.003, 2022-01-18 (rohm.com; same table values): https://fscdn.rohm.com/en/products/databook/datasheet/discrete/diode/schottky_barrier/rb168mm-40tr-e.pdf
- **[EE-D2]** EasyEDA/LCSC library part for C509936 (footprint `SOD-123FL_L2.6-W1.6-LS3.5-RD`)
- **[JLC]** JLCPCB parts API records for C723789, C509936, C15850; LCSC page for C723789 (rendered)
- **[ROT]** `matthewlai/JLCKicadTools` `cpl_rotations_db.csv` (master); `Bouni/kicad-jlcpcb-tools` `CORRECTIONS.md` (main)
- **[BOARD]** `boards/capture-clip/src/circuit.rs` l.198–207, l.240–256; `kicad/capture-clip.kicad_pcb`; `fab/capture-clip-cpl.csv`; KiCad libs in `/usr/share/kicad`

Figures were rendered with `pdftoppm` and read as images: "(figure)".

## What is actually soldered

| Ref | Footprint, KiCad angle | Pad → net (from the `.kicad_pcb`) | CPL rotation |
|---|---|---|---|
| U4 | `SOT-23-5`, 0° | 1 `/VSYS`, 2 `GND`, 3 `/VSYS`, 4 unconnected, 5 `+3V3` | 270 |
| D2 | `D_SOD-123F`, 0° | 1 `/VSYS`, 2 `VBUS` | 0 |
| C3 / C4 | 0805 | `/VSYS`–`GND` / `+3V3`–`GND`, both 10 µF C15850 (Samsung CL21A106KAYNNNE, 25 V X5R) | |

## U4: HE9073A33M5R

| Claim | Result | Source | Note |
|---|---|---|---|
| Pins: 1 VIN, 2 GND, 3 CE, 4 NC, 5 VOUT | **VERIFIED** | [HE] p.2 (figure); [EE-U4] | Figure has no numbers: dot at the VIN corner, VIN/GND/CE on the 3-lead side, VOUT opposite VIN, NC opposite CE. With JEDEC counter-clockwise numbering from the dot that gives 1–5 as claimed. Second, independent source: LCSC's own library symbol for C723789 names pins 1 VIN, 2 GND, 3 CE, 4 NC, 5 VOUT. |
| Pin family | VERIFIED | same | XC6204/XC6220/ME6211/RT9013 style (VIN-GND-EN / NC-VOUT). Not the HT73xx SOT-25 layout (GND-VIN-VOUT), and not XC6206 (3-pin). The SOT23-3 variant of this part (GND, VOUT bottom; VIN top) is XC6206-style: do not substitute "MR" for "M5R". |
| Board nets match the pinout | VERIFIED | [BOARD] | Pad 1 VSYS, 2 GND, 3 VSYS, 4 open, 5 +3V3. |
| Enable pin exists, tied to VIN | VERIFIED | [HE] p.1 circuit, p.5 (VIH ≥ 1.2 V, VIL ≤ 0.4 V), p.8 | p.8 recommends a VIN or VSS level on CE. Iq is specified with EN = VIN. |
| Input absolute maximum 9.0 V | VERIFIED | [HE] p.4 | Operating maximum is 7.0 V (p.4, p.5). LCSC lists "7V". The design's worst surge figure (8.9 V) is above operating range and 0.1 V under the absolute limit. |
| Current limit ≥ the 355 mA peak, guaranteed | **UNVERIFIABLE** | [HE] p.5 | ILIMIT 550 mA typ at VIN 5 V; no minimum. IOUT_MAX "500 mA typ" at 0.5 V headroom; no minimum. Absolute maximum output current is also 550 mA (p.4). |
| Start-up into the load | **UNVERIFIABLE** | [HE] p.5, p.7 Fig. 8 (figure) | "Short/Start load current" 90 mA typ. Fig. 8 shows the output staying low for the whole capture after an over-current event; whether it recovers by itself is not stated. Check at bring-up that 3V3 comes back after a brown-out. |
| Dropout 100 typ / 120 max mV at 100 mA; 200 / 250 mV at 200 mA | VERIFIED | [HE] p.5 | |
| Dropout at 350 / 400 / 450 / 500 mA | VERIFIED (typical only) | [HE] p.6 Fig. 5 (figure) | ~0.45 / 0.54 / 0.65 / 0.77 V. No maximum above 200 mA. The 0.77 V at 500 mA contradicts p.5's "500 mA at 0.5 V": the datasheet is inconsistent with itself. The earlier "0.46 V at 355 mA" stands. |
| Quiescent 0.3 typ / 0.7 max µA | VERIFIED | [HE] p.5; p.6 Fig. 2 (figure) | Flat ~0.3 µA from 1.6 V to 7 V; a bump to ~50 µA between about 0.8 and 1.5 V input (below a protected cell's cut-off). The figure's note (1) is blank in the PDF, so which output voltage it was measured on is unknown. |
| Capacitors: 10 µF in, ≥ 10 µF out | VERIFIED nominal | [HE] p.8; p.1 circuit says ≥ 4.7 µF | Fitted: C3 10 µF, C4 10 µF, plus C5 22 µF on +3V3. |
| Capacitors: real capacitance and ESR limits | **UNVERIFIABLE** | [HE] p.8 | No ESR range or stability chart; p.8 only says low-ESR ceramics are fine and that the real capacitance under bias must be ensured. A 25 V X5R 0805 loses capacitance under bias (INFERRED: C4 roughly 8 µF at 3.3 V, C3 roughly 6–7 µF at 4.5 V), so C4 alone is slightly under "10 µF"; C5 on the same rail covers it. |
| Thermal on USB | VERIFIED limit, INFERRED load | [HE] p.4 | Pd 450 mW, 220 °C/W. At 355 mA from ~4.45 V: 0.41 W, about +90 °C if sustained. Fine for bursts; a continuous 355 mA on USB would sit at the limit. |
| Package vs KiCad `SOT-23-5` | VERIFIED | [HE] p.9; KiCad footprint | Body 2.82–3.02 × 1.5–1.7 mm, span 2.65–2.95, pitch 0.95, foot 0.3–0.6. KiCad pads 1.325 × 0.6 at x ±1.1375, y 0/±0.95 cover 0.475–1.80 mm from centre; lead tip at 1.33–1.48. LCSC's footprint is nearly the same (pads centred ±1.154). Height 1.25 mm max. |
| Stock / class | VERIFIED | [JLC] | Extended, 55,400 at JLCPCB; package "SOT-23-5". |
| CPL rotation 270 (−90 on KiCad 0°) | VERIFIED against LCSC's footprint; not a JLCPCB document | [ROT]; [EE-U4] | The community table has `^SOT-23` −90 (both plugins). Checked independently: LCSC's footprint has pins 1-2-3 along the bottom, pin 1 at the left; KiCad has them down the left side, pin 1 at the top. KiCad's is LCSC's turned 90° clockwise, i.e. −90. JLCPCB publishes no per-package correction list, so the placement preview is still the final check (dot at the VSYS pad nearest the board's top-left of U4). |

## D2: RB168MM-40TR

| Claim | Result | Source | Note |
|---|---|---|---|
| Terminal (1) = cathode, marked by the bar | VERIFIED | [RB2] p.1 Inner Circuit, p.5 notes; [RB3] | |
| KiCad pad 1 = K; pad 1 on VSYS, pad 2 on VBUS | VERIFIED | [BOARD]; `Device:D_Schottky` (pin 1 K, 2 A); [EE-D2] (pad 1 = C) | Conducts VBUS → VSYS, blocks the cell from VBUS. Correct. |
| Package is SOD-123FL | VERIFIED | [RB2] p.1, p.5: "SOD-123FL, [SC-109B], (PMDU)" | JLCPCB lists it as "SOD-123F". |
| Fits `D_SOD-123F` | VERIFIED | [RB2] p.5 (figure + table); KiCad footprint | Part: body 2.6 × 1.6, overall 3.38–3.62, lead width 0.8–1.0. ROHM land: 1.2 wide × 0.85 long at 3.05 pitch (copper from 1.10 to 1.95 mm from centre). KiCad: 1.1 × 1.1 at ±1.4 (0.85 to 1.95 mm). Same outer edge as ROHM's land; 0.1 mm narrower, still wider than the lead. Toe 0.14–0.26 mm. LCSC's own footprint is larger (1.3 × 1.3 at ±1.7). |
| Forward voltage 0.60 typ / 0.65 max V at 1 A, 25 °C | VERIFIED | [RB2] p.1; [RB3] | |
| Forward voltage at 0.5 A, 25 °C | VERIFIED (typical only) | [RB2] p.2 VF-IF (figure) | ~0.56 V; ~0.47 V at 0.1 A. No maximum is given below 1 A; it cannot exceed the 0.65 V at 1 A. |
| Reverse current 0.05 typ / 0.55 max µA at 40 V, 25 °C | VERIFIED | [RB2] p.1 | JLCPCB's "50nA@40V" is the typical. |
| Reverse current at 5 V, 25 °C | VERIFIED (typical only) | [RB2] p.2 VR-IR (figure) | ~0.008 µA. |
| Reverse current at 5 V, 60 °C | **UNVERIFIABLE** | [RB2] p.2 (figure) | No 60 °C curve and no hot maximum. Curves: ~0.008 µA at 25 °C, ~0.9 µA at 75 °C. Log interpolation gives ~0.2 µA typical (INFERRED); the earlier "~2.5 µA worst" is an assumption (typ × 11), not a datasheet figure. |
| Average current 1 A, surge 40 A, 40 V | VERIFIED | [RB2] p.1 | 1 A is for a glass-epoxy mount, 60 Hz half sine. Load here is ≤ ~0.36 A. |
| Stock / class | VERIFIED | [JLC] | Extended, 39,670. |
| CPL rotation 0 | VERIFIED against LCSC's footprint; not a JLCPCB document | [EE-D2]; [ROT] | No table entry for SOD-123F. LCSC's footprint has pad 1 (cathode) on the left, as KiCad's does at 0°, so no correction is expected. Check in the preview: bar toward R6/VSYS side, i.e. the pad at x = 6.5 mm in CPL coordinates. |

## Not checked

- HEERMICR's own website (no response): the pinout rests on the LCSC-hosted datasheet plus LCSC's library symbol, which may share an origin.
- JLCPCB's placement preview (needs an upload).
