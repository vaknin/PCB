# How a project goes from idea to order (D-022)

This refines the phases in `docs/brief.md` (the owner's original, kept verbatim) after the owner's input on 2026-09-29. Where the two differ, this file wins.

## 1. Brainstorm (as long as the owner wants)
- Claude asks questions and proposes 2–3 different ways to build the idea, with rough cost and effort for each. No part numbers, no schematics.
- The phase ends only when the owner says so. Its output is the spec.

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
  - the power budget
  - each requirement ID and what covers it (a part, a pin, a firmware self-test or a gate)
  - the firmware's self-tests, and optionally the order quantities and the budget
- **Both sides read `board.toml`.** The BOARD.TOML gate checks it against the circuit, and the `fw` stage writes the firmware's pin header (`firmware/board_pins.h`) from it.
- **The gate fails if they drift** (runs in `sch`, `check` and `fw`): a requirement nothing covers or missing from `spec.md`, a pin on a different net or GPIO than the circuit, a GPIO the circuit uses that the map lacks, loads over the power budget. Full list: D-023.

## 3. Firmware in simulation
- ESP-IDF (D-009), in `boards/<name>/firmware/`, run in **Wokwi** with virtual sensors, LEDs and buttons wired to `board.toml`'s pin map. It includes the generated `board_pins.h`.
- **Wokwi is not set up yet** (D-023). It needs ESP-IDF, `wokwi-cli` and a Wokwi token. Set it up with the first firmware: a `firmware/wokwi.toml` plus `diagram.json`, with the parts on the pins from `board_pins.h`. The review page notices `wokwi.toml`.
- Includes a self-test mode that checks every part on the board and reports over USB. The same self-test runs at bring-up.
- **No physical prototype by default.** The owner won't buy modules or solder.
  - Claude asks for a dev-board test only for a specific risk simulation can't settle, e.g. a sensor's behaviour in real air.
  - Only with plug-in parts, and with the reason stated.

## 4. Design rounds (iterate before ordering)
- **Each round:**
  - change the circuit or layout
  - run the full pipeline with every gate
  - render the board and estimate the cost (`cost` stage: live JLCPCB prices)
  - the independent reviewer agent checks it
  - write `boards/<name>/round.md` (`templates/round.md`): what changed and why, open risks, questions for the owner
- **Each round ends in a review page for the owner** (plain language, no schematics):
  - a picture of the board
  - what changed and why
  - the parts and cost against budget
  - the firmware in simulation
  - open risks
- **The page:** `cargo run --release -p <board> -- review` writes `boards/<name>/review/index.html` (gitignored). Publish it as an Artifact for the owner, updating the same one each round.
- Rounds are git tags `<board>-draft-<n>` (`scripts/draft.sh <board>`, on a clean tree), so any two can be compared; the page lists the commits since the previous one.
- It repeats until the owner says **"freeze"**: `scripts/freeze.sh <board>` tags `<board>-rev<X>-freeze` on the reviewed draft. That locks the spec for this revision.

## 5. Order
- On the frozen version (`scripts/check-frozen.sh <board>` passes: the tag exists and nothing in the board directory changed since):
  - all gates
  - the datasheet table
  - the reviewer
  - rotations checked in JLCPCB's preview
  - the fab's own manufacturability check
- Then a cost summary and the owner's OK.
- Ordered as its own order in a combined parcel (D-021): 5 bare boards, 2 assembled.

## 6. Bring-up and the next revision
- The owner plugs the board in. Claude flashes it, runs the self-test, and records what's wrong in `boards/<name>/errata-rev<X>.md` (`templates/errata.md`).
- Rev A is the prototype, so it is built for rework:
  - spare pins on test pads
  - 0 Ω jumpers where a guess could be wrong
  - a USB flashing path
- The next revision batches every errata fix and starts again at step 4.
- Firmware updates go over Wi-Fi once a board works.
