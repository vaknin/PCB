# How a project goes from idea to order (D-022)

This refines the phases in `docs/brief.md` (the owner's original, kept verbatim) after the owner's input on 2026-09-29. Where the two differ, this file wins.

## 1. Brainstorm (as long as the owner wants)
- Claude asks questions and proposes 2–3 different ways to build the idea, with rough cost and effort for each. No part numbers, no schematics.
- The phase ends only when the owner says so. Its output is the spec.

**Choices the owner makes** (at any step): what the owner sees, holds or pays for (the case's material, colour and finish, the board's colour, size against battery life, features, cost tiers) is shown on an Artifact choice page, with a picture, the price difference and the trade-offs of each option and Claude's recommendation first, then asked as a one-click question. Technical choices stay Claude's (DECISIONS.md). Details: the pcb-pipeline skill, "Choices the owner makes".

**Working in parallel** (owner's request, 2026-09-30): the steps below are tracks, not a queue.
- Once the spec and `board.toml` exist, the firmware (step 3) and the circuit, layout and case rounds (step 4) run at the same time, so the owner sees a board and a case early. Nothing is ordered before freeze, so a firmware finding that changes the board costs a re-run, not money. Only the order (step 5) waits for every track.
- Independent pieces of work go to subagents running side by side: research, one module or test file each, the reviewer, the variants for a choice page. Each gets a full brief and its own files; agents that would touch the same files work in separate worktrees.
- One at a time: anything that costs money or quota, the gates on one board directory, and questions to the owner.
- Claude reads each agent's report and then runs the whole check itself; an agent saying "passes" is not a gate.

**Right the first time** (owner's request, 2026-09-30): a mistake found after the order costs a second shipment, so before freeze:
- Every risk is sorted by what fixing it would cost: firmware only (free, over USB or Wi-Fi), rework on the delivered board, or a new board. The rounds exist to empty the last group.
- The owner is asked, on a choice page, what the device might be wanted to do later. Each plausible wish gets its hardware hook now (a sense line, a spare pin on a pad, storage room, two update slots), even if its firmware waits.
- Rev A firmware can update itself over Wi-Fi with rollback, and reports battery level, version and its last error somewhere the owner already looks.
- **The readiness page** (an Artifact, shown before the freeze question): each requirement with how it was proven (simulated, datasheet-verified by the independent checker, measured on the dev board, or not provable before delivery), every open UNVERIFIED or INFERRED item, and what a miss would cost. An unproven item that would need a new board is red, and red blocks freeze unless the owner accepts it by name.
- Independent agents (datasheet checker, blind reviewer, red team) each review without the designer's reasoning.
- What simulation can't prove (radio range, microphone sound, real sleep current, how the case feels) is said plainly on the page.

## 2. Spec: `boards/<name>/spec.md` + `board.toml`
- **`spec.md`** is plain Markdown on a fixed template (`templates/spec.md`):
  - purpose; where it lives
  - power; inputs and outputs
  - size and enclosure; budget
  - what "done" means
  - not in this revision
  - Each requirement has an ID (R1, R2, …).
- **`board.toml`** (`templates/board.toml`) holds the facts that must never disagree:
  - the pin map (signal → module pin)
  - the power budget: one per power source (USB, battery), and for a battery board what it draws asleep
  - each requirement ID and what covers it (a part, a pin, a firmware self-test or a gate)
  - the firmware's self-tests, and optionally the order quantities and the budget
- **Both sides read `board.toml`.** The BOARD.TOML gate checks it against the circuit, and the `fw` stage writes the firmware's pin header (`firmware/board_pins.h`) from it.
- **The gate fails if they drift** (runs in `sch`, `check` and `fw`): a requirement nothing covers or missing from `spec.md`, a pin on a different net or GPIO than the circuit, a GPIO the circuit uses that the map lacks, loads over a source's budget, a sleep total over its budget. Full list: D-023, D-025 (Phase C).

## 3. Firmware in simulation
- ESP-IDF (D-009), in `boards/<name>/firmware/` (the `fw` stage copies `templates/firmware` there once), using the shared components in `firmware/components/` and the generated `board_pins.h`.
- Open source first (D-025): the `sim` stage runs the whole image in Espressif's QEMU (free, unlimited); `sim --wokwi` also runs the pin checks in Wokwi (50 free simulated minutes a month), from the `diagram.json`, `wokwi.toml` and scenario the `fw` stage writes from `board.toml`'s `sim` parts and `[[sim.wokwi_step]]`s. Results go to `firmware/sim.json` and the review page.
- Includes a self-test mode that checks every part on the board and reports over USB. The same self-test runs at bring-up.
- **No physical prototype by default.** The owner won't buy modules or solder.
  - Claude asks for a dev-board test only for a specific risk simulation can't settle, e.g. a sensor's behaviour in real air.
  - Only with plug-in parts, and with the reason stated.

## 4. Design rounds (iterate before ordering)
- **Each round:**
  - change the circuit or layout
  - run the full pipeline with every gate
  - run the firmware in simulation (`sim`)
  - from the first layout on, design and fit-check the case (`case`: the board's 3D model against the printed case, plus pictures). The case is part of the design from then on, not a later phase (D-025).
  - render the board and estimate the cost (`cost` stage: live JLCPCB prices)
  - the independent reviewer agent checks it
  - write `boards/<name>/round.md` (`templates/round.md`): what changed and why, open risks, questions for the owner
- **Each round ends in a review page for the owner** (plain language, no schematics):
  - a picture of the board, and of the case with its fit check
  - what changed and why
  - the parts and cost against budget
  - the power budget per source, and the sleep total next to the battery-life requirement
  - the firmware in simulation
  - open risks, including a secret that expires soon (`board.toml [provision]` `expires`)
- **The page:** `cargo run --release -p <board> -- review` writes `boards/<name>/review/index.html` (gitignored). Publish it as an Artifact for the owner, updating the same one each round.
- Rounds are git tags `<board>-draft-<n>` (`scripts/draft.sh <board>`, on a clean tree), so any two can be compared; the page lists the commits since the previous one.
- It repeats until the owner says **"freeze"**: `scripts/freeze.sh <board>` tags `<board>-rev<X>-freeze` on the reviewed draft. That locks the spec for this revision, and the case with it (`case/` is in the board directory, so `check-frozen.sh` covers it).

## 5. Order
- On the frozen version (`scripts/check-frozen.sh <board>` passes: the tag exists and nothing in the board directory changed since):
  - all gates
  - the datasheet table
  - the reviewer
  - rotations checked in JLCPCB's preview
  - the fab's own manufacturability check
- Then a cost summary and the owner's OK. It covers the boards and the case together: the case's STL/STEP files (`case/`) go to JLC3DP in the same order summary.
- Ordered as its own order in a combined parcel (D-021): 5 bare boards, 2 assembled.

## 6. Bring-up and the next revision
- The owner plugs the board in. Claude flashes it, runs the self-test (`devctl selftest`, saved in `bringup/` and shown on the review page), and records what's wrong in `boards/<name>/errata-rev<X>.md` (`templates/errata.md`).
- Rev A is the prototype, so it is built for rework:
  - spare pins on test pads
  - 0 Ω jumpers where a guess could be wrong
  - a USB flashing path
- The next revision batches every errata fix and starts again at step 4.
- Firmware updates go over Wi-Fi once a board works.
