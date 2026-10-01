---
name: pcb-pipeline
description: >
  Design an ESP32 (or any) PCB as code in ~/Projects/PCB and take it to a checked order: circuit
  and layout in Rust (pcbgen) → KiCad 10 schematic, ERC, code-placed board, Freerouting, DRC →
  JLCPCB gerbers/BOM/CPL and live cost, plus firmware tested in simulation (laptop, QEMU, Wokwi),
  a 3D-printed enclosure fit-checked against the board, review and readiness pages, freeze and
  order. Use it whenever the owner describes a hardware idea or gadget, or asks to change a
  circuit, pick or look up JLCPCB/LCSC parts, place or route a board, run ERC/DRC, simulate or
  flash board firmware, design the case, compare costs or fabs, or order boards; also for any
  work in ~/Projects/PCB, on capture-clip or the starter board, or in a crate that uses pcbgen.
  Triggers: PCB, circuit board, schematic, KiCad, kicad-cli, Freerouting, DRC, ERC, gerbers,
  JLCPCB, LCSC, BOM, CPL, footprint, pcbgen, ESP32, ESP-IDF, Wokwi, QEMU, devctl, enclosure,
  case, board.toml, freeze, fab order.
---

# PCB pipeline

Repo: `/home/kivan/Projects/PCB` (git, public). Every other path here is relative to it.

## Rules that always apply
- **The owner is not an EE.** Plain, short words; no schematics unless asked; no calendar
  timelines. Decide technical things yourself and log them in `DECISIONS.md`.
- **Nothing that costs money** (orders, paid tools, quota) happens without a clear summary and
  the owner's explicit OK.
- **Read the JSON reports, never an exit code alone.** A stage that "passed" proves nothing
  until its report says so. The same goes for an agent saying "passes": run the check yourself.
- **Mark guesses.** Anything not checked against a primary source is INFERRED or UNVERIFIED,
  in code comments, `board.toml` sources, and what you tell the owner.
- **Work in parallel.** Once the spec and `board.toml` exist, firmware and the
  circuit/layout/case rounds run side by side; only the order waits for all of them. Hand
  independent work (research, one module or test file each, reviewers, choice-page variants)
  to subagents, each with a full brief (goal, files to read, what is ruled out, its own files
  to edit). Two agents never edit one file; agents that overlap get a worktree (symlink
  `tools/` in). One at a time: anything costing money or quota (orders, Wokwi minutes, Gemini
  requests), the gates on one board directory, questions to the owner.
- **One order, right the first time.** Sort every risk by what fixing it would cost: firmware
  only (free) / rework on the delivered board / a new board. Rounds exist to empty the last
  group. See `rounds.md`.
- **The owner's choices go on a choice page.** What the owner sees, holds or pays for, and every
  money-against-function trade-off (which fab, fee-bearing parts or fee-free substitutes,
  cheaper-but-less), goes on an Artifact page with a picture and price per option, then
  `AskUserQuestion`. Never plain terminal text, never decided by Claude. See `choice-page.md`.
- **Gates before any order:** `scripts/check-frozen.sh <board>` OK → every pipeline gate PASS →
  an independent datasheet check → a blind review → a plain-language cost summary with options
  → the owner's explicit OK. Details: `rounds.md`.
- **Public repo:** no secrets, tokens or personal recordings in it; `board.toml [provision]`
  holds references only.

## Start of every session
Read `HARDWARE_LESSONS.md` (verified part data, fab rules, tool gotchas, mistakes) and
`DECISIONS.md` (decisions and open questions). Then the file for your task below.

## Run
```
cargo run --release -p <board> -- [sch] [pcb] [route] [check] [fab] [fw] [sim] [case] [cost] [review] [--out DIR] [--tries N] [--wokwi]
```
No stage named runs `sch` to `fw`; `sim`, `case`, `cost` and `review` run only when named. Stage
table and outputs: `docs/pipeline.md` "Run". Tools: KiCad 10 (`kicad-cli` only), Rust from
`mise.toml`, Freerouting in `tools/` (`scripts/fetch-tools.sh`), ESP-IDF v6.1 in `~/esp/`.

## Task → what to read
| Task | Read |
|---|---|
| How a project runs (brainstorm → spec → firmware in sim → rounds → freeze → order) | `docs/workflow.md`, `docs/brief.md` (gates, cost table) |
| New board, writing `circuit.rs`/`layout.rs`, parts, placement, pcbgen from another crate | `authoring.md`, `docs/pipeline.md` "A board directory" |
| `board.toml` (pins, power, requirements, proofs, risks, case, provision) | `templates/board.toml`, `docs/workflow.md` step 2 |
| Reading ERC/DRC/routing reports, a route that fails, `fit.json` | `results.md` |
| A design round, readiness page, freeze, before an order, cost options | `rounds.md` |
| Asking the owner to choose (look, feel, price, fab, parts) | `choice-page.md` |
| Datasheet checker, blind reviewer, red team | `review-agents.md` |
| Firmware: tests, simulation, scenarios, flashing, provisioning | `firmware.md`, `docs/pipeline.md` "Firmware" |
| Changing pcbgen, case.py or the scripts; tool upgrades | `pcbgen-dev.md` |
| Verified part data, fab prices, tool gotchas | `HARDWARE_LESSONS.md` |

After editing any file in this skill, run `scripts/skill-check.sh` (also the first step of
`scripts/fw-test.sh`): it checks that every script, stage, flag and `board.toml` key named here
exists.
