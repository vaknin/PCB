# The pcbgen pipeline

Circuit as Rust code → KiCad schematic → ERC → PCB placed by code → Freerouting → DRC → fab files.
Why this shape: `DECISIONS.md` D-004, D-005, D-010 and D-016 (the Rust port, no SWIG).

## Setup (once)
- **KiCad 10** from Arch `extra` (D-001). Only `kicad-cli` is used; no KiCad library is loaded.
- **Rust:** pinned in `mise.toml` (1.98.1); `cargo` comes from mise.
- **Freerouting:** `scripts/fetch-tools.sh` downloads its bundle (with its own Java 25) into `tools/` and checks it.
- **Enclosure (the `case` stage):** `cd enclosure && uv sync` once (CadQuery 2.8.0 in a gitignored `enclosure/.venv`, ~1.7 GB, no sudo). The 3D models are committed in `lib/3dmodels/`.

## Run
```
cargo run --release -p <board> -- [sch] [pcb] [route] [check] [fab] [fw] [sim] [case] [cost] [review] [--out DIR] [--tries N] [--wokwi]
```
With no stage named, `sch` to `fw` run in order; `sim`, `case`, `cost` (network) and `review` run only when named. `--out DIR` writes `DIR/kicad`, `DIR/fab`, `DIR/firmware` and `DIR/review` instead of the board's own directories (for comparison runs). `--tries N` routes N footprint orders instead of the layout's number (`--tries 1` for a quick look while placing). A full run of the starter board takes about 2.5 min, nearly all of it Freerouting (8 orders, 4 at a time, ~55 s each); checking the best 3 adds ~10 s.

| Stage | Does | Output |
|---|---|---|
| sch | writes `.kicad_pro` + `.kicad_dru` (fab rules), then the schematic; refuses if any pin is neither connected nor marked nc; then the netlist round-trip, net-class and BOARD.TOML gates | `kicad/<name>.kicad_sch` |
| pcb | exports the netlist from the schematic, loads footprints, places them from the layout, adds outline, GND pours, local copper zones (`CopperZone`), labels, escape stubs; `kicad-cli pcb upgrade --force` re-saves it | `kicad/<name>.kicad_pcb` |
| route | deletes old unlocked tracks → DSN written by pcbgen (pours hidden from the router) → Freerouting on 8 footprint orders, 4 at a time, fanout off; ranked by fewest unrouted, then least track under class width, fewest vias, shortest → the best 3 are each finished (SES tracks, kicad-cli zone fill, stitching vias for GND and `stitch_local` nets, fill) and DRC-checked in `route/try-<n>/check/`; the best-ranked one with no open DRC item is kept (D-019). If none is clean it escalates, cheapest first: checks the other routed orders, then routes `extra_rounds` × `extra_tries` new orders (default 1 × 8) and checks them. If still none is clean it keeps the one with the fewest open items and writes the failure report (D-020) | same file; the winner's work files in `kicad/route/`, each order's in `route/try-<n>/`, scores and steps in `route/tries.json`; on failure `route/failure.json` and a line per failed order in `<board>/route-failures.jsonl` |
| check | netlist round trip, net-class patterns, BOARD.TOML, ERC, DRC (JLCPCB rules, schematic parity), routing report; fails on any error, unwaived warning, unrouted connection or copper in a keep-out | `kicad/reports/{erc,drc,routing,gates}.json` |
| fab | gerbers + drill (zip), JLCPCB BOM and CPL with rotation corrections (bottom side: 180 − angle, as kicad-jlcpcb-tools); lists parts whose rotation is UNVERIFIED, and every bottom-side part | `fab/` (see `fab/README.md`) |
| fw | the BOARD.TOML gate, then the ESP-IDF pin header from `board.toml` (a board without one is skipped); the first time, copies `templates/firmware` into `firmware/`; when a pin has a `sim` part, the Wokwi files | `firmware/board_pins.h`, `firmware/{diagram.json,wokwi.toml,wokwi-selftest.yaml}` |
| sim | the firmware in Espressif's QEMU (free, unlimited): boot banner, the self-tests, a provisioning round trip; with `--wokwi` also the pin checks in Wokwi (quota: 50 simulated min/month, logged in `~/.config/wokwi/usage.jsonl`) | `firmware/sim.json`, logs in `firmware/build-{qemu,wokwi}/` |
| case | the BOARD.TOML gate (with `[case]`), then `case/board.json` from the saved board (KiCad mm, y down) and `case/board.step` (`kicad-cli pcb export step --user-origin 0x0mm` with `lib/3dmodels`; fails on any "Could not add 3D model"); `enclosure/case.py` builds a tray and lid (plus a cap per `button`) and fit-checks them against the STEP: interference, clearance per case part with the nearest part, PCB edge gap, each opening against its part's 3D body, the pressed cap's travel, printability and the screw length. A `pinhole` opening with `side = "bottom"` goes through the tray's floor instead of the lid, for a part whose port is a hole through the PCB (a bottom-port microphone): the footprint must have exactly one unplated hole, the floor hole sits under it inside a chimney up to the PCB's underside, and the checks are `opening.<ref>` (on the PCB hole's axis within 0.2 mm, under the part) and `seal.<ref>` (the chimney reaches the PCB within 0.05 mm). Passes only on `fit.json` `"ok": true`. ~50 s on the starter. A board.toml without `[case]` is skipped (D-025 Phase B) | `case/{board.json,fit.json}`, `case/case-{bottom,lid}.{step,stl}`, `case/cap-<ref>.{step,stl}`, `case/case-{iso,exploded,top}.png`; `case/board.step` (not committed) |
| cost | prices the BOM line by line from JLCPCB's parts API (live; minimums, attrition, Extended fees), plus PCB, setup, stencil and joints, for `[order]` (default 5 bare, 2 assembled); shipping and VAT apart, since they are per parcel (D-021, D-023) | `fab/cost.json` |
| review | the owner's review page from what the other stages wrote: renders, round and changes since the last draft tag, things to check, readiness, requirement coverage, cost against budget, power, checks (D-023); and the readiness page (below) | `review/{index.html,readiness.html,readiness.json}` (gitignored) |

**The readiness page** (`crates/pcbgen/src/readiness.rs`; `docs/workflow.md` "Right the first time") is what the owner reads before saying "freeze". It comes from two optional things in `board.toml`:
- `[[requirement.proof]]` under a requirement: `how` = `simulated` | `datasheet` | `devboard` | `gate` | `unprovable`, and `evidence` (free text: the scenario, datasheet table, measurement or gate; for `unprovable`, why and what covers it at bring-up). `evidence = "scenario:<name>"` with `simulated` links a QEMU scenario (`[[sim.scenario]]` or `sim/run.py`) in `firmware/sim.json`: red while it fails or has no result.
- `[[risk]]`, one per open guess: `what`, `tag` = `UNVERIFIED` | `INFERRED`, `fix` = `firmware` | `rework` | `new_board`, `miss` (what a miss costs the owner), optional `check` and `accepted` (the owner's words accepting it by name).
- **Red (blocks freeze):** a requirement with no proof; a `new_board` risk not accepted. **Amber:** a requirement with only `unprovable` proofs; a `rework` risk; an accepted `new_board` risk. **Green:** everything else.
- The page gives the verdict ("Ready to freeze" or "N things block freeze"), each requirement with its proofs, the risks in three groups by fix cost, the review page's automatic "things to check", and what simulation can never prove.
- `readiness.json` is `{"red", "amber", "green", "date", "filled", "blocking": [texts of the red items]}`. `scripts/freeze.sh` refuses when it is missing, older than `board.toml`, or has `red` > 0.
- The BOARD.TOML gate fails only on empty `evidence`, `what`, `miss` or `accepted`. Missing proofs are not a gate failure; they are red on the page. The review page shows a chip ("Readiness: ready", "N blocking" or "not filled in" when there is no proof and no risk at all) and a short section.

`scripts/render.sh boards/<name> [out.png] [layers]` renders the top side to look at (`B.Cu,B.Fab,B.Courtyard,B.SilkS,Edge.Cuts` for the back).
`scripts/draft.sh <board>`, `scripts/freeze.sh <board>` and `scripts/check-frozen.sh <board>` tag design rounds, freeze a revision (only on a draft-tagged, clean tree whose readiness page is current and has nothing red), and check before an order that nothing changed since the freeze (D-023, `docs/workflow.md`).
`scripts/fw-test.sh [--gcc]` runs the firmware's laptop tests (see Firmware below).
`scripts/fr-violations/run.sh <board.dsn>` lists the clearance violations Freerouting counts, with the items involved (D-018).
`scripts/nodump.sh` prints the path of a preload library (built from `scripts/nodump.c`) that keeps a crashing QEMU from leaving a core dump or a desktop crash notice; the `sim` stage and the scenario runners use it (`LD_PRELOAD="$(scripts/nodump.sh)"`).
`scripts/skill-check.sh` checks that the pcb-pipeline skill (`.claude/skills/pcb-pipeline/`) names only scripts, stages, flags and `board.toml` keys that exist; `scripts/fw-test.sh` runs it first.

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
- `cargo test --release`: unit tests that need no kicad-cli (DSN writer conventions and pad shapes, SES reader, stitching grid, router log and score, the escalation ladder, the failure report and log, net-class globs, geometry, `board.toml` parsing and every gate error (`[case]` included), the kicad-cli missing-3D-model line (and, with kicad-cli installed, that an empty model directory fails the STEP export), the pin header, cost arithmetic from a canned API answer, the review page's helpers, the readiness page's counting and text). The `board.toml` tests load KiCad's installed symbol libraries.
- `cargo test --release -p starter -- --ignored`: the whole pipeline on the starter board into a temp directory, checking the reports (needs kicad-cli and Freerouting; a few minutes).

## Code layout
- `crates/pcbgen`: the library. `circuit` (model), `symlib` (`.kicad_sym`), `schematic`, `project` (`.kicad_pro`, `.kicad_dru`), `footprint` (`.kicad_mod` loading and placing), `pcb` (board writer), `board` (typed view of a saved `.kicad_pcb`), `geom`, `dsn`, `ses`, `route`, `failure`, `stitch`, `gates`, `report`, `fab`, `boardfile` (`board.toml`, its gate and the pin header), `cost`, `review`, `readiness`, `layout` (the types a board's layout uses), `cli`.
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
    - `notches` (`BoardSpec::notches`): `Notch { edge, at, width, depth }` cuts a rectangle into one straight edge of the outline, e.g. where a battery's lead passes from under the board to a connector on top. `edge` is `Edge::Top`, `Right`, `Bottom` or `Left`; `at` is the notch's centre along that edge (x for Top/Bottom, y for Left/Right); `width` is its size along the edge and `depth` how far it goes in (mm). It must lie in the edge's straight part, clear of the rounded corners and of other notches, or the `pcb` stage fails. Inner corners are drawn square (the fab's router rounds them, about 0.5 mm). The case's shell follows the outline's convex hull, so a notch stays open inside the case.
  - `route`: Freerouting and stitching options (`RouteOptions`: `tries`, `parallel`, `fanout`, `drc_checks`, `extra_rounds`, `extra_tries`, `pad_rings`, stitching).
  - `waivers`: `Waiver { kind, substring, reason }` entries.
- `board.toml`, `spec.md` (from `templates/`): pin map, power budget, requirements; see `docs/workflow.md` step 2. `round.md`: Claude's notes for the current design round, shown on the review page. `errata-rev<X>.md` after bring-up.
- Generated: `kicad/`, `fab/`, `firmware/board_pins.h`, the Wokwi files, `firmware/sim.json` and `case/` except `case/board.step` (committed); `review/` and `firmware/build*/` (not committed). The board's firmware source is `firmware/main/`.
- Modified footprints go in `lib/footprints/<Lib>.pretty` (repo root); the `sch` stage points the project's fp-lib-table there for any library of that name. Currently `pcbgen:ESP32-S3-WROOM-1_EPAD-Drill0.3` (D-015).

## Firmware (D-025)
Each board's firmware is an ESP-IDF project in `boards/<name>/firmware/` using the shared components in `firmware/components/` (`board`: boot banner, NVS, marking an update good; `selftest`; `provision`; `sensirion`; `capture`: the Capture client logic; `clip`: capture-clip's device logic). Kconfig `BOARD_TARGET` = REAL, QEMU or WOKWI; the sim builds add `sdkconfig.qemu` / `sdkconfig.wokwi` in their own build directories. Tested in four layers, open source first:
1. **Laptop** (`scripts/fw-test.sh`, unlimited): gcc tests `firmware/test/test_*.c` and `boards/*/firmware/test/test_*.c`, each naming its sources on line 1 (`// SOURCES: ...`), runner `firmware/test/unit.h`, stubs in `firmware/test/stubs/`, ASan + UBSan. Code needing NVS or FreeRTOS: the ESP-IDF linux-target app `firmware/test/linux`. Keep hardware out of logic files so they can be tested here.
2. **QEMU** (`sim`): the whole image, 16 MB flash + 8 MB octal PSRAM; no GPIO, I2C, I2S, USB, Wi-Fi or deep sleep, so tests needing them report `skip`.
3. **Wokwi** (`sim --wokwi`): pins only. `board.toml` `[[pin]] sim = "button" | "led" | "led_r/g/b" | "pot"` puts a part on the pin; `[[sim.wokwi_step]]` `wait` for a console line, then `press` a button signal (`hold_ms`) or `expect` a signal at `level`. The run ends at `SELFTEST_DONE`. Firmware prints each report only after the button is released (HARDWARE_LESSONS). No real secret ever goes into Wokwi.
4. **Hardware** with `devctl` (`crates/devctl`): `cargo run --release -p devctl -- <flash|selftest|provision|monitor> <board> [--port DEV] [--qemu] [--seconds N]`. `flash` refuses any build whose `sdkconfig.json` isn't `BOARD_TARGET_REAL` and any chip that isn't an ESP32-S3 with 16 MB, then checks the boot banner against `board.toml`. `selftest` writes `bringup/selftest-<date>.json`. `provision` sends `board.toml [provision]` values (`file:<path>#<field>` from a `key=value` file, or `prompt`) only after the banner matches, and never prints them. `--qemu` runs the same against the QEMU image (results stay in `firmware/build-qemu/`). `--seconds N` ends `monitor` after N seconds.

## Status
- The starter board passes every gate end to end (ERC 0, DRC 0 open with waived cosmetic silk warnings, nothing unrouted). It is a tooling test and is not ordered unless the owner asks.
- capture-clip (D-024) is in design rounds; where the plan stands is in `docs/plan.md`, open items at the end of `DECISIONS.md`.
