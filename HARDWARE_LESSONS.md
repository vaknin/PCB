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

**Still to verify against datasheets (next session, separate agent):** ESP32-S3-WROOM-1 pads, SHT4x pins, LDL1117 pins and max C_out, USB-C pins, Qwiic pin order (UNVERIFIED: 1 GND, 2 3V3, 3 SDA, 4 SCL).

(Pinouts, voltage ranges and footprints get added here as each is checked against its datasheet.)

## Fab rules and prices
- **JLCPCB Economic PCBA:**
  - setup $8.18, stencil $1.53, $0.0016 per joint
  - $3.07 per unique Extended part; Basic and Preferred-Extended parts are free
  - (2026-09-09 help page)
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
- **Laptop (verified 2026-09-29):** `kicad-cli` 10.0.6. `import pcbnew` works from system Python 3.14
  (`/usr/lib/python3.14/site-packages/pcbnew.py`). Build venvs with `uv venv --python /usr/bin/python3 --system-site-packages`.
  Libraries are in `/usr/share/kicad/{symbols,footprints}`; no 3D models yet.
- **tscircuit autorouter:**
  - It can report success and still leave shorts (overlapping vias, a via on a pad). Always run `tsci check shorts` and an independent KiCad DRC.
  - It does not enforce USB differential pairs.
  - Pin versions: it releases several times a day.
- **tscircuit → KiCad export:**
  - The exported schematic fails KiCad ERC.
  - Pad-number collisions can merge pads.
  - Board cutouts are dropped from gerbers.
- **kicad-cli (v10):** it cannot export Specctra DSN or import SES. Use the SWIG `pcbnew` module (deprecated; removed in KiCad 11).
- **KiCad 10 IPC API:** it needs the GUI running. Headless automation means SWIG or editing the file directly.
- **Freerouting 2.4.x:**
  - It needs Java 25.
  - KiCad's DSN export omits board-edge clearance, so pass `--router.copperToEdgeClearanceUm=500`.
- **atopile 0.15.x:** it cannot read KiCad 10-saved boards.
- **SWIG `pcbnew` on Python 3.14:** every `for x in board.Tracks()` / `fp.GraphicalItems()` raises `'SwigPyIterator' object has no attribute 'next'`. `pcbgen/kicad.py` aliases `next = __next__`; always `from .kicad import pcbnew`.
- **SWIG `pcbnew` has no `RotatePoint`.** Rotate by hand: board = (x·cos + y·sin, −x·sin + y·cos), with the angle CCW and Y pointing down.
- **Segfault risk (inferred, 2026-09-29):** holding `zone.GetFilledPolysList(layer).Outline(i)` (a reference into KiCad's fill buffer) while adding vias crashed Python 5 times. The crash shows up later, inside an unrelated `import`. Work on `CloneDropTriangulation()` copies and `SHAPE_LINE_CHAIN(poly.COutline(i))`. Run crash-prone scripts with `python -X faulthandler -u`, because a segfault loses buffered stdout.
- **SWIG "memory leak of type PCB_TRACK/PCB_VIA" lines** at exit are harmless noise. Filter them with `grep -v "swig/python detected"`.
- **KiCad 11 removes the SWIG `pcbnew` module** (DSN export, SES import, zone fill, stitching all use it). Arch upgrades KiCad on a normal `pacman -Syu`. When KiCad 11 lands, either hold the package or port `pcb.py`/`route.py` to the IPC API (which needs the GUI) or direct file editing. Timing is not yet confirmed.
- **Freerouting and footprint keep-outs:** Freerouting routes to pad *centres*. A footprint keep-out with only a pad-sized notch (SHT40) makes those pads unroutable; a locked escape stub fixes it (D-010). Freerouting's log "N unrouted" is the first thing to read.
- **Editing a library footprint's silk** (to fix clearance warnings) triggers DRC `lib_footprint_mismatch`. Waive cosmetic silk items with a reason instead.
- **All KiCad `TestPoint_Pad_*` footprints** put the silk ring 0.14 mm from the pad, under JLCPCB's 0.15 mm guideline (waived; cosmetic).
- **DRC schematic parity** expects no-connect pads to carry KiCad's `unconnected-(...)` nets, and footprint `Datasheet` fields to match the symbol's.

## Mistakes to avoid
- **Run a DRC before routing.** Courtyard overlaps, silk collisions and parity errors show up there, and they are cheaper to fix than after a 1-minute route.
- **Don't trust part numbers typed into code;** re-check them against the research file. The starter circuit had a wrong SHT40 LCSC# (C2757403), a wrong fuse (C70069) and an LED colour that JLCPCB doesn't stock as Basic.
- **Green 0805 LEDs** (Vf up to 3.1 V) can't run from 3.3 V through a resistor. Feed them from 5 V.
