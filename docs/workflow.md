# How a project goes from idea to order (D-022)

This refines the phases in `docs/brief.md` (the owner's original, kept verbatim) after the owner's input on 2026-09-29. Where the two differ, this file wins.

## 1. Brainstorm (as long as the owner wants)
- Claude asks questions and proposes 2–3 different ways to build the idea, with rough cost and effort for each. No part numbers, no schematics.
- The phase ends only when the owner says so. Its output is the spec.

## 2. Spec: `projects/<name>/spec.md` + `board.toml`
- **`spec.md`** is plain Markdown on a fixed template:
  - purpose; where it lives
  - power; inputs and outputs
  - size and enclosure; budget
  - what "done" means
  - not in this revision
  - Each requirement has an ID (R1, R2, …).
- **`board.toml`** holds the facts that must never disagree:
  - the pin map (signal → module pin)
  - the power budget
  - the key parts
  - each requirement ID and what covers it
- **Both sides read `board.toml`.** The circuit code reads it, and the firmware's pin header is generated from it.
- **A gate fails if they drift:** a requirement nothing covers, or a pin used differently by the circuit and the firmware.
- (To build when the first real project gets here.)

## 3. Firmware in simulation
- ESP-IDF (D-009), run in **Wokwi** with virtual sensors, LEDs and buttons wired to `board.toml`'s pin map.
- Includes a self-test mode that checks every part on the board and reports over USB. The same self-test runs at bring-up.
- **No physical prototype by default.** The owner won't buy modules or solder.
  - Claude asks for a dev-board test only for a specific risk simulation can't settle, e.g. a sensor's behaviour in real air.
  - Only with plug-in parts, and with the reason stated.

## 4. Design rounds (iterate before ordering)
- **Each round:**
  - change the circuit or layout
  - run the full pipeline with every gate
  - render the board and estimate the cost
  - the independent reviewer agent checks it
- **Each round ends in a review page for the owner** (plain language, no schematics):
  - a picture of the board
  - what changed and why
  - the parts and cost against budget
  - the firmware in simulation
  - open risks
- Rounds are git tags `<board>-draft-<n>`, so any two can be compared.
- It repeats until the owner says **"freeze"**. That locks the spec for this revision.

## 5. Order
- On the frozen version:
  - all gates
  - the datasheet table
  - the reviewer
  - rotations checked in JLCPCB's preview
  - the fab's own manufacturability check
- Then a cost summary and the owner's OK.
- Ordered as its own order in a combined parcel (D-021): 5 bare boards, 2 assembled.

## 6. Bring-up and the next revision
- The owner plugs the board in. Claude flashes it, runs the self-test, and records what's wrong in `projects/<name>/errata-rev<X>.md`.
- Rev A is the prototype, so it is built for rework:
  - spare pins on test pads
  - 0 Ω jumpers where a guess could be wrong
  - a USB flashing path
- The next revision batches every errata fix and starts again at step 4.
- Firmware updates go over Wi-Fi once a board works.
