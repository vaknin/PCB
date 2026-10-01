# Writing a board

The board crate's format (every builder method, `RouteOptions` field, what is generated and
committed) is in `docs/pipeline.md` "A board directory". The worked example is
`boards/starter/src/{main,circuit,layout}.rs`: copy from it rather than from memory.

## A new board
- Scaffold it: `cargo run --release -p pcbgen --bin pcb -- new <name>` writes `boards/<name>/`
  (crate, `spec.md`, `board.toml`, `round.md`, a 50×50 mm ESP32-S3 board that already passes
  every gate). Its templates are `templates/board/`.
- Prefer the blocks over rewriting common circuits (`crates/pcbgen/src/blocks.rs`):
  `esp32s3_core` (module, decoupling, EN, reset, optional boot button), `usb_c_power` (with
  `.fused(net)` for a fuse), `ldo_3v3` (LDL1117 or HE9073). They carry the verified pinouts;
  end a circuit with `nc_unconnected` for the module pins left over.
- Then fill in `spec.md` and `board.toml` (`docs/workflow.md` step 2), then the circuit, then
  the layout. A module other than ESP32-S3-WROOM-1/-1U needs an entry in `MODULES` in
  `crates/pcbgen/src/boardfile.rs`.

## Parts
- Find parts with `cargo run --release -p pcbgen --bin pcb -- parts search "<what>"` and copy
  the LCSC code from its table instead of typing one. Flags: `--basic` (Basic only) or
  `--no-fee` (Basic + Preferred Extended), `--qty N` (price at N), `--limit N` (rows shown),
  `--pages N`, `--any-stock` (include parts out of stock), `--fresh` (skip the 1-day cache in
  `~/.cache/pcbgen/jlc/`). Fee-free parts come first. Its KiCad footprint column is a name
  match, a hint, never a checked land pattern.
- Check one part with `cargo run --release -p pcbgen --bin pcb -- parts show C123`
  (`--datasheet` downloads the PDF and a `pdftotext` copy into the cache).
- Every assembled part needs `.lcsc(..)` and `.mpn(..)` (the blocks and the template carry
  them). The `cost` stage's PARTS gate fails when a BOM line's LCSC listing has another MPN
  (a one-digit LCSC typo moves the price a cent but swaps the part) or too little stock;
  a line without an MPN only warns for now.
- A symbol or footprint KiCad doesn't ship: `cargo run --release -p pcbgen --bin pcb -- lib import C123`.
  It runs easyeda2kicad and adds both to the repo's `pcbgen` libraries (the symbol library in
  lib/symbols is made by the first import; footprints in `lib/footprints/pcbgen.pretty`) as
  `pcbgen:<MPN>`, recorded UNVERIFIED in `lib/IMPORTED.toml`. It refuses an LCSC number or a
  name already there. Then:
  - Fix what it reports. It warns when pads reach past the courtyard. EasyEDA silk often sits
    under 0.15 mm from pads, which DRC catches.
  - Pins come in as `passive`; set real types during the datasheet check.
  - Add the printed line to `EASYEDA` in `enclosure/models.py` and align that model by hand.
  - Every board using it has a red readiness item until a datasheet check
    (`review-agents.md`) sets its status to VERIFIED and fills in checked_by.
- An Extended (fee-bearing) part, or a fee-free substitute that costs function, is the owner's
  choice (`choice-page.md`). A technical fix that adds one is proposed with its price.
- Modified footprints go in `lib/footprints/<Lib>.pretty` under a new name.

## circuit.rs and layout.rs
- `c.part(ref, symbol, value, footprint).lcsc(..).mpn(..).block(..).id()`, then `c.connect`,
  `c.nc` (every unused pin), `c.pwr_flag` (nets driven from off the sheet). Bad symbols, pins or
  double connections panic at the line that made them.
- Layout coordinates: mm from the board's top-left, Y down, rotation degrees CCW. Every
  reference gets `at`, `at_rot` or `at_bottom` (bottom: the angle KiCad shows).
- KiCad names local nets `/NAME`: net-class patterns for them need a wildcard (`*USB_D*`).
- Local copper of one net (regulator cooling): `CopperZone` in `spec.zones` plus
  `RouteOptions { stitch_local: vec![(net, pitch)], .. }`. A cut in an edge (battery lead):
  `spec.notches` with `Notch { edge, at, width, depth }`.
- Warnings may be accepted only with `Waiver { kind, substring, reason }` in `layout.rs`.
  Errors can't be waived.

## Keep board.toml in step
- The BOARD.TOML gate (in `sch`, `check`, `fw`) fails on any drift: wrong net or GPIO, a used
  module GPIO missing from the map, loads over a source's budget, sleep total over `sleep_ua`,
  a requirement uncovered or missing from `spec.md`. Change a pin in `circuit.rs` → change
  `board.toml` in the same edit.
- Only ESP32-S3-WROOM-1 and -1U have a GPIO table (`MODULES` in `crates/pcbgen/src/boardfile.rs`);
  another module needs an entry there.
- Case: flat cases rarely give an M3 screw 2 × d of thread in the lid; use `screw = "M2"`.

## Placement: order of work
1. Edit `layout.rs` → `pcb` → `scripts/render.sh boards/<name>` → look at the PNG.
2. `check` (DRC before routing is cheap: courtyards, silk, parity).
3. `route --tries 1` while iterating; the full default route (8 orders) before calling a
   board done.
4. From the first layout on, `case` too, and look at its renders.

## Placement rules of thumb
- Antenna modules flush with a board edge, nothing under the antenna. USB-C flush with an edge
  (HRO TYPE-C-31-M-12: origin at `H - 3.7`).
- Decoupling caps next to the pins they serve; ESD parts between the connector and the MCU.
- Leave routing channels. A 0.4 mm power track couldn't reach a 0.8 mm-pitch DFN pin; 0.3 mm
  could.
- JLCPCB: silk text ≥ 1.0 mm, pad-to-silk 0.15 mm, track/clearance floor 0.1/0.1 mm, every
  drill ≥ 0.3 mm (smaller adds ~$50). Prices and sources: `HARDWARE_LESSONS.md` "Fab rules".
- Bottom-side parts: their CPL rotation is UNVERIFIED and bottom assembly costs extra.

## pcbgen from another crate
A standalone crate (outside the repo) works: `edition = "2024"` and
`pcbgen = { path = "/home/kivan/Projects/PCB/crates/pcbgen" }` (an absolute path; `~` doesn't
work in Cargo). Copy `boards/starter/src/main.rs`. pcbgen finds Freerouting, the fab rules
and `lib/footprints` relative to its own repo; `kicad/` and `fab/` go into the crate's
directory.
