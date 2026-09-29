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
With no stage named, all stages run in order. Add `-X faulthandler -u` when debugging crashes. Filter the output with `grep -v "assert\|swig/python detected"`.

| Stage | Does | Output |
|---|---|---|
| sch | writes `.kicad_pro` + `.kicad_dru` (fab rules), then the schematic; refuses if any pin is neither connected nor marked nc | `kicad/<name>.kicad_sch` |
| pcb | exports the netlist from the schematic, loads footprints, places them from `layout.SPEC`, adds outline, GND pours, labels, escape stubs | `kicad/<name>.kicad_pcb` |
| route | DSN → Freerouting → SES import, zone fill, GND stitching vias | same file; work files in `kicad/route/` |
| check | ERC + DRC (JLCPCB rules, schematic parity) from the JSON reports; fails on any error or unwaived warning | `kicad/reports/{erc,drc}.json` |
| fab | **not written yet** (gerbers, drill, pos, JLCPCB BOM/CPL) | `fab/` |

`scripts/render.sh boards/<name> [out.png] [layers]` renders the top side to look at.

## A board directory
- `circuit.py`: `build() -> Circuit`. It declares the parts (KiCad symbol, footprint, LCSC#, MPN) and connects them:
  - `net += part["PIN"]` connects a pin (by number or name).
  - `part.nc(...)` marks pins unused.
  - `c.pwr_flag(net)` marks a net as driven from off the sheet.
- `layout.py`:
  - `RULES`: net classes.
  - `SPEC`: board size, the `Place(x, y, rot)` for every reference in mm from the top-left with Y down, and silk `Text` labels.
  - `ROUTE`: Freerouting and stitching options.
  - `WAIVERS`: `(type, substring, reason)` entries.

## Status (2026-09-29, starter board)
- **ERC:** 0 violations.
- **Routing:** 100% routed.
- **DRC:** open only on a few disconnected top-GND-pour pieces. The per-piece stitching fix in `route.py` is written but untested, because an earlier version segfaulted.
- **Committed board:** the last good run, with grid stitching only.
