# Reading the results

Read the reports themselves; an exit code is not a result. Reports are in
`boards/<name>/kicad/reports/` and `kicad/route/`.

## ERC, DRC, gates
- `erc.json`: `sheets[].violations[]`.
- `drc.json`: `violations`, `unconnected_items`, `schematic_parity`.
- `gates.json`: every gate's verdict in one place.
- Positions in KiCad reports are page mm: the board's top-left is (100, 100). `layout.rs` uses
  mm from the board's top-left.
- Warnings pass only with a `Waiver` in `layout.rs` (with a reason). Errors can't be waived.

## routing.json
Unrouted connections and copper in keep-outs (both are gates), vias per net, track length per
layer, track thinner than its class (only the locked 0.2 mm escape stubs are expected), pour
pieces and fill %, and `router`: every footprint order's score and the one kept.
`route/freerouting.log` is the kept order's log (`Net 'X' (N unrouted connection)` lines);
each order's files are in `route/try-<n>/`.

Freerouting's own "N violations" on the starter are by design (same-net pad overlaps, SHT40
stubs); `scripts/fr-violations/run.sh <board.dsn>` lists them. KiCad's DRC is the gate.

## When routing fails
How the stage escalates on its own: `docs/pipeline.md` "When routing fails".
- If the route stage warns that every order left something unrouted, `check` will fail. Give
  the router room (placement, track widths); more orders help only when `failure.json` shows
  some orders failing, not all.
- Read `boards/<name>/kicad/route/failure.json` (also printed). Each open DRC item has its parts, a position in
  layout mm, "N of M orders" and a suggested fix. All orders hit = placement or rules; some =
  routing luck. Fix the placement or rules. Don't raise `--tries` for a fault that hits every
  order: more orders can't fix it.
- **Failure log:** `boards/<name>/route-failures.jsonl` gets a line per failed order; commit it.
  When the stage prints `RULE (D-020)` (the same error type on two boards), add a prevention
  rule the router gets (an opt-in `RouteOptions` field), a `HARDWARE_LESSONS.md` entry, and a
  DECISIONS entry.
- `RouteOptions::pad_rings` (`PadRing::new("J1", "SH")`) keeps tracks and vias off a poured
  pad's thermal spokes (`starved_thermal`). It costs routability near the part: use it only
  when a board hits that error.

## The case: case/fit.json
Passes only on `"ok": true`. Read its `checks`: interference, clearance per case part with the
nearest part, edge gap, each opening against its part's 3D body (`opening.<ref>`, and
`seal.<ref>` for a bottom pinhole), cap travel, battery against case and board, printability,
screw length. Look at `case/case-{iso,exploded,top}.png` too. Any "Could not add 3D model"
fails the stage: models live in `lib/3dmodels`.

## Firmware: firmware/sim.json
The `sim` stage's results (boot, self-tests, provisioning round trip; with `--wokwi` the pin
checks). Tests QEMU can't run report `skip`, which is not a pass.
Each `[[sim.scenario]]` from `board.toml`, and each scenario of the board's own
`firmware/sim/run.py`, has its own entry in `sim.json` `scenarios` (`ok`, `evidence`: the lines
it matched, `qemu_crashes`, `log`) and a row in the review page's Scenarios table. A failure's
`evidence` names the step and what came instead; the whole console is in its `log`.
`qemu_crashes` above 0 with `ok` true is the simulator's flakiness, not the firmware's.
