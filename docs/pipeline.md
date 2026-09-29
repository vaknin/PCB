# The pcbgen pipeline

Circuit as Rust code → KiCad schematic → ERC → PCB placed by code → Freerouting → DRC → fab files.
Why this shape: `DECISIONS.md` D-004, D-005, D-010 and D-016 (the Rust port, no SWIG).

## Setup (once)
- **KiCad 10** from Arch `extra` (D-001). Only `kicad-cli` is used; no KiCad library is loaded.
- **Rust:** pinned in `mise.toml` (1.98.1); `cargo` comes from mise.
- **Freerouting:** `scripts/fetch-tools.sh` downloads its bundle (with its own Java 25) into `tools/` and checks it.

## Run
```
cargo run --release -p <board> -- [sch] [pcb] [route] [check] [fab] [--out DIR] [--tries N]
```
With no stage named, all stages run in order. `--out DIR` writes `DIR/kicad` and `DIR/fab` instead of the board's own directories (for comparison runs). `--tries N` routes N footprint orders instead of the layout's number (`--tries 1` for a quick look while placing). A full run of the starter board takes about 2.5 min, nearly all of it Freerouting (8 orders, 4 at a time, ~55 s each); checking the best 3 adds ~10 s.

| Stage | Does | Output |
|---|---|---|
| sch | writes `.kicad_pro` + `.kicad_dru` (fab rules), then the schematic; refuses if any pin is neither connected nor marked nc; then the netlist round-trip and net-class gates | `kicad/<name>.kicad_sch` |
| pcb | exports the netlist from the schematic, loads footprints, places them from the layout, adds outline, GND pours, local copper zones (`CopperZone`), labels, escape stubs; `kicad-cli pcb upgrade --force` re-saves it | `kicad/<name>.kicad_pcb` |
| route | deletes old unlocked tracks → DSN written by pcbgen (pours hidden from the router) → Freerouting on 8 footprint orders, 4 at a time, fanout off; ranked by fewest unrouted, then least track under class width, fewest vias, shortest → the best 3 are each finished (SES tracks, kicad-cli zone fill, stitching vias for GND and `stitch_local` nets, fill) and DRC-checked in `route/try-<n>/check/`; the best-ranked one with no open DRC item is kept (D-019). If none is clean it escalates, cheapest first: checks the other routed orders, then routes `extra_rounds` × `extra_tries` new orders (default 1 × 8) and checks them. If still none is clean it keeps the one with the fewest open items and writes the failure report (D-020) | same file; the winner's work files in `kicad/route/`, each order's in `route/try-<n>/`, scores and steps in `route/tries.json`; on failure `route/failure.json` and a line per failed order in `<board>/route-failures.jsonl` |
| check | netlist round trip, net-class patterns, ERC, DRC (JLCPCB rules, schematic parity), routing report; fails on any error, unwaived warning, unrouted connection or copper in a keep-out | `kicad/reports/{erc,drc,routing}.json` |
| fab | gerbers + drill (zip), JLCPCB BOM and CPL with rotation corrections (bottom side: 180 − angle, as kicad-jlcpcb-tools); lists parts whose rotation is UNVERIFIED, and every bottom-side part | `fab/` (see `fab/README.md`) |

`scripts/render.sh boards/<name> [out.png] [layers]` renders the top side to look at (`B.Cu,B.Fab,B.Courtyard,B.SilkS,Edge.Cuts` for the back).
`scripts/fr-violations/run.sh <board.dsn>` lists the clearance violations Freerouting counts, with the items involved (D-018).

## When routing fails (D-020)
- **Tries more on its own.** If none of the checked orders passes DRC, the route stage checks the other routed orders (~15 s each). Then it routes one more round of 8 seeded orders (~2 min) and checks those. It is deterministic: the same board gives the same result.
- **`route/failure.json`** (also printed) lists every open DRC item across the checked orders, with:
  - its type, severity, the parts involved, and their position in layout mm (from the top-left, as `layout.rs` places parts)
  - how many checked orders hit it: all of them means placement or rules; some means routing luck
  - a suggested fix, from a small table in `crates/pcbgen/src/failure.rs`
- **`boards/<name>/route-failures.jsonl`** is committed. It has one line per failed order: date, board, a layout fingerprint, the order, and each error's type and parts. A re-run of an unchanged board adds nothing. The stage prints the error types by frequency.
- **The rule:** when the same error type fails on two boards, the stage prints `RULE (D-020)`. Turn it into a prevention rule the router gets, and add a `HARDWARE_LESSONS.md` entry.
- **Prevention rules so far:** `RouteOptions::pad_rings` (`PadRing::new("J1", "SH")`) keeps tracks and vias off a poured pad's thermal spokes (`starved_thermal`).

## Tests
- `cargo test --release`: unit tests that need no KiCad (DSN writer conventions and pad shapes, SES reader, stitching grid, router log and score, the escalation ladder, the failure report and log, net-class globs, geometry).
- `cargo test --release -p starter -- --ignored`: the whole pipeline on the starter board into a temp directory, checking the reports (needs kicad-cli and Freerouting; a few minutes).

## Code layout
- `crates/pcbgen`: the library. `circuit` (model), `symlib` (`.kicad_sym`), `schematic`, `project` (`.kicad_pro`, `.kicad_dru`), `footprint` (`.kicad_mod` loading and placing), `pcb` (board writer), `board` (typed view of a saved `.kicad_pcb`), `geom`, `dsn`, `ses`, `route`, `failure`, `stitch`, `gates`, `report`, `fab`, `layout` (the types a board's layout uses), `cli`.
- `boards/<name>`: one binary crate per board (a member of the workspace).

## A board directory (a crate)
- `Cargo.toml`: depends on `pcbgen = { path = "../../crates/pcbgen" }`.
- `src/main.rs`: hands the board to `pcbgen::cli::main` (copy it from `boards/starter`).
- `src/circuit.rs`: `build() -> Circuit`. It declares the parts (KiCad symbol, footprint, LCSC#, MPN) and connects them:
  - `c.part(ref, symbol, value, footprint).lcsc(..).mpn(..).block(..).id()` adds a part.
  - `c.connect(net, part, &["PIN", ...])` connects pins (by number or name).
  - `c.nc(part, &[...])` marks pins unused.
  - `c.pwr_flag(&[net])` marks a net as driven from off the sheet.
  - A bad symbol, pin or double connection panics at the line that made it.
- `src/layout.rs`: `layout() -> Layout` with
  - `rules`: net classes (`NetClass::new(name, track, clearance, via_dia, via_drill).patterns(..)`).
  - `spec`: board size, `places` (every reference: `at(x, y)`, `at_rot(x, y, deg)` or `at_bottom(x, y, deg)` for the back side, mm from the top-left with Y down; a bottom part's angle is the one KiCad shows, D-018), silk `Text` labels, and `CopperZone`s (local pours of one net, e.g. regulator cooling copper; filled above GND).
  - `route`: Freerouting and stitching options (`RouteOptions`: `tries`, `parallel`, `fanout`, `drc_checks`, `extra_rounds`, `extra_tries`, `pad_rings`, stitching).
  - `waivers`: `Waiver { kind, substring, reason }` entries.
- Generated: `kicad/` and `fab/`.
- Modified footprints go in `lib/footprints/<Lib>.pretty` (repo root); the `sch` stage points the project's fp-lib-table there for any library of that name. Currently `pcbgen:ESP32-S3-WROOM-1_EPAD-Drill0.3` (D-015).

## Status (2026-09-29, starter board)
- **All gates pass on a clean end-to-end run of the Rust pipeline:** netlist round trip, ERC 0, DRC 0 open (11 waived cosmetic silk warnings), routing 0 unrouted and no copper in keep-outs.
- **Numbers from the latest run are in `kicad/reports/routing.json`** (with the kept router order under `router`). Freerouting's tracks differ between pipeline versions (see D-016, D-017), the design does not: 20 +3V3 cooling vias, smallest hole 0.3 mm.
- **Fab files:** made. 8 parts have UNVERIFIED rotations (listed in `fab/README.md`).
- **Datasheet check and blind review done** (D-014). Before any order: check rotations in JLCPCB's preview, the fab's own manufacturability check, and the owner's OK on cost.
- **Phase 1 is closed.** The starter board is a tooling test; it will not be ordered unless the owner asks.
- **Reviews:** `research/2026-09-29-datasheet-check.md` (every pin VERIFIED) and `research/2026-09-29-blind-review.md` (no wiring errors; cheap fixes applied in D-014). Reviewer's rough cost for 5 assembled boards: ~$105 delivered to Israel, ~$125 if the parts go over the $75 VAT line. Option A only: the module and DFN sensor can't be hand-soldered.

## Next session
1. Read `HARDWARE_LESSONS.md`, `DECISIONS.md` (open questions at the end), this file and `docs/workflow.md` (how a project runs, D-022).
2. **Next:** brainstorm the first real project with the owner (`docs/workflow.md` step 1), then its spec.
3. Deferred items before any real order are listed under "Open questions" in `DECISIONS.md`.
