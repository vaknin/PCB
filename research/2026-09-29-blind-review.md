# Starter board Rev 0: blind review (2026-09-29)

Reviewer did not build the design. Sources read: `boards/starter/circuit.py`, `layout.py`,
`kicad/starter.kicad_pcb` (pads, tracks, vias and zones dumped with SWIG `pcbnew`, read-only),
`kicad/starter.kicad_pro` (net classes), `reports/{erc,drc,routing}.json`, `fab/` (BOM, CPL,
README, gerber zip and drill file), `research/2026-09-29-parts-starter.md`, `HARDWARE_LESSONS.md`.
Renders of top copper, bottom copper and silk/mask were inspected.

Coordinates are mm from the board's top-left corner, Y down (as in `layout.py`). The raw
pad dump is offset +0.05 mm by the edge-line width; the numbers below are corrected.

Tags: **(V)** verified from the files; **(I)** inferred or from memory, not checked against a datasheet or fab page.

ERC: 0 violations. DRC: 0 errors, 11 warnings, all silk and all covered by the three waivers in `layout.py`; 0 unconnected, 0 parity. **(V)**

## Findings

| # | Severity | Area | Finding | Where | Fix |
|---|---|---|---|---|---|
| 1 | **must-fix** | Manufacturability | Placement rotations for U1, U4, D3, J2, D1, D2, SW1 and SW2 are unverified; there is no correction entry for them **(V, fab/README)**. The dangerous one is **D3**: the SMF5.0A sits on VBUS *upstream* of the fuse, so if it goes on backwards it forward-conducts and shorts the host's USB port directly, with nothing on the board to stop it. A U1 at the wrong rotation kills the board, and a U4 turned 180° puts SDA on VDD and SCL on GND (the sensor won't work). | D3 (17.0, 40.0); U1 (25.0, 12.75); U4 (46.5, 33.0) | In JLCPCB's placement preview, check the cathode bar of D3 against the pad on the VBUS track (pad 1, lower, y 41.4). Also check U1 pin 1 (top-left, pad at 16.25, 7.5), the U4 pin-1 dot (SDA pad at 46.9, 32.3), and the LED cathodes. Don't pay until all eight are confirmed. Record the corrections in HARDWARE_LESSONS. |
| 2 | should-fix | Power | The LDO input cap C1 is about 7 mm from U2's IN pin, and reaching it takes two vias and a detour through B.Cu (U2.3 → via (11.1, 30.2) → B.Cu → via (9.7, 34.5) → C1). The LDL1117 needs its input cap close to IN **(research file: "1 µF in" suggested; placement rule I)**. | C1 (4.0, 33.5); U2 pin 3 (9.8, 29.2) | Move C1 so it sits right next to U2 pin 3 and connects on F.Cu (for example at about (11.5, 27.0), rotated 90°), with its GND pad on the pour. |
| 3 | should-fix | Power / Layout | The bulk caps are in the wrong places. The 22 µF (C2) is at the LDO, and the module only gets 10 µF + 100 nF. The feed from the LDO to module pad 2 is about 15 mm of 0.3 mm track, necked to 0.225 mm at the pad. Espressif's reference circuit puts **22 µF + 0.1 µF at the module** **(V, research §1)**. Under Wi-Fi TX bursts, the module side is where the charge is needed. | C2 (11.0, 33.5); C3 (13.2, 9.0); U1 pad 2 (16.25, 8.76) | Swap the values: make C3 (at the module) 22 µF C45783, and C2 (at the LDO) 10 µF C15850. The BOM lines stay the same and it costs nothing. The LDL1117 maximum C_out is still UNVERIFIED (research file), but the total capacitance doesn't change. |
| 4 | should-fix | Power | The TVS comes before the fuse (J1 → D3 → F1). If a faulty charger holds VBUS above about 6.4 V, D3 conducts with no fuse in its path, so it overheats and usually fails short. A fuse-first order lets the PTC trip and protect both the TVS and the board. | D3 (17.0, 40.0), F1 (17.0, 35.0), VBUS/+5V | Put F1 first and move D3 onto +5V right after F1 (for example at (14.0, 34.0)). ESD clamping is unaffected: the PTC is resistive, not a barrier to fast ESD edges on the downstream side **(I)**. |
| 5 | should-fix | Power / thermal | The LDO tab (OUT, +3V3) has only its own 2×3.8 mm pad and 0.3 mm tracks; the GND pour surrounds it with a 0.3 mm gap. At 5 V in, the LDO dissipates (5.0 − 0.15 fuse − 3.3) × I: **0.78 W at 0.5 A peak and about 0.4 W at 0.25 A sustained Wi-Fi**. The datasheet's 120 °C/W assumes more copper than this **(I)**. So expect a rise of roughly 50–70 °C sustained and above 90 °C during peaks. That heat is 10–15 mm from an R8 module limited to **65 °C ambient** **(V, research §1)**. | U2 (7.5, 26.0) | Add a +3V3 copper zone around the tab on F.Cu, roughly x 1–13, y 18–26 (currently empty pour), and a matching B.Cu zone stitched with 6–9 vias. Give it higher zone priority than GND. Alternatively, accept this for a test board and log it; it will work at desk-level loads. |
| 6 | should-fix | Layout / gates | The **USB net class is never applied**. The patterns are `USB_D+`/`USB_D-`, but the nets are named `/USB_D+`/`/USB_D-` (hierarchical prefix). The D+/D- tracks are 0.2 mm (the Default class), not the class's 0.3 mm, and `routing.json` doesn't flag it because it compares against the class actually applied **(V)**. This is the same failure mode as the "gate passing on the wrong rules" lesson. It is harmless electrically: ESP32-S3 USB is full-speed (12 Mbps), where 0.2 mm is fine. | `layout.py` `NetClass("USB", ...)`; `starter.kicad_pro` netclass_patterns | Change the patterns to `*USB_D*` (or `/USB_D+`). Better, make the pipeline fail when a net-class pattern matches no net. Then either re-route, or drop the class if 0.2 mm is intended. |
| 7 | should-fix | Manufacturability | The drill file has **12 holes of 0.2 mm** (the module footprint's EPAD thermal vias, tool T1). The 2-layer minimum and pricing at JLCPCB for 0.2 mm holes are **(I)**: my recollection is a 0.3 mm standard minimum on 1–2 layers, and the project `.kicad_dru` allows 0.15 mm. These holes also sit inside the EPAD's open mask and paste area, so solder can wick through to the back. | U1 EPAD, around (22.1–24.9, 13.8–16.6) | At upload, check that the quote shows no small-hole surcharge and the DFM check shows no error. If it does, either override those pads to 0.3 mm drill in a local footprint copy (this triggers `lib_footprint_mismatch`, so waive it with a reason), or remove them; GND still reaches the module through pads 1 and 40 and the EPAD paste. |
| 8 | note | Power | Fuse rating. JK-nSMD050-30 holds 0.5 A and trips at 1 A at 25 °C **(V, research §5)**. PTC hold current derates with temperature, to roughly 0.4 A at 50 °C **(I)**. ESP32-S3 TX peaks are short (ms), well below the PTC's thermal time constant, and the average draw is under 0.35 A **(I)**, so it won't nuisance-trip. The worst-case drop is 0.3 Ω × 0.5 A = 0.15 V typ, or 0.5 V after a trip (R1max 1 Ω). That still leaves more than 3.9 V into an LDO with 0.35 V dropout. | F1 | Keep it. If Qwiic accessories will draw more than 100 mA, go to a 0.75 A hold part. |
| 9 | note | Power | The SMF5.0A has a 5.0 V standoff. USB VBUS can legally reach 5.25 V, and some chargers sit at 5.3–5.5 V, which is above standoff and still below the 6.4 V minimum breakdown. Leakage rises but it doesn't clamp **(I)**. | D3 | Acceptable. SMF6.0A would give more margin if a Preferred part exists (not checked). |
| 10 | note | Layout | Antenna: the module is flush with the top edge. The footprint keep-out covers y < 6.0 across x 1–49, with nothing on either copper layer; `routing.json` copper_in_keepouts is empty, and I checked the F.Cu and B.Cu renders **(V)**. The mounting holes H1/H2 at (4, 10) and (46, 10) are about 12 mm from the antenna's side edges. | H1, H2 | Use nylon screws and standoffs in those two holes, or no metal near the antenna. |
| 11 | note | Layout | USB routing. The D+ path is about 29 mm and D− about 34.5 mm including branches (**V**, summed from the track list), a mismatch of about 5 mm. At full speed (12 Mbps, ns edges) that is irrelevant. The pair is not routed as a coupled pair. It runs on B.Cu (x 23.5–24.4, y 23–41) and passes under the gap in F.Cu cut by the +3V3 track at y ≈ 29.4. D− reaches U3 on a stub of about 3.4 mm, while D+ goes through U3's pad; both are fine for ESD at FS. There are two vias under the module body: D+ at (24.4, 24.0) is 0.2 mm from module pad 20, and D− at (23.5, 22.7). They are tented (`tenting front yes`), so they won't short **(V)**. | U3 (25.0, 36.0); U1 bottom-left | Optional: move the two vias to x < 16 (outside the module outline) and put the D+/D− F.Cu tracks in the gap left of the module. Not needed for function. |
| 12 | note | Layout | ESD placement is right: U3 sits between J1 (y ≈ 42) and the module (y ≈ 24), 6 mm from the connector. D3 is about 5 mm from the J1 VBUS pins, on the VBUS track. **(V)** | U3, D3 | none |
| 13 | note | Layout | GND. B.Cu is one piece (77.6% fill) and F.Cu is 3 pieces, all connected (islands are removed); there are 72 stitching vias, and the EPAD is stitched through the footprint's vias **(V)**. Long B.Cu signal tracks cut slots in the bottom plane: EN at x ≈ 14 from y 13 to 42 (29 mm), +3V3 at x 42.2 from y 19 to 37, and USB at x 23.5–24.4. The F.Cu pour bridges them, so this is fine at these speeds. | B.Cu | none required; if re-routing, keep EN and +3V3 on F.Cu |
| 14 | note | Manufacturability | Pour pads are connected solid (zone pad connection = full, **V**). 0805 passives with one pad on the pour and the other on a 0.2 mm track can tombstone, and are harder to rework by hand. | all GND 0805 pads | Set the zone to thermal reliefs for SMD pads, or accept it (JLC reflow usually copes). |
| 15 | note | Layout | The SHT40 sits on the same GND pour as the LDO and module, so board heat conducts straight to it. Expect temperature readings a few °C high **(I)**. After reflow, Sensirion recommends a rehydration/reconditioning step before RH readings are accurate **(I)**. | U4 (46.5, 33.0) | Optional: a milled slot around three sides of U4, with the pour pulled back from it. At bring-up, expect an RH offset for the first days. |
| 16 | note | Layout | J2's two MP (mounting) tabs are left floating, and the router tied them together with a track on the `unconnected-(J2-MountPin-PadMP)` net **(V)**. | J2 (48.2, 21.3)/(48.2, 26.9) | Tie MP to GND in `circuit.py` (a common practice that gives mechanical strength plus a shield). |
| 17 | note | Manufacturability | One +5V via at (11.1, 30.2) is 0.6/0.4 mm (0.1 mm ring) instead of the class's 0.8/0.4. It is within JLC limits **(V, `.kicad_dru` min ring 0.05)** but is a sign that Freerouting ignored the class via. | +5V via | If fixing #2 anyway, this via goes away. |
| 18 | note | Manufacturability | BOM: all 20 assembled refs have the LCSC numbers and values from the research file's picks **(V)**. CPL and gerbers share the bottom-left origin, and the drill file matches the pad positions **(V)**. Silk is clean and doesn't overlap pads; refs are on Fab only; the waived silk is cosmetic **(V, render)**. Copper-to-edge is ≥ 0.5 mm by rule **(V)**. JLC Economic can place C165948 despite its through-hole shell legs **(I, widely used; confirm at BOM upload)**. | fab/ | none |
| 19 | note | Circuit | The pin map was checked against the ESP32-S3-WROOM-1 pin list I know: 2 = 3V3, 3 = EN, 13/14 = IO19/IO20, 25 = IO48, 27 = IO0, 36/37 = RXD0/TXD0, 38/39 = IO2/IO1 **(I, from memory; HARDWARE_LESSONS still lists it as to-verify)**. The SHT4x (1 SDA, 2 SCL, 3 VDD, 4 VSS) and the LDL1117 SOT-223 (1 GND, 2 OUT/tab, 3 IN) mappings match **(I / research file)**. H5VUT2U pin 3 = GND remains inferred. | circuit.py | Close the "Still to verify" line in HARDWARE_LESSONS against the datasheets before ordering. |

## Cost estimate: 5 assembled boards, JLCPCB Economic PCBA, shipped to Israel

Prices come from the research file (JLC @10 tier; @1 for the module, since 5 < 10) and the fee schedule in HARDWARE_LESSONS. Minimum-order and attrition extras on the small parts are estimated **(I)**.

| Item | Basis | USD |
|---|---|---|
| PCB, 5 pcs, 50×50 2-layer, green HASL | JLC promo price **(I)** | 2–5 |
| PCBA setup + stencil | 8.18 + 1.53 | 9.71 |
| Solder joints | ~122 joints/board × 5 × $0.0016 | 0.98 |
| Extended loading fees | 6 unique (U1, J1, U2, F1, U4, J2) × 3.07; the ESD pair is Preferred (no fee) | 18.42 |
| ESP32-S3-WROOM-1-N16R8 | 5 × 5.13 | 25.67 |
| SHT40-AD1B-R2 | 5 × ~1.63–1.80 | 8.2–9.0 |
| LDL1117S33R | 5 × ~0.44–0.55 | 2.2–2.8 |
| JST SH, USB-C, PTC, ESD pair, buttons, LEDs | 1.30 + 0.93 + 0.33 + ~0.9 + 0.21 + ~0.6 (MOQs) | ~4.3 |
| Passives (14/board; MOQ 10–20 each) | ~0.2–0.5 per line | ~3 |
| **Goods subtotal** | | **~74–79** |
| Shipping, FedEx | HARDWARE_LESSONS | ~30 |
| Israeli VAT | 0 if goods ≤ $75 and shipping is on a separate line; otherwise 18% × (goods + shipping) | 0 or ~19 |
| **Total delivered** | | **~$105 (under the $75 line) or ~$125 (over it)** |

**The goods subtotal lands right on the $75 VAT line**, so a few dollars of savings can avoid about $19 of VAT. Ways to drop an Extended fee:

| Swap | Saves | Cost to the design |
|---|---|---|
| F1 PTC → 0 Ω 1206 jumper (a Basic 1206 0 Ω exists, **I**; the footprint stays) | $3.07 + ~$0.3 | Loses overcurrent protection. The host port's own current limit and the TVS remain. Acceptable for a tooling test board, and the cleanest cut. |
| U2 LDL1117 → AMS1117-3.3 C6186 (Basic, same SOT-223 pinout **V/I**) | $3.07 + ~$1.2 | 1.1–1.3 V dropout: at 4.75 V USB minus the fuse drop, the rail can sag to about 3.3–3.4 V under peaks (still inside the ESP32's 3.0–3.6 V). Stability with ceramic-only output caps is UNVERIFIED. Undoes D-007. I would not swap it. |
| J2 Qwiic not assembled (owner solders it) | $3.07 + $1.30 | 1.0 mm-pitch hand soldering: hard for a beginner with a basic iron. Not recommended. |
| U1, J1, U4 | none | No Basic or Preferred equivalent exists (research file). |

Option C (bare PCB, hand-solder everything) is not viable: the module's EPAD and the 1.5 mm DFN SHT40 can't be soldered with a basic iron.

My recommendation: swap F1 for a 0 Ω jumper only if the cart comes out just above $75. Otherwise keep the design as it is. Check the real cart total, with shipping listed as its own line, before deciding.

## Verdict

I would not order this exact file set as-is, but it is close, and I expect it would work if built, provided the placement preview is right. There is no wiring error that would stop it booting or enumerating on USB. The one real hazard is **placement orientation (finding 1)**: the unverified D3 rotation can short the host's USB port, and U1/U4 rotations decide whether the board works at all. That must be checked in JLCPCB's preview before paying. Before regenerating, I'd make the cheap, low-risk fixes:
- move C1 next to the LDO
- swap the 22 µF to the module
- put the fuse before the TVS
- add copper under the LDO tab
- fix the USB net-class pattern, so the gates check what they claim to check

Then confirm at upload that the 0.2 mm EPAD holes cause no surcharge or DFM error. Everything else is a note.
