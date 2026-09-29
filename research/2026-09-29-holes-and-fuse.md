# ESP32 EPAD holes and PTC fuse derating (2026-09-29)

Reviewer: independent research pass. I did not build this design and did not run the pipeline.
All sources below were read on 2026-09-29. Tags: **VERIFIED** = I read it in the source;
**INFERRED** = my reasoning or reading of a chart.

How this was checked:
- The KiCad footprint was read directly from `/usr/share/kicad/footprints/RF_Module.pretty/ESP32-S3-WROOM-1.kicad_mod` (KiCad 10.0 library, `version 20260206`).
- jlcpcb.com pages (the quote form and part pages) were rendered in headless Brave with the `headless-browser` skill. On the quote form I clicked the options and read the "Charge Details" box. I did not upload, save or order anything.
- JLCPCB part status came from part pages and from the JSON API that jlcpcb.com/parts itself calls (`/api/overseas-pcb-order/v1/shoppingCart/smtGood/selectSmtComponentList/v2`), queried from inside the page.
- PDFs were read with `pdftotext`. The figures were rendered with `pdftoppm` and read as images.

Sources:
- **[FP]** KiCad footprint `RF_Module:ESP32-S3-WROOM-1`, local file above.
- **[JCAP]** JLCPCB, *PCB Capabilities*: https://jlcpcb.com/capabilities/pcb-capabilities (no page date shown).
- **[JQ]** JLCPCB instant quote form: https://jlcpcb.com/quote (redirects to cart.jlcpcb.com/quote), rendered 2026-09-29.
- **[JVC]** JLCPCB help, *Via Covering…*: https://jlcpcb.com/help/article/pcb-via-covering ("Last updated on Sep 09, 2026").
- **[JTP]** JLCPCB blog, *PCB Thermal Pad Design & Via Layout*: https://jlcpcb.com/blog/pcb-thermal-pad-design (published Jun 04, 2026).
- **[ESP-M]** Espressif, *ESP32-S3-WROOM-1 & WROOM-1U Datasheet v1.8* (2026-03-02, the latest; v1.7 was 2025-11-18): https://www.espressif.com/sites/default/files/documentation/esp32-s3-wroom-1_wroom-1u_datasheet_en.pdf
- **[ESP-HDG]** Espressif, *ESP32-S3 Hardware Design Guidelines*, "PCB Layout Design" (latest): https://docs.espressif.com/projects/esp-hardware-design-guidelines/en/latest/esp32s3/pcb-layout-design.html
- **[LF]** Littelfuse, *PolySwitch 1206L Series datasheet*, Revised GD 02/26/24: https://www.littelfuse.com/assetdocs/resettable-ptcs-1206l-datasheet?assetguid=2b6a1515-d4ee-4c83-8bd4-152b4901b8f5
- **[BNS]** Bourns, *MF-NSMF Series datasheet*, REV. AF 09/26 (PDF modified 2026-09-23): https://www.bourns.com/docs/Product-Datasheets/mf-nsmf.pdf
- **[JLC-P]** JLCPCB part pages: https://jlcpcb.com/partdetail/764095-JK_nSMD05030/C720075, https://jlcpcb.com/partdetail/C89653, https://jlcpcb.com/partdetail/C883128, https://jlcpcb.com/partdetail/C7542952

---

## Question A: the 0.2 mm holes in the ESP32-S3-WROOM-1 EPAD

### A0. What the footprint actually has [FP]

| Item | Value | Tag |
|---|---|---|
| Copper pad 41 (EPAD) | one `smd rect` pad, 3.9 × 3.9 mm, at (−1.5, 2.46), layers F.Cu + F.Mask, `pad_prop_heatsink` | VERIFIED |
| Holes | **12** extra `pad "41" thru_hole circle` entries: **size 0.6 mm, drill 0.2 mm**, layers `*.Cu` + `F.Mask` (no `B.Mask`), `pad_prop_heatsink` | VERIFIED |
| Hole positions | x ∈ {−2.9, −2.2, −1.5, −0.8, −0.1}, y ∈ {1.06 … 3.86}, alternating rows. These are the 12 gap positions of a 3 × 3 grid at 1.4 mm pitch, all inside the 3.9 mm pad. | VERIFIED |
| Paste | 9 separate `F.Paste` squares, 0.9 × 0.9 mm, 3 × 3 at 1.4 mm pitch. The holes sit in the 0.5 mm gaps between paste squares ("window-pane"), so no paste is printed on a hole. | VERIFIED |
| Bottom side | No `B.Mask` on the holes, so solder mask covers (tents) them on the back. The front is open, inside the EPAD. | VERIFIED |
| Annular ring | (0.6 − 0.2)/2 = 0.2 mm | INFERRED (arithmetic) |
| Aspect ratio at 1.6 mm | 1.6 / 0.2 = 8 : 1 | INFERRED (arithmetic) |

So yes: these are thermal/ground vias inside the EPAD, and the layout copies Espressif's figure (see A2).

### A1. JLCPCB: 2-layer minimum hole, cost, aspect ratio

| Claim | Source | Tag |
|---|---|---|
| 2-layer drill range: "0.15 – 6.3 mm (0.1mm microvias only available for board thickness ≤1mm; ENIG/OSP surface finish required)" | [JCAP] Drill Diameter | VERIFIED |
| 2-layer via: "0.15mm hole size / 0.25mm via diameter" | [JCAP] Via Hole Size | VERIFIED |
| Cost rule: "0.1mm or 0.15mm hole size with any size via diameter, and **0.2mm or 0.25mm hole size with via diameter less than 0.45mm**, will cost more." | [JCAP] | VERIFIED |
| Quote-form tooltip for "Min via hole size/diameter": "additional charge is needed for 0.2mm hole/0.4mm diameter, while 0.3mm hole/0.40mm diameter is free." | [JQ] | VERIFIED |
| PTH annular ring, 2-layer 1 oz: "Recommended 0.25 mm or above; absolute minimum 0.18 mm" | [JCAP] | VERIFIED |
| No aspect-ratio limit is stated. The only thickness condition is for 0.1 mm microvias (≤ 1 mm board). | [JCAP] | VERIFIED (that nothing is stated) |
| So a 0.2 mm hole on a 1.6 mm 2-layer board is within capability, with no refusal. | [JCAP] | INFERRED |

Quote-form options ([JQ], VERIFIED): `0.3mm/(0.4/0.45mm)`, `0.25mm/(0.35/0.4mm)`, `0.2mm/(0.3/0.35mm)`, `0.15mm/(0.25/0.3mm)`, `0.1mm/(0.2/0.25mm)`.

Prices I read from the form, with 2 layers, **50 × 50 mm, qty 5, 1.6 mm, 1 oz, HASL**, all other options left at their defaults. The layer count was left at the form default; that it is 2 is INFERRED from the base price of $4.00. Prices as rendered on 2026-09-29, logged out:

| Min via option | Min-via line | Other lines that appear | Board total |
|---|---|---|---|
| 0.3 mm (default) | none | none | **$4.00** |
| 0.25 mm | $16.75 | Via Covering $16.56, 4-Wire Kelvin Test $16.71 | $54.02 |
| **0.2 mm** | **$16.85** | **Via Covering $16.56, 4-Wire Kelvin Test $16.71** | **$54.12** |
| 0.15 mm / 0.1 mm | $33.70 | same two | $70.97 |

(VERIFIED, read from the "Charge Details" box. A 100 × 100 mm board gave nearly the same numbers: 0.2 mm → $55.60.)

What this means:
- **By JLCPCB's written rule, the footprint's holes, 0.2 mm drill on a 0.6 mm pad (≥ 0.45 mm), are in the free category** (VERIFIED rule; that our holes fall into it is INFERRED).
- **But the form sells "0.2 mm" as a bundle of about +$50, three times the $4 board.** If JLCPCB's gerber review decides the 0.2 mm tool needs the 0.2 mm option, the order cost goes up by roughly $50 or the order is held for re-pricing. I could not test this without uploading gerbers (INFERRED risk).

### A2. Espressif on the EPAD

| Claim | Source | Tag |
|---|---|---|
| Pad 41 = EPAD = GND (Table 3-1) | [ESP-M] p.11 | VERIFIED |
| Figure 11-1, "ESP32-S3-WROOM-1 Recommended PCB Land Pattern" (p.45): a 3.7 × 3.7 mm EPAD drawn as a 3 × 3 grid of 0.9 mm copper squares, with a legend entry "Via for thermal pad" (red circles). I count **12 vias**, one in each gap between squares. **No via drill or diameter is dimensioned.** | [ESP-M] p.45, figure rendered at 600 dpi | VERIFIED (figure) |
| The text says only that the figures give "all the dimensions needed" and points to Autodesk-viewer source files. It gives no written rule on the via count or size. | [ESP-M] §11.1 | VERIFIED |
| "If you need to add a thermal pad EPAD under the chip on the bottom of the module, it is recommended to employ a square grid on the EPAD, cover the gaps with solder paste, and place ground vias in the gaps … This helps effectively reduce solder leakage issues when soldering the module EPAD to the substrate." | [ESP-HDG] "General Principles of PCB Layout for the Chip" → Power Supply | VERIFIED |
| "For optimal grounding, connect the EPAD to a large external ground area using wide traces or copper planes." | [ESP-HDG] same section | VERIFIED |
| The module section ("Positioning a Module on a Base Board") covers antenna placement only; it says nothing on the EPAD or vias. | [ESP-HDG] | VERIFIED |
| Espressif *recommends* vias in the EPAD gaps but never says they are mandatory, and never gives a size. An EPAD with no vias, soldered to top-side GND copper that is stitched to the bottom pour by vias just outside it, meets the only hard requirement (EPAD = GND, joined to a large ground area). It gives up some heat-spreading into the bottom layer. | — | INFERRED |
| KiCad's footprint follows Espressif's pattern: the same 12 gap vias and the 3 × 3 paste grid, on a 3.9 mm pad (Espressif draws 3.7 mm). | [FP] vs [ESP-M] | VERIFIED (both read); the match is INFERRED |

### A3. Solder wicking and JLCPCB via-in-pad guidance

| Claim | Source | Tag |
|---|---|---|
| "Open vias can wick solder away from the thermal pad during reflow, creating voids that reduce thermal contact." Suggested fixes: filled & capped (best), tented (cost-conscious), via-in-pad process. Thermal via size: "0.3 mm (12 mil) for regular design or 0.2 mm (8 mil) for fine pitch." Thermal pads "work on 2-layer boards." | [JTP] | VERIFIED |
| Via-in-pad on JLCPCB = the "Epoxy Filled & Capped" / "Copper paste Filled & Capped" process, for 0.15–0.55 mm holes, and "the default for 6-layer and above". It is not a default on 2 layers. | [JCAP] | VERIFIED |
| Quote-form "Via Covering" options: Tented / Untented / Plugged / Epoxy Filled & Capped / Copper paste Filled & Capped. "Boards with 6+ layers are upgraded to Epoxy Filled & Capped Via for free." | [JQ] | VERIFIED |
| "Single-sided or double-sided vias, holes in pad, and vias with a distance less than 0.35mm from the pad cannot be plugged with ink." | [JVC] | VERIFIED |
| JLCPCB's filled-via processes treat "vias". These holes are footprint pads, and the gerbers carry no plug/fill layer, so on the default 2-layer order they will be plain plated holes: open on top inside the EPAD, and mask-covered on the bottom because the footprint has no B.Mask. | [FP], [JCAP] | INFERRED |
| Wicking risk here is low to moderate. No paste is printed over any hole (window-pane paste), the holes are small (0.2 mm), and the bottom is tented, so solder cannot run through to the back. The 9 paste squares cover about 7.3 mm² of a 15.2 mm² pad (~48 %), which is a normal EPAD paste ratio. A 0.3 mm hole would take slightly more solder than a 0.2 mm one. | [FP] arithmetic | INFERRED |

### Recommendation A

**Make a local copy of the footprint and change the 12 EPAD holes to 0.3 mm drill. Keep the 0.6 mm pad, the same 12 positions and the same paste squares.**

Why:
- A 0.3 mm hole is JLCPCB's no-question default ("0.3mm hole/0.40mm diameter is free", [JQ]). That removes the risk that the 0.2 mm tool pulls the order into the +$50 "0.2 mm" bundle (A1). The ring (0.15 mm) is still ≥ the via rule (0.3/0.4 → 0.05 mm), but below JLCPCB's 0.25 mm PTH recommendation. As a plated via it is fine (INFERRED).
- It keeps Espressif's recommended 12-via, window-pane pattern (A2) and the heat path to the bottom pour.
- A 0.3 mm hole fits the 0.5 mm paste gap with 0.1 mm to each paste square. The bottom stays tented, so wicking stays limited (INFERRED).
- Cost to the project: the pipeline flags a `lib_footprint_mismatch`, which needs a written waiver in DECISIONS.md.

Fallbacks, in order:
1. Keep the stock 0.2 mm holes. JLCPCB's written rule says 0.2 mm on a ≥ 0.45 mm pad is free. At gerber upload, check that "Min via hole size" stays at 0.3 mm and the total stays at the base price. Choose this only if the owner prefers zero footprint edits.
2. Remove the holes from the EPAD. This is electrically acceptable (INFERRED: GND also reaches the module via pads 1 and 40, plus stitching vias beside the EPAD) but goes against Espressif's recommendation, so use it only if the 0.3 mm version fails DRC.

---

## Question B: PTC fuse hot derating

### B1. Published hold-current derating for a 1206 0.5 A PTC

**Littelfuse 1206L050** ([LF] p.2, "Temperature Rerating", hold current in A; rated Ihold at 20 °C still air). Quoted rows:

| Part | −40 | −20 | 0 | 20 °C | 40 | 50 | 60 | 70 | 85 °C |
|---|---|---|---|---|---|---|---|---|---|
| 1206L050 | 0.71 | 0.64 | 0.57 | 0.50 | 0.42 | 0.39 | 0.35 | 0.31 | 0.25 |
| 1206L075/16 | 1.14 | 1.01 | 0.88 | 0.75 | 0.65 | 0.59 | 0.54 | 0.49 | 0.41 |

Note in [LF]: "The temperature rerating data is only for reference." (VERIFIED)

**Bourns MF-NSMF050** ([BNS] p.3, "Thermal Derating Table - Ihold (Amps)"; rated at 23 °C). Quoted rows:

| Part | −40 | −20 | 0 | 23 °C | 40 | 50 | 60 | 70 | 85 °C |
|---|---|---|---|---|---|---|---|---|---|
| MF-NSMF050 | 0.76 | 0.68 | 0.59 | 0.50 | 0.44 | 0.40 | 0.35 | 0.32 | 0.26 |
| MF-NSMF075 | 1.11 | 1.00 | 0.85 | 0.75 | 0.67 | 0.61 | 0.52 | 0.50 | 0.42 |

(VERIFIED)

Derating factors (hold at T / hold at 20–23 °C). The numbers are VERIFIED from the tables; the division is arithmetic:

| T | Littelfuse 0.5 A | Bourns 0.5 A | Littelfuse 0.75 A | Bourns 0.75 A |
|---|---|---|---|---|
| 40 °C | 0.84 | 0.88 | 0.87 | 0.89 |
| 50 °C | 0.78 | 0.80 | 0.79 | 0.81 |
| 60 °C | 0.70 | 0.70 | 0.72 | 0.69 |
| 70 °C | 0.62 | 0.64 | 0.65 | 0.67 |

Applied to our part: Jinrui publishes no table, so I assume it behaves like these two (INFERRED). **At 50–60 °C a 0.5 A PTC holds only about 0.35–0.40 A.** That is the same as our average load (< 0.35 A), so there is no margin. A long stretch of heavy Wi-Fi in a warm box could cause a nuisance trip (INFERRED). A 0.75 A part holds 0.52–0.61 A at 60–50 °C, which is about 50 % above the average load (INFERRED).

### B2. JLCPCB PTC options (status checked on jlcpcb.com)

- **There are no Basic resettable fuses at JLCPCB.** The parts API used by jlcpcb.com/parts returns total = 0 for category "Resettable Fuses" filtered to Basic. The same filter does return Basic parts for a resistor search, so the filter works (VERIFIED).
- **Every 1206 (1,284 parts) and 1210 (634 parts) resettable fuse came back as Extended**, with `preferredComponentFlag = false` (VERIFIED). Caveat: that flag was also false for every common part I sampled, so I cannot vouch for it as a "Preferred" indicator. The part pages I opened all show the badge **"Extended"** (VERIFIED, below).
- **C720075 (JK-nSMD050-30) is Extended** on its jlcpcb.com part page (VERIFIED). So the project already pays the +$3.07 Extended fee for F1, and **a swap to another Extended PTC costs nothing extra** (INFERRED from the owner's fee figure).

Candidates (jlcpcb.com part pages / API, 2026-09-29, qty 1 price):

| LCSC | Maker | MPN | Hold / trip @23 °C | Vmax | R min / R1max | Max time to trip | Published derating? | JLC status | JLC stock | Price |
|---|---|---|---|---|---|---|---|---|---|---|
| **C89653** | **Bourns** | **MF-NSMF075-2** (`-2` = tape & reel [BNS]) | 0.75 / 1.50 A | 6 V | 0.10 / 0.40 Ω | 0.20 s @ 8 A | **Yes**, [BNS] p.3 (table above) | Extended | 17,850 | $0.0368 |
| C720075 (current) | Jinrui | JK-nSMD050-30 | 0.50 / 1.00 A @25 °C | 30 V | 0.15 / 1.0 Ω (R_typ 0.30) | 0.10 s @ 8 A | No | Extended | 259,285 | $0.0334 |
| C883128 | BHFUSE | BSMD1206-075-16V | 0.75 / 1.5 A | 16 V | 0.09 / 0.50 Ω (JLC listing) | 0.2 s (listing) | Not checked: LCSC blocked the PDF download | Extended | 27,523 | $0.0551 |
| C7542952 | LUTE | 1206L075/24NR | 0.75 / 1.5 A | 24 V | 0.09 / 0.50 Ω (listing) | 0.2 s (listing) | Not checked. LUTE is a second-source brand using Littelfuse-style names, not Littelfuse | Extended | 39,154 | $0.0507 |

Bourns values are VERIFIED in [BNS] p.1 (MF-NSMF075 row: Vmax 6 V, Imax 100 A, Ihold 0.75, Itrip 1.50, Rmin 0.10, R1max 0.40, 8.0 A / 0.20 s, 0.6 W). Status, stock and price are VERIFIED on [JLC-P]. The BHFUSE and LUTE electrical values are unverified JLC listing text.

For comparison at 0.5 A: Bourns MF-NSMF050-2 (C75464; 0.50/1.00 A, 13.2 V, derating table in [BNS]) is also Extended. The jlcsearch mirror listed 60,985 in stock at $0.0358; I did not open its jlcpcb.com page. It has the same 0.35–0.40 A hot-hold problem as the current part. Genuine Littelfuse 1206L075/16WR (C371166, $0.2345) and 1210L075YR (C207046) exist on JLCPCB but cost about 6× more (jlcsearch listing, INFERRED current).

### B3. Does 0.75 A hold still protect a USB port?

| Claim | Source | Tag |
|---|---|---|
| MF-NSMF075 trips at ≥ 1.50 A (23 °C) and within 0.20 s at 8 A | [BNS] p.1 | VERIFIED |
| Bourns typical time-to-trip curve, 23 °C: MF-NSMF075 ≈ 4 s @ 1.5 A, ≈ 1 s @ 2 A, ≈ 0.2 s @ 3 A, ≈ 0.05 s @ 5 A. MF-NSMF050 ≈ 8 s @ 1 A, ≈ 0.5 s @ 1.5 A, ≈ 0.2 s @ 2 A. | [BNS] p.4 (figure) | INFERRED (read off a log-log chart) |
| Trip current also falls with temperature, by the same factor as hold (a PTC trips on its own temperature) | general PTC behaviour, consistent with both tables | INFERRED |
| A hard short on the board pulls whatever the host allows (USB hosts current-limit, often at 1–2.5 A or more). At ≥ 2 A the 0.75 A part opens in about 1 s; at 3 A in about 0.2 s. That is fast enough to stop a burnt trace or a hot LDO. It will not keep the draw under 0.5 A for a USB 2.0 host, but neither does the 0.5 A part (it passes up to 1 A indefinitely at 25 °C). | — | INFERRED |
| Voltage drop: 0.40 Ω max × 0.35 A = 0.14 V, less than the JK-nSMD050's 1.0 Ω × 0.35 A = 0.35 V | [BNS], prior research §5 | INFERRED (arithmetic) |
| Vmax 6 V is above USB's 5.25–5.5 V, but with little room for hot-plug ringing. The TVS on VBUS (if present) clamps that. | — | INFERRED |

### Recommendation B

**Switch F1 to Bourns MF-NSMF075-2 (LCSC C89653).**

- It is the only candidate with a real maker's derating table (0.61 A at 50 °C, 0.52 A at 60 °C), and it keeps about 50 % margin over the 0.35 A average in a warm box. The current part drops to about 0.35–0.40 A there (by analogy with Littelfuse/Bourns), which is no margin at all.
- Cost: nothing extra. C720075 is already Extended, and no Basic or Preferred 1206/1210 PTC exists on JLCPCB. $0.0368 vs $0.0334 per part; 17,850 in stock.
- It is still useful as short protection: ≈ 1 s at 2 A, 0.2 s at 8 A. Its lower resistance (R1max 0.40 Ω vs 1.0 Ω) also gives the LDO more headroom.
- Same 1206 footprint (`Fuse:Fuse_1206_3216Metric`); Bourns' body is 3.2 × 1.6 mm, so no layout change (INFERRED).
- If a > 6 V rating is wanted, BHFUSE BSMD1206-075-16V (C883128, 16 V, $0.0551, 27.5k stock) is the fallback. Its derating data was not checked (LCSC blocked the datasheet).

Keeping JK-nSMD050-30 is acceptable only if the enclosure stays below about 40 °C near the fuse. At 40 °C a 0.5 A PTC holds about 0.42–0.44 A.
