# The pcbgen pipeline

Circuit as Python code → KiCad schematic → ERC → PCB placed by code → Freerouting → DRC → fab files.
Why this shape: `DECISIONS.md` D-004, D-005 and D-010.

## Setup (once)
- **KiCad 10** from Arch `extra` (D-001).
- **Python venv:** `uv venv --python /usr/bin/python3 --system-site-packages .venv`. System site-packages are needed for `pcbnew`.
- **Freerouting:** `scripts/fetch-tools.sh` downloads its bundle (with its own Java 25) into `tools/` and checks it.

## Run
```
.venv/bin/python -m pcbgen boards/<name> [sch] [pcb] [route] [check] [fab]
```
With no stage named, all stages run in order, each in its own process (a crash in KiCad's SWIG code stops only that stage, by name). Add `-X faulthandler -u` when debugging crashes; for heap corruption see HARDWARE_LESSONS ("SWIG ownership bugs"). Filter the output with `grep -v "assert\|swig/python detected"`.

| Stage | Does | Output |
|---|---|---|
| sch | writes `.kicad_pro` + `.kicad_dru` (fab rules), then the schematic; refuses if any pin is neither connected nor marked nc; then the netlist round-trip and net-class gates | `kicad/<name>.kicad_sch` |
| pcb | exports the netlist from the schematic, loads footprints, places them from `layout.SPEC`, adds outline, GND pours, local copper zones (`CopperZone`), labels, escape stubs | `kicad/<name>.kicad_pcb` |
| route | DSN export (pours hidden from the router) → Freerouting (up to 3 tries) → SES import → kicad-cli zone fill → stitching vias (GND, plus `stitch_local` nets) → fill; each SWIG step in its own process | same file; work files in `kicad/route/` |
| check | netlist round trip, net-class patterns, ERC, DRC (JLCPCB rules, schematic parity), routing report; fails on any error, unwaived warning, unrouted connection or copper in a keep-out | `kicad/reports/{erc,drc,routing}.json` |
| fab | gerbers + drill (zip), JLCPCB BOM and CPL with rotation corrections; lists parts whose rotation is UNVERIFIED | `fab/` (see `fab/README.md`) |

`scripts/render.sh boards/<name> [out.png] [layers]` renders the top side to look at.

## A board directory
- `circuit.py`: `build() -> Circuit`. It declares the parts (KiCad symbol, footprint, LCSC#, MPN) and connects them:
  - `net += part["PIN"]` connects a pin (by number or name).
  - `part.nc(...)` marks pins unused.
  - `c.pwr_flag(net)` marks a net as driven from off the sheet.
- `layout.py`:
  - `RULES`: net classes.
  - `SPEC`: board size, the `Place(x, y, rot)` for every reference in mm from the top-left with Y down, silk `Text` labels, and `CopperZone`s (local pours of one net, e.g. regulator cooling copper; filled above GND).
  - `ROUTE`: Freerouting and stitching options.
  - `WAIVERS`: `(type, substring, reason)` entries.
- Modified footprints go in `lib/footprints/<Lib>.pretty` (repo root); the `sch` stage points the project's fp-lib-table there for any library of that name. Currently `pcbgen:ESP32-S3-WROOM-1_EPAD-Drill0.3` (D-015).

## Status (2026-09-29, starter board)
- **All gates pass on a clean end-to-end run:** netlist round trip, ERC 0, DRC 0 open (11 waived cosmetic silk warnings), routing 0 unrouted and no copper in keep-outs.
- **Routing (after D-015):** 20 signal vias, 20 +3V3 cooling vias, 54 GND vias, ~670 mm of track. Bottom GND pour is one piece covering 73% of the board. Smallest hole 0.3 mm.
- **Fab files:** made. 8 parts have UNVERIFIED rotations (listed in `fab/README.md`).
- **Datasheet check and blind review done** (D-014). Before any order: check rotations in JLCPCB's preview, the fab's own manufacturability check, and the owner's OK on cost.
- **Phase 1 is closed.** The starter board is a tooling test; it will not be ordered unless the owner asks.
- **Reviews:** `research/2026-09-29-datasheet-check.md` (every pin VERIFIED) and `research/2026-09-29-blind-review.md` (no wiring errors; cheap fixes applied in D-014). Reviewer's rough cost for 5 assembled boards: ~$105 delivered to Israel, ~$125 if the parts go over the $75 VAT line. Option A only: the module and DFN sensor can't be hand-soldered.

## Next session
1. Read `HARDWARE_LESSONS.md`, `DECISIONS.md` (open questions at the end) and this file.
2. **Phase 2:** write the first real project's spec with the owner (`docs/brief.md`, Phases). Then prototype on a dev board (Phase 3) before any custom PCB.
3. Deferred items before any real order are listed under "Open questions" in `DECISIONS.md`.
