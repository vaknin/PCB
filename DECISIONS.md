# Decisions

Newest first. Each entry: what was decided, why, and status (proposed / confirmed by owner).
Research behind these: `research/2026-09-29-landscape.md`.

## D-019 pcbgen: the route stage checks its best candidates with KiCad's DRC (DECIDED, technical, 2026-09-29)
- **Why:** the router's own numbers (unrouted, thin track, vias, length) can't see some faults. On D-018's scratch board, the order ranked best ran a track through the ground pour's spokes to the USB-C shell pads, and the full check failed with 2 `starved_thermal` errors. A different order would have passed.
- **What:** after Freerouting has routed every order, the best `RouteOptions::drc_checks` of them (default 3) are each finished the way the kept one always was: tracks and vias, zone fill, stitching, fill. Each runs in its own copy of the project (`route/try-<n>/check/`) and gets the same DRC gate as the check stage: fab rules, schematic parity, the board's waivers.
  - The kept order is the best-ranked one with no open DRC item. If none is clean, it is the one with the fewest open items, and the stage says the check will fail.
  - Every checked order's open count goes into `route/tries.json` (`drc_open`). `drc_checks: 0` keeps the old behaviour.
- **Cost:** about 10 s per run (the three checks run at once), on a ~2.5 min run.
- **Result:** on D-018's scratch board (R5 and the SHT40 on the back), order 5 was dropped (2 open), order 6 kept (0 open), and every gate passed.
  - On the unchanged starter, the kept order is the same as before (order 2), so the committed board stays as it is.
- The check stage still runs its own DRC; this only chooses better among routes that already exist. It adds no new rule.
- Stitching now returns its "no room for a via" notes instead of printing them, so only the kept board's notes are shown.

## D-018 pcbgen: bottom-side parts; Freerouting's "violations" explained (DECIDED, technical, 2026-09-29)
- **Bottom-side parts work end to end.** In a layout, `at_bottom(x, y, rot)` places a part on the back. `rot` is the angle KiCad shows for the flipped part, still CCW as seen from the top. It means: flip the footprint left-right at angle 0 (pcbnew's flip), then turn it to `rot`.
- **Board file:** `footprint::flip` rewrites the library footprint the way pcbnew saves a flipped one:
  - Y negated in the footprint's frame; F.* ↔ B.* layers
  - pad angles negated, trapezoid dy negated, chamfer corners swapped
  - KiCad's rule for text angles, justification and mirroring (rules in `HARDWARE_LESSONS.md`)
  - The part is then placed as before.
  - **Checked:** 23 footprints, 11 of them different library parts plus two synthetic test footprints covering every item type and text case, were placed by pcbgen and compared item by item with a board pcbnew saved after `Flip`. All match, except KiCad re-saving a 30°-turned rectangle as a polygon with its own point order (same corners).
  - Hidden symbol fields on a bottom part go on B.SilkS, turned 180° and mirrored. That is inferred from the flip rule, not checked against pcbnew's netlist update.
- **DSN:** a bottom part is written as KiCad's exporter writes it: the image of the top-side footprint it came from, placed `back` at its angle + 180, so top and bottom copies share an image.
  - **Checked against `pcbnew.ExportSpecctraDSN`** on the same 23 parts: placements, pin positions, rotations and shapes, and keep-outs all match.
  - Duplicate-numbered pins (`SH@1`…) get their `@n` suffixes in a different order on back parts: same shapes, internal names only.
  - **Checked in Freerouting too:** its own reader puts every pin of the bottom parts where the board file has them, on the back layer, and the SHT40's keep-out on the back.
- **Rest of the pipeline:**
  - Escape stubs go on the part's own copper side, against keep-outs on that side.
  - The routing report counts a track in a keep-out only on the keep-out's layers (a via on any layer), and names the footprint.
  - Stitching already avoided courtyards and pads on both sides.
  - **CPL:** bottom rotation = 180 − KiCad's angle + the package correction, position as seen from the top. That is kicad-jlcpcb-tools' rule (its `fabrication.py`), not JLCPCB's own documentation. Every bottom part is listed UNVERIFIED in `fab/README.md` for the placement-preview check.
  - Owner note for later: bottom-side assembly at JLCPCB costs extra (a second side). Nothing uses it yet.
- **Found on the way:** pads whose copper is offset from their hole (`(drill ... (offset x y))`) were written into the DSN centred on the hole. They are now shifted, and named `Oval[A][dx,dy]Pad_...` as KiCad names them. `Pad::dist` (stitching) uses the offset too. No starter part has one.
- **Also:** image keep-outs are rounded to 1 nm in the footprint's frame, so a rotated part's image carries no sub-nm noise.
- **Scratch test:** a copy of the starter board with R5 and the SHT40 (U4) moved to the back, full pipeline into a scratch directory.
  - Passed: ERC 0, schematic parity 0, 0 unrouted, no copper in keep-outs, 11 waived silk warnings as on the starter, CPL as expected. The render shows both parts on the back with mirrored fab text, the keep-out and escape stubs in place.
  - DRC failed on one item: 2 `starved_thermal` errors on the USB-C shield pads. The kept route ran a track through their ground-pour spokes. That is a routing outcome near J1, not a bottom-side placement fault, and the gate caught it.
  - The route stage's score didn't look at DRC; D-019 fixes that, and the same board then passes every gate.
- **Unchanged starter:** a full scratch run gave a byte-identical schematic, BOM, CPL, positions, every DSN body and the session file, and an identical routing report. The committed board is not re-routed. `fab/README.md` changes only in a heading ("…, or bottom side").
- **Freerouting's ~20 "clearance violations" (open since D-017): explained, harmless.** `scripts/fr-violations/run.sh <dsn>` loads the DSN with Freerouting's own reader (`DsnReader.readBoard`) and prints `Item.clearanceViolations()` with both items and their places. On the starter's DSN they are:
  - 12: the ESP32 EPAD's thermal holes overlapping the pad, same net
  - 4: the HRO USB-C footprint's stacked pad pairs, same net, same place
  - 4: the locked SHT40 escape stubs inside the sensor's keep-out notch (D-010)
  - They are all by design: present before routing, and never changed by it. KiCad's DRC rightly sees none. Nothing in the DSN is wrong.
  - The probe runs on Freerouting's Java 25 runtime through a copy of its launcher, because javac 21 can't read Java 25 class files; hence the reflection.
- **Tests:** 25 unit tests (new: `flip` against pcbnew's saved values, a bottom part read back through `board::Footprint`, the DSN of a top and a bottom copy, drill-offset padstacks, CPL rotations).

## D-017 pcbgen: best-of-N routing with fanout off, pad shapes, tests (DECIDED, technical, 2026-09-29)
- **Thin tracks had one cause: Freerouting's fanout stage.** Its code (read from the 2.4.1 jar with `javap`, `FoundConnectionInserter.insertFanoutMicroNeckdown`) narrows a fanout track to 3/4 or 3/5 of its class width: that is where the 0.225 mm (of 0.3) and 0.15 mm (of 0.2) tracks came from. Its `automatic_neckdown` setting changed nothing (identical session file). With fanout off, all 8 footprint orders of the starter board routed with **no** track under its class width (40–49 mm with fanout on) and 19–27 vias instead of 23–31. `RouteOptions::fanout` is now `false` by default.
- **Best of N orders, not first success.** The route stage routes 8 footprint orders (`RouteOptions::tries`), 4 at a time (`parallel`), and keeps the best: fewest unrouted connections on other nets, then on the pour net, then least track under its class width (0.1 mm steps), then fewest vias, then shortest (0.1 mm steps); ties go to the lower order. Each order works in `kicad/route/try-<n>/`; the winner's `board.dsn`/`.ses`/log are copied to `kicad/route/`, and every order's score goes to `route/tries.json` and into `reports/routing.json` as `router`. Order 0 is the board's own footprint order; the others are seeded shuffles, so a run is reproducible.
- **Cost: time.** Freerouting takes ~21 s alone on this laptop (Ryzen 7 5700U, 8 cores); 4 at once take ~54 s each (memory-bound: its thread settings and JVM GC options changed nothing, and results are identical whatever the thread count). A full starter run is now ~2.5 min (was ~35 s). `--tries 1` on the command line gives a quick single route while iterating on placement.
- **Starter result:** order 2 of 8 kept: 0 unrouted, 20 routed vias (was 30), 1.6 mm under class width (only the locked 0.2 mm escape stubs on +3V3/GND; was 45.9 mm). All gates pass. The GND pour on top is now in 3 pieces, 50% of its outline (was 2 pieces, 61%), because more track runs on top; every piece is stitched and DRC shows 0 unconnected. The design (parts, nets, placement, rules) is unchanged; BOM and CPL byte-identical.
- **Via cost flag fixed:** Freerouting 2.4.1's setting is `router.scoring.viaCosts` (field names read from its settings classes); `router.via_costs` was ignored. Same value (50) as before.
- **Freerouting reports ~20 "clearance violations" on every starter route** that KiCad's DRC doesn't see. 4 come from the locked SHT40 escape stubs; the rest are probably overlaps its own model counts (e.g. keep-outs over pads). KiCad's DRC is the gate; the count is kept in `tries.json` for comparison. (Cause found in D-018: all by design.)
- **Pads:** trapezoid pads (KiCad's `rect_delta` corners) and custom pads (convex hull of anchor and primitives, strokes included) now go into the DSN as polygons; stitching's pad distance uses the same outline. Freerouting itself takes a padstack polygon's convex hull (read from its `Library` parser), so the hull loses nothing. Unnumbered pads are named `@1`, `@2`, ... as KiCad does; the first one used to be written with an empty name, which broke the DSN. Checked against KiCad 10's own DSN export (pcbnew's `ExportSpecctraDSN`, still installed with KiCad 10; used only as a reference) on 11 library footprints: all 104 pin shapes present, KiCad's copper at most 0.4 µm outside ours, ours at most 10 µm larger. Chamfered roundrects stay rounded (covers more copper; conservative). Bottom-side parts are still refused.
- **Netlist exported once per run** (the netlist and net-class gates and the pcb stage each used to export it).
- **Tests:** 21 unit tests without KiCad (DSN naming, quoting, shapes and conventions; SES reading; stitch grid origin; log parsing and score order; glob; pad distances; convex hull) and one opt-in end-to-end test (`cargo test --release -p starter -- --ignored`, needs kicad-cli and Freerouting).
- **A board crate outside the repo works:** a throwaway crate in a scratch directory with `pcbgen = { path = "/home/kivan/Projects/PCB/crates/pcbgen" }` ran the full pipeline (see the skill).
- **Not changed:** `schematic::today()` still runs `date +%F`: the title-block date is the local date, and `date` knows the time zone without a new dependency.

## D-016 pcbgen ported to Rust, SWIG removed (the KiCad 11 port) (DECIDED, technical, 2026-09-29)
- **Why now:** KiCad 11 removes the SWIG `pcbnew` module, and Arch upgrades KiCad on a normal `pacman -Syu`. Python is in the stack only because of SWIG, and the owner's rule for long-lived tools is Rust. Doing it before Phase 2 means no second board is written against the Python API. It costs no money.
- **Target:** a Rust `pcbgen` that writes `.kicad_sch` and `.kicad_pcb` itself as S-expressions and never loads KiCad as a library. kicad-cli stays for what it does well: netlist export, ERC, DRC, zone fill (`drc --refill-zones --save-board`), gerbers, drill, positions, and `sch upgrade` / `pcb upgrade --force` so KiCad itself re-saves every file we write.
- **Toolchain:** Rust 1.98.1 (current stable, 2026-09-01), pinned in the repo's `mise.toml`. Crates, each at its latest stable version (checked 2026-09-29): `anyhow` 1.0.104, `serde` 1.0.229, `serde_json` 1.0.151 (with `preserve_order`, so `.kicad_pro` keeps KiCad's key order), `uuid` 1.26.1 (`v5`), `regex` 1.13.1, `csv` 1.4.0, `zip` 8.6.0 (9.0 is still a pre-release). No geometry, S-expression or KiCad crate: none is 1.0 and maintained, and what is needed here is small.
- **Crate layout:** a Cargo workspace at the repo root.
  - `crates/pcbgen`: the library. Modules: `sexpr` (reader/writer), `symlib` (`.kicad_sym`, flattening `extends`), `circuit`, `schematic`, `project` (`.kicad_pro`, `.kicad_dru`, lib tables), `footprint` (`.kicad_mod` loading and placement), `pcb` (the board writer), `board` (a typed read-only view of any saved `.kicad_pcb`), `geom`, `dsn`, `ses`, `route` (Freerouting and stitching), `gates`, `report`, `fab`, `cli`.
  - `boards/<name>`: one small binary crate per board: `src/circuit.rs`, `src/layout.rs`, and a `main.rs` that hands both to `pcbgen::cli`. Run with `cargo run --release -p <name> -- [sch] [pcb] [route] [check] [fab]`.
- **Board format: Rust, circuit and layout both.** A circuit needs code (helper functions for R/C, loops over test points), and layout coordinates are computed from each other (`W / 2`, `H - 3.7`) with a comment on nearly every line. TOML would lose both. Rust also type-checks the board: a misspelled field or a missing placement fails to compile or fails at the line that wrote it (`#[track_caller]` panics for authoring errors such as an unknown pin).
- **S-expressions:** a ~200-line reader/writer. Unquoted atoms keep their exact text (numbers are never re-rounded on a round trip); quoted strings are unescaped and re-escaped (`\"`, `\\`, `\n`).
- **Schematic:** a line-by-line port of `schematic.py`, with the same UUIDs (uuid5 over the same keys), the same layout arithmetic (Python's `round()` is half-to-even, so Rust uses `round_ties_even`) and the same KiCad 9 output followed by `kicad-cli sch upgrade`. Check: the upgraded `.kicad_sch` and KiCad's exported netlist must be identical to the Python ones apart from the date and file paths.
- **Footprints:** read from the project's `fp-lib-table` (`/usr/share/kicad/footprints`, `lib/footprints/*.pretty`). Placing one in a board follows KiCad's file conventions (read from the Python-made board): pad and graphic positions stay in the footprint's own frame, pad and text angles get the footprint's rotation added, and zones inside a footprint (keep-outs) are stored in board coordinates. Every UUID inside is replaced by a deterministic one. Bottom-side placement is refused with a clear error until a board needs it.
- **Board writer:** builds the `.kicad_pcb` tree (footprints with nets and schematic paths from kicad-cli's netlist, outline, pours, `CopperZone`s, keep-outs, texts, locked escape stubs, aux/grid origin), writes it, then runs `kicad-cli pcb upgrade --force` so KiCad parses and re-saves it at once. Check: DRC before routing gives parity 0 and only the expected unrouted items and dangling stubs, and the render looks the same as the Python board.
- **Routing:** the route stage deletes unrouted tracks and vias from the saved board (locked stubs stay), writes the Specctra DSN itself from the saved board (KiCad's own DSN conventions: µm, Y up, `(resolution um 10)`, `Via[0-1]_<dia>:<drill>_um` padstacks, one image per footprint with its keep-outs, NPTH holes as keep-outs of drill + 2 × hole clearance, classes from the `.kicad_pro` net-class patterns, locked tracks as `(type fix)` wires). Pours are never written, as before (D-012). Freerouting gets the same arguments as before. The SES reader adds its wires and vias to the board. Check: the DSN's classes, keep-outs, pins and padstacks match the Python DSN.
- **Stitching:** reads the filled zones from the board kicad-cli has just filled (`filled_polygon`) and applies the same rules as `route.stitch`: 3 mm grid, then one via per isolated piece, then `stitch_local`; a via must sit fully inside the fill on both layers (point-in-polygon plus distance to the polygon's edges ≥ the via radius + 0.05 mm, instead of SWIG's `Deflate`), outside courtyards (courtyard-fallback: ≥ 0.2 mm from every pad), ≥ 0.5 mm hole to hole and ≥ 0.25 mm from other-net tracks.
- **Order:** the Python stays working until the Rust pipeline reproduces the starter board and every gate passes on the same reports; only then do the docs, the skill and the lessons switch, and the Python is deleted.
- **Result (same day): done, Python removed.** Checked stage by stage against the Python pipeline on the starter board:
  - `sch`: `.kicad_sch`, `.kicad_pro`, lib tables and `.kicad_dru` byte-identical; KiCad's netlist identical apart from path and timestamp; ERC 0.
  - `pcb`: same footprints, pads, nets, zones, texts and escape stubs (only a 1 nm difference on three outline-arc midpoints). DRC before routing gave the same counts on both (parity 0, 47 unrouted, 3 dangling stubs, the 11 known silk warnings). The render matches.
  - DSN: same boundary, vias, rules, keep-outs, every pin's position, rotation and shape (roundrect pads as polygons at most 1 µm outside the copper, as KiCad's), nets, classes and locked stubs.
  - Full run in place: netlist and net classes PASS, ERC 0, DRC 0 open (the same 11 waived silk warnings; 0 unconnected, parity 0), 0 unrouted, no copper in keep-outs, 20 +3V3 cooling vias, smallest drill 0.3 mm. BOM, CPL and positions byte-identical to the Python ones.
  - The tracks differ: a new router run, which happened on every Python run too. The design (parts, nets, placement, rules) is unchanged.
- **Found on the way: Freerouting is deterministic.** The same DSN routed identically on repeated runs (both DSNs, twice each). The Python pipeline's run-to-run variation came from pcbnew's random footprint UUIDs reordering the DSN. With deterministic UUIDs every retry would repeat the same failure, so try 1 uses the file order and tries 2 and 3 shuffle the footprint order with a fixed seed: each try differs, each run is reproducible.
- **Known limits, all refused with a clear error rather than guessed:** bottom-side parts, trapezoid and custom pad shapes in the DSN. Add them when a board needs them.
- **Not changed:** Freerouting's arguments, including `--router.via_costs=50`, which Freerouting 2.4.1 ignores ("Unknown settings property"); noted in the lessons.

## D-015 Starter board: the three "before a real order" fixes from D-014 (DECIDED, technical, 2026-09-29)
The owner said "just fix for now": no order and no cost table yet. Research: `research/2026-09-29-holes-and-fuse.md`, done by a separate agent with sources tagged VERIFIED/INFERRED.
- **Regulator cooling copper:**
  - U2's tab (+3V3) now sits in a +3V3 pour on both layers, x 1–13 / y 14–26 mm. It is filled above the GND pours (priority 10) and tied together by 20 vias on a 2 mm grid.
  - New pcbgen features, reusable on any board: `CopperZone` in `BoardSpec.zones`, and `RouteOptions.stitch_local` (net → via pitch). Stitching runs per net. The router still sees no pours, so +3V3 is fully routed with tracks and the pour comes on top. Unlike GND, unrouted +3V3 connections still count.
  - `routing.json` now reports every pour by net and layer (`pours`, which replaces `gnd_pours`) and `vias_by_net`.
  - Heat estimate (INFERRED, not measured): about 12×12 mm of copper on both sides plus vias should roughly halve the reviewer's 50–70 °C rise at 0.4 W. Measure it on a real board.
- **ESP32 EPAD holes: local footprint with 0.3 mm drills** (`lib/footprints/pcbgen.pretty/ESP32-S3-WROOM-1_EPAD-Drill0.3`). It is KiCad's footprint with only the 12 drills changed (0.2 → 0.3 mm) plus a new name and description. The positions, the 0.6 mm pads and the window-pane paste all stay.
  - Why: JLCPCB's capabilities page says a 0.2 mm hole on a pad of 0.45 mm or more is free. But on its quote form, the "0.2 mm" min-via option adds about $50 (drill, via covering and Kelvin test) to a $4 board. 0.3 mm is the form's stated free size, so the risk goes away.
  - Espressif recommends vias in the EPAD gaps (datasheet Fig. 11-1, 12 vias, no size given).
  - It is a separate library, so DRC `lib_footprint_mismatch` compares it with itself and no waiver is needed.
  - The annular ring is now 0.15 mm, below JLCPCB's 0.25 mm PTH recommendation but above the via minimum, and DRC with JLCPCB's rules passes (the ring INFERRED to be fine, since these are via-like holes inside copper).
  - The board's smallest hole is now 0.3 mm.
- **Fuse: F1 is now Bourns MF-NSMF075-2 (C89653)**, a 0.75 A hold / 1.5 A trip part, 6 V, 0.40 Ω max. It replaces the JK-nSMD050-30.
  - Bourns' own derating table gives 0.61 A hold at 50 °C and 0.52 A at 60 °C. A 0.5 A PTC holds only 0.35–0.40 A there (Bourns and Littelfuse tables), which is no margin over the ~0.35 A average load.
  - It still trips about 1 s at 2 A and 0.2 s at 3 A (read off a chart, INFERRED), so it still protects against a board short.
  - No fee change: the old part was already Extended, and JLCPCB has no Basic PTC fuses at all. $0.037 each, 17,850 in stock (2026-09-29).
  - 6 V rating over USB's 5.25 V; the TVS after it clamps spikes. Fallback if a higher rating is ever wanted: BHFUSE BSMD1206-075-16V (C883128). Its derating was not checked.
- **Result:** clean full run. Netlist and net classes PASS, ERC 0, DRC 0 open (the same 11 waived silk warnings), 0 unrouted, no copper in keep-outs.
- **Still open before any order:** the 8 UNVERIFIED rotations in JLCPCB's preview (D3 matters most), and the A/B/C cost table. At upload, confirm the quote keeps "Min via hole size" at 0.3 mm with no extra lines.

## D-014 Starter board changes after the datasheet check and blind review (DECIDED, technical, 2026-09-29)
Reports: `research/2026-09-29-datasheet-check.md` (every pin VERIFIED) and `research/2026-09-29-blind-review.md`.
- **Capacitors on 3V3:** C3 at the module is now 22 µF and C2 at the regulator 1 µF (were 10 µF and 22 µF). This follows Espressif's 22 µF + 0.1 µF at the module. It also brings the total (~23 µF nominal, less under 3.3 V bias; inferred) back to the edge of ST's LDL1117 stability plot, which only covers 1–22 µF. The board had 32 µF.
- **Fuse before TVS:** D3 (SMF5.0A) now sits on +5V after F1. A faulty charger trips the fuse instead of burning the TVS.
- **C1 moved** under U2, next to its input pin (it was ~7 mm away, reached through vias).
- **USB net class never applied:** its patterns `USB_D+` didn't match KiCad's `/USB_D+`. Now `*USB_D*`. A new `netclasses` gate (in `sch` and `check`) fails when any pattern matches no net, and was checked against the old patterns.
- **Stitching vias** now check their distance to other-net tracks directly (≥ 0.25 mm). One via ended up 0.185 mm from a track even though it was inside the pour; cause not found.
- **Deferred to "before a real order"** (all three done in D-015):
  - Extra 3V3 copper under the regulator tab for cooling. The reviewer estimates a 50–70 °C rise at 0.4 W (inferred).
  - The fuse's hot-derating margin (no derating curve in its datasheet).
  - The 12 × 0.2 mm thermal-via holes in KiCad's ESP32-S3-WROOM-1 footprint. JLCPCB allows them but charges extra for holes under 0.3 mm. Check the quote.
- **Before paying:** check the 8 UNVERIFIED rotations in JLCPCB's preview. D3 matters most: a reversed TVS shorts +5V.

## D-013 Stack survey confirms the stack (DECIDED, 2026-09-29)
- Session pcb-99 surveyed the alternatives (`research/2026-09-29-stack-survey.md`). Nothing is both better and within the owner's rules (1.0+, local CLI, no lock-in), so D-004 stands.
- **Revisit:** Diode `pcb`/Zener at its 1.0 (placement-preserving netlist sync).
- **Optional second opinions, never gates:** `pcb dfm` and kicad-happy's analysers.
- **Optional paid fallback router for dense boards:** DeepPCB's API, with the owner's OK.
- **D-005 note:** SKiDL's UUIDs are now deterministic, but the drawing-quality reason still holds.

## D-012 Routing and fab details learned on the starter board (DECIDED, technical, 2026-09-29)
- **The router never sees the copper pours.** When the DSN includes the GND pours, Freerouting counts every GND pad as connected and routes none of them. Tracks then cut the top pour into pieces, and pads were stranded (DRC `unconnected_items`, the 5 errors in `2cca03f`). The export step now removes the pours from the DSN, so every GND pad gets a real track. The pours and stitching vias come on top of that.
- **Power net class: 0.3 mm (was 0.4).** The committed `2cca03f` board was routed with net classes silently missing: every net was at 0.2 mm. With the classes applied, a 0.4 mm track could not reach the SHT40's 3V3 escape stub in 3 tries. At 0.3 mm it routes. 0.3 mm on 1 oz outer copper carries about 1 A at a 10 °C rise (IPC-2221), twice the ~0.5 A peak.
- **Freerouting retries:** up to 3 tries while any net other than the pour net is left unrouted. The ones seen so far failed the same way every try, so a retry mostly helps when luck is involved; the DRC gate decides.
- **Stitching fallback:** a pour piece with no free spot outside courtyards may get its via inside a courtyard, if it clears every pad by 0.2 mm (a tented via under a part body). A via in a pad would wick solder.
- **CPL rotations:** corrections come from the community table used by kicad-jlcpcb-tools (JLCKicadTools `cpl_rotations_db.csv`). Only unpolarised R/C/fuse parts are assumed to need no correction. Every other part without a table entry is listed as UNVERIFIED in `fab/README.md`, to check in JLCPCB's placement preview before paying.
- **Gates now:** netlist round trip (after `sch` and in `check`), ERC, DRC, and a routing report (`reports/routing.json`). The routing report fails on any unrouted connection or any track/via in a keep-out.

## D-011 SWIG segfaults: root cause fixed; SWIG steps isolated (DECIDED, owner approved the scope, 2026-09-29)
- **Root cause, found with glibc's heap checker** (`LD_PRELOAD=/usr/lib/libc_malloc_debug.so GLIBC_TUNABLES=glibc.malloc.check=3 PYTHONMALLOC=malloc`):
  1. pcbnew's Python `Remove()` sets `thisown=1`, so Python frees each removed track while KiCad still points at it (connectivity, DSN export). `route.py` removed old tracks at the start of every run, so every later step read freed memory. Removed tracks are now leaked on purpose (the process is short-lived), and connectivity is rebuilt.
  2. `zone.Outline()`, `fp.GetCourtyard()` and polygon `COutline(i)` come back with `thisown=True`, so Python would delete KiCad's own polygons. `pcbgen.kicad.borrowed()` marks them as KiCad's.
- **Isolation (the owner approved steps 3 and 4 of pcb-99's plan):**
  - Every stage runs in its own process when several stages are run together.
  - Inside `route`, each SWIG step (DSN export, SES import, stitching) runs in its own process (`kicad.run_step`). A crash reads "step X crashed (SIGSEGV)" and can't corrupt later steps.
  - Zones are filled by kicad-cli (`pcb drc --refill-zones --save-board`), not by SWIG's `ZONE_FILLER`.
- **Deferred to the KiCad 11 port:** writing the `.kicad_pcb` directly as S-expressions (step 2). With the root cause fixed, it is a large rewrite with no bug behind it, and the owner wants this test project kept lean.
- **Result:** the full pipeline runs clean under the heap checker.

## D-010 Layout and routing automation in pcbgen (DECIDED, technical, 2026-09-29)
Everything below lives in the generator, so every future board gets it.
- **References go to the fab layer.** Owner-facing silk labels (5V, 3V3, GND, TX, RX, SDA, SCL, PWR, LED, RESET, BOOT, Qwiic) are `Text` entries in `layout.py`. The title goes on the back silk. JLCPCB's minimum text height is 1.0 mm.
- **GND pours:** SMD pads connect solid (the fab reflows them) and through-hole pads get thermal reliefs (`ZONE_CONNECTION_THT_THERMAL`). With thermals everywhere, small pads (USB-C GND, SHT40) failed DRC's two-spoke minimum.
- **Escape stubs:** some footprints carry their own copper keep-out. The SHT40 DFN does ("no copper under the sensor"), leaving only a pad-sized notch. Freerouting always aims for pad centres and gives up on those pads.
  - `pcb._escape_stubs` adds a locked 0.2 mm track from each connected pad centre, along the pad's long axis, 0.5 mm past the pad end. The router connects to the free end.
  - `route.py` keeps locked tracks when it clears old routing.
- **GND stitching** after routing (`route.stitch`), because Freerouting doesn't stitch and a top pour cut by tracks leaves disconnected pieces:
  - A 3 mm grid of 0.6/0.3 vias, placed only where a via fits fully inside the filled pour on both layers, outside every courtyard and ≥0.5 mm hole-to-hole.
  - Then one via per remaining pour piece, found by a fine search. (It crashed Python until the SWIG ownership bugs were fixed: D-011.)
- **Net classes (starter):**
  - Default 0.2 mm track / 0.15 mm clearance, so signals escape the SHT40's 0.3 mm pads.
  - Power 0.3/0.2 (was 0.4, see D-012): about 1 A at a 10 °C rise per IPC-2221, against a ~0.5 A peak.
  - USB 0.3/0.15.
  - JLCPCB's floor is 0.1/0.1.
- **Waivers:** an accepted warning needs a `(type, substring, reason)` entry in the board's `WAIVERS`. The `check` stage now passes them to the gates. The starter waives only cosmetic silk items:
  - KiCad's test-point silk ring sits 0.14 mm from the pad (JLCPCB guideline: 0.15).
  - The module and USB-C outline silk run to the board edge, where both parts are flush by design.
  - Library footprints are *not* modified, so the lib-footprint parity check stays meaningful.
- **`scripts/render.sh boards/<name>`** renders the top side to PNG. Look at the render after every placement change.

## D-009 Firmware: C on ESP-IDF; the ESP32-S3 stays (DECIDED, 2026-09-29)
- Agreed with session pcb-99. The owner leans toward C on ESP-IDF.
- **ESP-IDF v6.1 is stable** (2026-08-27).
- **Rust is not ready here.** Rust's Wi-Fi driver `esp-radio` is only 1.0.0-beta.1 (2026-09-16). The `esp-idf-*` Rust crates have been community-maintained since Feb 2025. The S3's Xtensa core also needs Espressif's forked compiler.
- Closes the open question "Firmware language".

## D-008 USB ESD: H5VUT2U + SMF5.0A instead of USBLC6-2SC6 (DECIDED, technical, 2026-09-29)
- **The parts:**
  - H5VUT2U (C20615824): 0.6 pF clamp on D+/D−.
  - SMF5.0A (C19077497): a 200 W TVS on VBUS.
  - Both are JLCPCB **Preferred Extended**, so no loading fee. USBLC6 (C7519) would add $3.07 per order.
- **KiCad has no symbol for either part.** U3 uses `Device:D_TVS_Dual_AAC` (pin 1 → D+, pin 2 → D−, pin 3 → GND). D3 uses `Diode:SMF5V0A` (pin 1 = cathode → VBUS) on `Diode_SMD:D_SOD-123F`.
- **Pin mapping checked against the datasheet:** H5VUT2U pins 1/2 are the I/O lines and pin 3 is the common pin, from the datasheet's function diagram (image). The claim that pin 3 is GND is inferred from the "C(I/O–GND)" spec plus the diagram. SMF5V0A: pin 1 is the cathode, read from the symbol graphic, which matches KiCad's diode pad 1 = K.
- **Still to do:** the full datasheet verification by a separate agent (next steps).

## D-007 LDO: ST LDL1117S33R instead of AMS1117-3.3 (DECIDED, technical, 2026-09-29)
- **Dropout:** AMS1117 drops 1.1–1.3 V. USB can sag to about 4.4 V at the connector, less the fuse drop, which leaves the 3.3 V rail below the ESP32's 3.0 V minimum. LDL1117 drops 0.35 V typ / 0.6 V max at 1.2 A.
- **Capacitors:** AMS1117's datasheet asks for a 22 µF tantalum. LDL1117 is made for ceramic caps.
- **Cost:** +$3.07 per order (Extended). It fits the same SOT-223 footprint as AMS1117.
- **Symbol:** `Regulator_Linear:LD1117S33TR_SOT223`, which has the same pinout (1 GND, 2 OUT/tab, 3 IN). KiCad has no LDL1117 symbol.
- **UNVERIFIED:** LDL1117's maximum stable output capacitance (the datasheet figure is an image). The board has 22 µF + 10 µF + 100 nF on 3V3.

## D-006 Freerouting runs from its own Linux bundle, not Docker (DECIDED, technical, 2026-09-29)
- `freerouting-2.4.1-linux-x64.zip` ships its own Java 25.0.4 runtime. `scripts/fetch-tools.sh` fetches it into `tools/` (gitignored) and checks its sha256.
- Docker would need sudo: the daemon is stopped and the user isn't in the `docker` group. mise Java 25 would add a toolchain. Neither is needed now.
- Analytics are off (`--usage_and_diagnostic_data.disable_analytics=true`).

## D-005 Circuit code emits KiCad schematics directly; SKiDL rejected (DECIDED, technical, 2026-09-29)
- **Spike:** SKiDL 2.3.0 has a KiCad 10 generator, and its output is ERC-checkable. But the drawing is poor (the ESP32 symbol is rotated, labels overlap pin names, parts are crammed together), and part UUIDs are random unless every part is tagged by hand.
- **Our own `pcbgen` package** (about 900 lines, no dependencies beyond KiCad's own):
  - It reads KiCad's library symbols and flattens `extends`.
  - Parts are placed in titled blocks. Each pin gets a wire stub plus a net label; power nets use power symbols; unused pins get no-connect flags.
  - UUIDs are deterministic (uuid5).
  - It writes KiCad 9 format, then runs `kicad-cli sch upgrade`, so KiCad itself writes the final file.
  - It also writes project-local sym/fp lib tables and a `.kicad_pro` (net classes and rules).
- **Pre-check:** every pin must be on a net or marked nc before any file is written.
- **Result:** the starter schematic passes KiCad ERC with 0 violations (no waivers).
- **The PCB comes from `kicad-cli sch export netlist`,** so it is built from exactly what ERC checked, and each footprint carries its symbol's UUID for the schematic-parity DRC.
- **Fab DRC rules:** Cimos/KiCad-CustomDesignRules moved to **Cimos/kicad-druid** (MIT). `rules/JLCPCB-2L-1oz.kicad_dru` is from v1.2.0.

## D-004 Pipeline = KiCad-native; full bake-off replaced by a routing check (CONFIRMED 2026-09-29)
- **Owner's direction:** "zero preferences, only care about quality and ease; assume Opus 5.5 can handle any tool." So tools are judged on output quality, how well the checks catch mistakes, and durability. How easy they are for an AI to use doesn't count.
- **atopile is out, whatever the skill level.** Its problems are platform risk, not difficulty:
  - its local tool is being retired
  - part picking goes through a closed, login-only backend that already broke once (July 2026)
  - it is locked to KiCad 9
- **tscircuit is not the pipeline.** What it adds is autorouting and convenience, which only saves effort. It costs quality:
  - its router can report success on a board that still has shorts
  - it has no real ERC
  - its KiCad export has pad-merging bugs
  - it is pre-1.0, with several releases a day
- **KiCad-native wins on quality:**
  - KiCad is the industry-standard format.
  - KiCad's own ERC and DRC check the real design files, using the fab's rules.
  - Its fab outputs are accepted everywhere (JLCPCB, NextPCB, PCBWay).
  - The owner, or any EE, can open the design in a free GUI.
  - The pieces are maintained and don't depend on any vendor.
- **How it works:**
  - The circuit is written as Python code, and the generator emits a real KiCad schematic, so ERC runs on it.
  - The choice between SKiDL and emitting the KiCad files directly is made in a Phase 1 spike. The requirement is a real `.kicad_sch` that KiCad's ERC can check.
  - Parts are placed by code (explicit coordinates and constraints).
  - Freerouting does the routing, run headless in Docker. Quilter's free tier and DeepPCB are the fallback routers.
- **Tooling:** plain project scripts plus a project skill, called through Bash. An MCP server adds nothing for a single-agent CLI workflow; build one only if a real need appears.
- **The bake-off shrinks to what is actually open:**
  - whether Freerouting's result on the starter board is good enough
  - whether the whole pipeline runs from code to checked fab files without the owner
- This supersedes D-003.

## D-003 Bake-off lineup: drop atopile, add a KiCad-native lane (SUPERSEDED by D-004)
- **atopile is out.**
  - Its local CLI (0.15.9) is soft-deprecated, needs an atopile account for part picking, and only works with KiCad 9. KiCad 10 files break it (#1822).
  - Its public repo has been frozen since March. The supported 0.16 runs only in a browser with its own agent, which bypasses Claude Code, git and our gates.
  - Building a "repeatable pipeline" on a tool being retired is a bad bet.
- **Lane T: tscircuit end to end.**
  - The toolchain: pinned versions, Bun, the official skill, and its own autorouter and checks.
  - Its output is then re-checked independently with KiCad 10 DRC using JLCPCB rules.
- **Lane K: KiCad-native.**
  - The circuit is written in Python (SKiDL). Parts come from the JLCPCB catalog, and their footprints are pulled from LCSC.
  - Parts are placed by a script, then Freerouting (Docker) routes the board.
  - KiCad 10 runs ERC and DRC, and kicad-cli produces the fab outputs.
- **Cross-check:** the Lane T board is also routed with Freerouting, to compare the two routers on an identical netlist.
- **Test board = draft starter board.** Using the real candidate means the winner's output feeds phase 4. Nothing is ordered during the bake-off.

## D-002 Starter board size >= 50x50 mm (DECIDED, technical)
NextPCB's Rev 0 free-assembly offer rejects boards smaller than 50×50 mm. A starter
board that size or larger keeps that option open, and it costs essentially nothing extra
at JLCPCB (the 5-board price covers up to 100×100).

## D-001 Laptop toolchain (DONE 2026-09-29: KiCad 10.0.6 + kicad-library installed, user in `uucp`)
- **KiCad 10.0.6 from Arch `extra`** (`kicad`, `kicad-library`). The 3D library
  (`kicad-library-3d`, 3.2 GB) is deferred to the enclosure phase.
  KiCad 10 over 9: it is current, kicad-cli has JSON DRC/ERC, and Freerouting and the MCP server target it. The one tool that needed 9 (atopile) is dropped (D-003).
- **Freerouting 2.4.1 in Docker.** It needs Java 25; the laptop has Java 21 for Android work, and Docker avoids a second global JDK.
- **Bun and Python pinned per project** (`mise.toml`, `uv`), nothing global, as with other projects.
  A Python venv that must import KiCad's `pcbnew` has to be created from system Python 3.14 with system site-packages.
- **Serial access:** add the user to the `uucp` group so the ESP32-S3's `/dev/ttyACM*` can be flashed without root.
- **Deferred:**
  - OpenSCAD: Arch ships the 2021.01 release, which is too old. Pick the tool in phase 8.
  - Firmware toolchain: phase 3, after the language choice.

## Open questions
- (Firmware language: closed by D-009.)
- **Before a real order (skipped while this is a tooling test, agreed with pcb-99):**
  - a 4-layer routing variant for comparison
  - vendoring the verified footprints into the repo (`lib/footprints/` exists since D-015; only the modified ESP32 footprint is there)
  - a full JLCPCB parts/cost script
  - (project skill: done as the global draft `~/.claude/skills/pcb-pipeline/SKILL.md`, at the owner's request)
  - check every UNVERIFIED rotation in JLCPCB's placement preview (list in `boards/<name>/fab/README.md`)
  - the fab's own manufacturability check on upload; current fab promotions (NextPCB Rev 0 vs JLCPCB); the A/B/C cost table from `docs/brief.md`
- (KiCad 11 port: closed by D-016. When KiCad 11 arrives, check that `sch upgrade` / `pcb upgrade` still accept the format versions pcbgen writes, then run the starter board end to end.)
- (Routing quality between tries: closed by D-017.)
- **Unexplained, harmless for now:** why the `2cca03f` board was routed without its net classes (a fresh `sch pcb route` applies them), and why one stitching via landed 0.185 mm from a track while inside the pour (now checked directly; the Rust stitcher keeps the same direct check).
