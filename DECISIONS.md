# Decisions

Newest first. Each entry: what was decided, why, and status (proposed / confirmed by owner).
Research behind these: `research/2026-09-29-landscape.md`.

## D-010 Layout and routing automation in pcbgen (DECIDED, technical, 2026-09-29)
Everything below lives in the generator, so every future board gets it.
- **References go to the fab layer.** Owner-facing silk labels (5V, 3V3, GND, TX, RX, SDA, SCL, PWR, LED, RESET, BOOT, Qwiic) are `Text` entries in `layout.py`. The title goes on the back silk. JLCPCB's minimum text height is 1.0 mm.
- **GND pours:** SMD pads connect solid (the fab reflows them) and through-hole pads get thermal reliefs (`ZONE_CONNECTION_THT_THERMAL`). With thermals everywhere, small pads (USB-C GND, SHT40) failed DRC's two-spoke minimum.
- **Escape stubs:** some footprints carry their own copper keep-out. The SHT40 DFN does ("no copper under the sensor"), leaving only a pad-sized notch. Freerouting always aims for pad centres and gives up on those pads.
  - `pcb._escape_stubs` adds a locked 0.2 mm track from each connected pad centre, along the pad's long axis, 0.5 mm past the pad end. The router connects to the free end.
  - `route.py` keeps locked tracks when it clears old routing.
- **GND stitching** after routing (`route.stitch`), because Freerouting doesn't stitch and a top pour cut by tracks leaves disconnected pieces:
  - A 3 mm grid of 0.6/0.3 vias, placed only where a via fits fully inside the filled pour on both layers, outside every courtyard and ≥0.5 mm hole-to-hole.
  - Then one via per remaining pour piece, found by a fine search. **This step crashed Python (segfault, 5 times). A fix that uses polygon copies is written but UNTESTED** (see HARDWARE_LESSONS, "Tool gotchas").
- **Net classes (starter):**
  - Default 0.2 mm track / 0.15 mm clearance, so signals escape the SHT40's 0.3 mm pads.
  - Power 0.4/0.2: > 1 A at a 10 °C rise per IPC-2221, against a ~0.5 A peak.
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
  - vendoring the verified footprints into the repo
  - a full JLCPCB parts/cost script
  - a project skill for the pipeline
