# Hardware lessons

Read at the start of every session. Only verified facts; each with source and date.
Anything unverified goes in `DECISIONS.md` open questions or is tagged UNVERIFIED.

## Verified part data
| Part | Fact | Source | Date |
|---|---|---|---|
| ESP32-S3-WROOM-1-N16R8 (LCSC C2913202) | JLCPCB **Extended** part (+$3.07 loading fee in Economic), $5.13 @1 | jlcpcb.com/partdetail/3198300 | 2026-09-29 |
| H5VUT2U (C20615824) | SOT-23: pins 1 and 2 are the I/O lines, pin 3 is the common pin (GND inferred from the "C(I/O–GND)" spec); 5 V working voltage, 0.6 pF typ | hongjiacheng datasheet Rev 2.0, p.1 (function diagram is an image) | 2026-09-29 |
| SMF5.0A (C19077497) | SOD-123FL, 200 W (10/1000 µs), unidirectional | hongjiacheng SMF series datasheet p.1 | 2026-09-29 |
| KiCad `Diode:SMF5V0A` symbol | pin 1 is the cathode (bar side of the graphic), matching KiCad's diode footprint pad 1 = K | read from `/usr/share/kicad/symbols/Diode.kicad_sym` | 2026-09-29 |
| KiCad `Sensirion_DFN-4_..._SHT4x_NoCentralPad` | carries its own F.Cu keep-out that leaves only a pad-sized notch at each pad (pads 0.5×0.3 mm, 0.8 mm pitch) | footprint file + DSN export | 2026-09-29 |
| KiCad `SW_Push_1P1T_XKB_TS-1187A` | 4 pads numbered 1,1,2,2, so the 2-pin `Switch:SW_Push` symbol maps correctly | footprint file | 2026-09-29 |
| KiCad `USB_C_Receptacle_HRO_TYPE-C-31-M-12` | footprint origin at the body centre; body (F.Fab) ends 3.7 mm below the origin, so place it at `H - 3.7` for a flush mouth | pcbnew bounding boxes | 2026-09-29 |
| KiCad `JST_SH_SM04B-SRSS-TB_...Horizontal` at rot 90 | mouth faces +x; signal pads on the inner side, MP tabs near the mouth | pcbnew pad positions | 2026-09-29 |

| ESP32-S3-WROOM-1 | pads 1 GND, 2 3V3, 3 EN, 13 IO19 = USB D−, 14 IO20 = USB D+, 25 IO48, 27 IO0, 36 RXD0, 37 TXD0, 38 IO2, 39 IO1, 40 GND, 41 EPAD = GND | Espressif WROOM-1 datasheet v1.8, Table 3-1 pp.11–12 | 2026-09-29 |
| ESP32-S3-WROOM-1 | EN RC: 10 kΩ + 1 µF. Strapping: IO46/IO45 internal pull-downs are right for N16R8 (3.3 V flash); IO3 is ignored unless the JTAG eFuse is burned. IO48 is 1.8 V only on R16V modules. Wants 22 µF + 0.1 µF at the 3V3 pad; Wi-Fi TX peak 355 mA | WROOM-1 §9 p.41, Tables 4-1..4-5 pp.13–15, Fig. 9-1, p.28 | 2026-09-29 |
| SHT40-AD1B | 1 SDA, 2 SCL, 3 VDD, 4 VSS; KiCad DFN-4 pad numbering matches; I²C 0x44; pull-ups ≥ 390 Ω; no copper under the sensor except its pads | Sensirion SHT4x datasheet v2, p.1, p.7, §5.4 p.13, Figs. 10–11 | 2026-09-29 |
| LDL1117S33R | 1 GND, 2 OUT (tab = OUT), 3 IN; C_in ≥ 1 µF; dropout ~0.2–0.3 V at 0.5 A (typical, from a plot). The stability plot covers C_out 1–22 µF only, with no stated maximum | ST DocID030319 Rev 3, Fig. 2/Table 1 p.6, §6.2 p.10, Figs. 12, 20–21 | 2026-09-29 |
| HRO TYPE-C-31-M-12 | VBUS A4/A9/B4/B9, GND A1/A12/B1/B12, CC1 A5, CC2 B5, D+ A6/B6, D− A7/B7, SBU A8/B8; KiCad pad order matches HRO's layout drawing | HRO drawing (LCSC mirror) | 2026-09-29 |
| H5VUT2U | pin 3 = GND **confirmed** ("pin1 or pin2 to pin3", "I/O pin to GND") | Hongjiacheng datasheet Rev 2.0, p.2 | 2026-09-29 |
| SMF5.0A | cathode band; VRWM 5 V, VBR 6.4–7.0 V, clamps at 9.2 V | SMF datasheet Rev 2.2, p.2 | 2026-09-29 |
| Qwiic (JST SH 4-pin) | 1 GND, 2 3.3 V, 3 SDA, 4 SCL (from SparkFun's design file; their web pages give only wire colours) | github.com/sparkfunX/Qwiic_Adapter Eagle schematic | 2026-09-29 |
| JK-nSMD050 PTC | 0.5 A hold / 1 A trip at 25 °C, up to 1 Ω after soldering; no derating table (a hot enclosure could cause nuisance trips, inferred). Replaced on the starter by MF-NSMF075-2 (D-015) | Jinrui datasheet, Table 2 | 2026-09-29 |
| Bourns MF-NSMF075-2 (C89653) | 1206 PTC: 0.75 A hold / 1.5 A trip at 23 °C, 6 V, R1max 0.40 Ω, trips ≤ 0.2 s at 8 A. Hold vs ambient: 0.67 A at 40 °C, 0.61 A at 50, 0.52 A at 60, 0.50 A at 70. Its 0.5 A sibling (MF-NSMF050) holds 0.40 A at 50 °C and 0.35 A at 60; Littelfuse 1206L050 is about the same | Bourns MF-NSMF datasheet REV AF, p.1 and derating table p.3; Littelfuse 1206L datasheet p.2 | 2026-09-29 |
| KiCad `RF_Module:ESP32-S3-WROOM-1` | EPAD (pad 41) is a 3.9 mm SMD pad plus 12 plated holes (0.2 mm drill, 0.6 mm pad, tented on the back) in the gaps of a 3×3 grid of 0.9 mm paste squares. This matches Espressif's land pattern (Fig. 11-1: 12 "via for thermal pad", no size given). Espressif's design guidelines recommend vias in the gaps but don't require them | footprint file; WROOM-1 datasheet v1.8 p.45; ESP32-S3 HW design guidelines, PCB layout | 2026-09-29 |

Full table with URLs: `research/2026-09-29-datasheet-check.md` (checked by a separate agent that didn't build the design).

(Pinouts, voltage ranges and footprints get added here as each is checked against its datasheet.)

## Fab rules and prices
- **JLCPCB Economic PCBA:**
  - setup $8.18, stencil $1.53, $0.0016 per joint
  - $3.07 per unique Extended part; Basic and Preferred-Extended parts are free
  - (2026-09-09 help page)
- **JLCPCB small holes (2 layers, 50×50, qty 5):**
  - The capabilities page says only 0.1/0.15 mm holes, and 0.2/0.25 mm holes on pads under 0.45 mm, cost extra.
  - The quote form's "Min via hole size" option is different: 0.3 mm is free ($4.00 board). 0.2 mm adds $16.85 plus $16.56 via covering plus a $16.71 Kelvin test ($54.12).
  - So keep every hole ≥ 0.3 mm (the pcbgen ESP32 footprint does, D-015).
  - (quote form and capabilities page, 2026-09-29)
- **JLCPCB has no Basic resettable (PTC) fuses.** Every 1206/1210 PTC is Extended, including JK-nSMD050-30 (C720075) and MF-NSMF075-2 (C89653) (jlcpcb.com parts search and part pages, 2026-09-29).
- **JLCPCB → Israel:**
  - FedEx ~$30 (6–9 business days), DHL ~$102.
  - No DDP, so VAT is paid on import. An Israeli ID is needed for customs.
- **NextPCB Rev 0 free assembly:**
  - minimum board 50×50 mm, 5 or 10 boards only, parts only from HQ Online, green only
  - (2026-09-29)
- **Israel:**
  - VAT exemption $75, goods only, and only if the invoice lists shipping separately.
  - Above that, 18% VAT on goods plus shipping.
  - (since 2026-06-02)

## Tool gotchas
- **Laptop (verified 2026-09-29):** `kicad-cli` 10.0.6; Rust 1.98.1 from the repo's `mise.toml`.
  Libraries are in `/usr/share/kicad/{symbols,footprints}`; no 3D models yet.
  pcbgen no longer loads KiCad's SWIG `pcbnew` (D-016), so KiCad 11 dropping it breaks nothing.
- **tscircuit autorouter:**
  - It can report success and still leave shorts (overlapping vias, a via on a pad). Always run `tsci check shorts` and an independent KiCad DRC.
  - It does not enforce USB differential pairs.
  - Pin versions: it releases several times a day.
- **tscircuit → KiCad export:**
  - The exported schematic fails KiCad ERC.
  - Pad-number collisions can merge pads.
  - Board cutouts are dropped from gerbers.
- **kicad-cli (v10):** it cannot export Specctra DSN or import SES. pcbgen writes the DSN and reads the SES itself (D-016).
- **KiCad 10 IPC API:** it needs the GUI running. pcbgen edits the files directly instead.
- **`kicad-cli pcb upgrade --force`** re-saves a board in KiCad's own format even when it is already current: a quick "does KiCad parse what we wrote" check (the `pcb` stage runs it). `sch upgrade` does the same for schematics.
- **KiCad 10 board-file conventions (verified 2026-09-29 against pcbnew-saved boards):**
  - Nets are written by name only (`(net "GND")`); there is no net table.
  - In a footprint, pad and graphic positions stay in the footprint's frame, but pad, property and text *angles* include the footprint's rotation (a pad at 0 in a footprint at 270 is saved at 270).
  - Zones inside a footprint (keep-outs) are saved in board coordinates.
  - KiCad saves footprints ordered by UUID, so a line diff of two boards is useless; compare footprints by reference.
  - A hidden field with no `(thickness)` gets 0.15 mm on load; write `(thickness 0)` to keep pcbnew's.
  - Rotating a footprint-frame point into the board: (x·cos + y·sin, −x·sin + y·cos), angle CCW, Y down.
- **KiCad 10 bottom-side footprints (verified 2026-09-29 against boards pcbnew saved after `fp.Flip(pos, FLIP_DIRECTION_LEFT_RIGHT)`; pcbgen's `footprint::flip` reproduces them item by item):**
  - A flipped footprint keeps its library items in its own frame with every Y negated (`at`, `start`/`end`/`mid`/`center`, `xy`, pad primitives, drill `offset`); the footprint's `(layer "B.Cu")` and its `at` angle then work as on the top. Zones in it are in board coordinates, as on the top.
  - F.* ↔ B.* on every layer name; `*.Cu`, `F&B.Cu` and user layers stay.
  - Pads: relative angle a → −a; trapezoid `rect_delta` dy → −dy; chamfer corners top ↔ bottom. The 3D `model` entry is left unchanged.
  - Texts and fields: relative angle a → −a; if that is under 180° (mod 360) it turns a further 180° and keeps its justification, otherwise left/right and top/bottom swap. `mirror` toggles only on F./B. layers (a Cmts.User text stays unmirrored). An `(unlocked yes)` text becomes 180° − a and keeps its justification.
  - Flip-then-rotate and rotate-then-flip differ: flipping a part at angle r gives angle 180 − r, with texts turned 180° from flip-then-rotate. pcbgen's `at_bottom(x, y, rot)` is flip at 0, then turn to `rot` (the angle KiCad shows).
  - Past 0/90/180/270, KiCad re-saves a flipped `fp_rect` as an `fp_poly` (same corners, its own point order).
  - **DSN:** KiCad's `ExportSpecctraDSN` writes a bottom part's image as the unflipped (top-side) footprint, with top-side layers, and places it `back` at its angle + 180. A top and a bottom copy share one image. Freerouting puts the pins where the board has them, on the back layer (checked with Freerouting's own DSN reader, 2026-09-29).
  - `kicad-cli pcb export pos` gives a bottom part's raw KiCad angle, and X is not negated unless you pass `--bottom-negate-x`.
- **Pads with a drill `offset`:** the pad's position is the hole; the copper shape is shifted by the offset in the pad's frame. KiCad's DSN export shifts the padstack shapes and names them `Oval[A][dx,dy]Pad_...`; pcbgen does too, since D-018; before that it wrote the copper centred on the hole.
- **Freerouting 2.4.x:**
  - It needs Java 25.
  - KiCad's DSN export omits board-edge clearance, so pass `--router.copperToEdgeClearanceUm=500`.
- **atopile 0.15.x:** it cannot read KiCad 10-saved boards.
- **Freerouting and copper pours:** zones go into the DSN as `plane`s, and Freerouting then counts every pad on that net as connected and routes none of them. Take the pours out of the board before the DSN export (D-012).
- **Freerouting is deterministic (verified 2026-09-29):** the same DSN gave the same score, violations and track widths on repeated runs, for both the Python and the Rust DSN. The "run to run variation" seen before came from pcbnew giving footprints random UUIDs, which reordered the DSN. The footprint order steers the result, so pcbgen routes several seeded orders and keeps the best (D-017).
- **Freerouting's fanout stage narrows tracks** to 3/4 or 3/5 of their class width (0.225 mm on a 0.3 mm class, 0.15 on 0.2); read from its code, verified 2026-09-29. With `--router.fanout.enabled=false` the starter board routes with none of that and fewer vias. `router.automaticNeckdown=false` does not stop it. pcbgen turns fanout off (D-017).
- **Freerouting 2.4.1 settings on the command line** are its Java field names, not the JSON names: `--router.scoring.viaCosts=50`, `--router.fanout.enabled=false`, `--router.resultJsonPath=FILE` (a JSON summary: score, clearance violations, trace and via counts). A wrong name only logs "Unknown settings property" (`router.via_costs` did). List them with `javap -p -cp <unzipped jar> app.freerouting.settings.RouterSettings` (also `ScoringSettings`, `FanoutSettings`, `OptimizerSettings`).
- **Freerouting in parallel:** ~21 s alone, ~54 s each with 4 at once on this laptop (memory-bound); thread and GC settings don't help, and the result is the same whatever the thread count.
- **Freerouting's "N violations" on the starter board are all by design (verified 2026-09-29).** Freerouting's own reader, loading `route/try-0/board.dsn`, lists:
  - 12: the ESP32 EPAD's 12 thermal holes overlapping the pad (same net)
  - 4: the HRO USB-C footprint's stacked pad pairs (A1/B12, A12/B1, A4/B9, A9/B4; same net, same place)
  - 4: the locked SHT40 escape stubs, inside the sensor's own keep-out notch (D-010)
  The count is there before the first track is laid and never changes. Freerouting flags same-net overlapping pins and fixed wires in keep-outs; KiCad's DRC doesn't, correctly. To list the violations, run the probe described in D-018: Freerouting's `DsnReader.readBoard`, then `Item.clearanceViolations()`.
- **Freerouting takes the convex hull of every padstack polygon** (its `Library` parser), so a non-convex custom pad is a convex one to the router.
- **KiCad 10 still ships the SWIG `pcbnew` module** (`/usr/bin/python3 -c "import pcbnew"`). pcbgen doesn't use it, but `pcbnew.ExportSpecctraDSN(board, path)` is a handy reference to check pcbgen's DSN against (D-017). Gone in KiCad 11.
- **`kicad-cli pcb drc --refill-zones --save-board`** is the headless zone fill (there is no separate fill command). `gates::fill_zones` uses it. The saved `filled_polygon`s are single outlines with their holes joined in by zero-width cuts; stitching reads them.
- **Worktree sessions:** `tools/` is gitignored in the main checkout. Symlink it into a worktree (`ln -s /home/kivan/Projects/PCB/tools tools`). The symlink shows as untracked (the ignore rule ends in `/`); don't commit it.
- **Freerouting and footprint keep-outs:** Freerouting routes to pad *centres*. A footprint keep-out with only a pad-sized notch (SHT40) makes those pads unroutable; a locked escape stub fixes it (D-010). Freerouting's log "N unrouted" is the first thing to read.
- **Modified footprints live in their own library** (`lib/footprints/<Lib>.pretty`, found by the `sch` stage). With a new name in its own library, DRC's `lib_footprint_mismatch` compares the footprint with itself, so no waiver is needed (verified on the 0.3 mm ESP32 copy, 2026-09-29).
- **Editing a library footprint's silk** (to fix clearance warnings) triggers DRC `lib_footprint_mismatch`. Waive cosmetic silk items with a reason instead.
- **All KiCad `TestPoint_Pad_*` footprints** put the silk ring 0.14 mm from the pad, under JLCPCB's 0.15 mm guideline (waived; cosmetic).
- **DRC schematic parity** expects no-connect pads to carry KiCad's `unconnected-(...)` nets, and footprint `Datasheet` fields to match the symbol's.
- **`.kicad_pro` churn:** the `sch` stage writes a minimal project file; kicad-cli (DRC with `--save-board`) rewrites it in KiCad's full format with the same settings. So running `sch` alone shows a big diff in `.kicad_pro` that means nothing; commit it after a full run.

- **JLCPCB CPL rotations:** the community correction table is `matthewlai/JLCKicadTools/jlc_kicad_tools/cpl_rotations_db.csv` (the kicad-jlcpcb-tools plugin downloads it). Entries used here: `^SOT-223` +180, `^SOT-23` −90, `^USB_C_Receptacle_HRO_TYPE-C-31-M-12` +180. The ESP32-S3-WROOM-1, the SHT4x DFN, JST SH, TS-1187A switches, LEDs and SOD-123F have no entry (UNVERIFIED; check the JLCPCB preview).
- **JLCPCB CPL, bottom side (UNVERIFIED with JLCPCB):** kicad-jlcpcb-tools writes a bottom part's rotation as 180 − KiCad's angle, then adds the same package correction, and uses the position as seen from the top (`fabrication.py`, `_rotation_for_match`, main branch read 2026-09-29). pcbgen's CPL follows it and lists every bottom part as UNVERIFIED for the preview check.

- **A through-hole pad in a pour needs room for its thermal spokes (verified 2026-09-29, D-020).** On D-018's board, a routed track next to the USB-C shield pads (J1 SH) cut the GND pour's spokes to them: DRC `starved_thermal` (min 2 spokes, actual 1), in 1–2 of every 8 footprint orders. The route stage's DRC check and escalation usually find an order without it.
  - If none is clean, `RouteOptions::pad_rings` keeps tracks and vias 1 mm off those pads, which removed it in 8 of 8 orders.
  - The rings cost routability near the connector: more orders leave a signal unrouted, and routing takes about twice as long.
- **Routing failures are logged per board** in `boards/<name>/route-failures.jsonl` (D-020). An error type failing on two boards becomes a prevention rule plus an entry here.

- **Wokwi and the ESP32-S3 (verified 2026-09-29, research/2026-09-29-wokwi.md):** the S3's I2S isn't simulated and Wokwi has no I2S/PDM mic part, so no custom chip can stand in; `esp_deep_sleep_start()` ends in a watchdog reset about 1 s later, with no GPIO or timer wake. The free plan is 50 simulated minutes a month. The internet path is Wokwi's public gateway, which Wokwi monitors: never put real secrets in a simulation. `wokwi.toml` uses `firmware='build/flasher_args.json'`; the chip needs `flashSize`, `psramSize`, `psramType` and `serialInterface` attributes in `diagram.json`. The docs' `expect-pin` example is wrong: the key is `value`.
- **`kicad-cli pcb export step` exits 0 with 3D models missing;** it only prints "Could not add 3D model". Read its output (research/2026-09-29-enclosure-tooling.md). KiCad ships no model for the HRO USB-C, the LTST-C19HE1WT LED or the JST PH SMD connector.
- **ESP32-S3 GPIO0 as the wake button:** strapping pins are sampled only at a full chip reset; a deep-sleep wake doesn't sample them, so holding the button at wake doesn't enter download mode (ESP32-S3 TRM, via research/2026-09-29-esp32-firmware.md). With `CONFIG_SPIRAM_MEMTEST` on, 8 MB of PSRAM adds about 2 s to every boot.

## Mistakes to avoid
- **A gate passing on the wrong rules proves nothing.** The committed `2cca03f` board was routed with its net classes missing (every net at 0.2 mm), and nothing flagged it. Cause not found; a fresh `sch pcb route` applies them. The USB class then stayed unapplied too: its patterns (`USB_D+`) never matched KiCad's local-net names (`/USB_D+`). The `netclasses` gate now fails on any pattern that matches no net.
- **Run a DRC before routing.** Courtyard overlaps, silk collisions and parity errors show up there, and they are cheaper to fix than after a 1-minute route.
- **Don't trust part numbers typed into code;** re-check them against the research file. The starter circuit had a wrong SHT40 LCSC# (C2757403), a wrong fuse (C70069) and an LED colour that JLCPCB doesn't stock as Basic.
- **Green 0805 LEDs** (Vf up to 3.1 V) can't run from 3.3 V through a resistor. Feed them from 5 V.
