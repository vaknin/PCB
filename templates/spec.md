# <Board title>: spec, revision <X>

- **Board:** `boards/<name>` (its `board.toml` holds the pin map, power budget and what covers each requirement)
- **Status:** draft (rounds are tagged `<name>-draft-<n>`; the frozen version `<name>-rev<X>-freeze`)

Plain language: this is the file the owner reads. Each requirement is a list item that
starts with its bold ID (`- **R1** ...`); the BOARD.TOML gate fails if the IDs here and in
`board.toml` differ. Mark anything guessed as INFERRED.

## Purpose
What the board is for, in a sentence or two.

## Where it lives
Indoors or out, on a desk or in a wall, near water or heat, how it is mounted.

## Power
Where the power comes from (USB-C 5 V, a wall adapter, ...) and how much it may draw.

## Inputs and outputs
Sensors, buttons, lights, connectors, radios; what the board senses and what it controls.

## Size and enclosure
Largest size, mounting holes, the case (printed by the fab or none), openings the case needs.

## Budget
What the owner will pay per design, delivered to Israel with VAT.

## Requirements
- **R1** ...
- **R2** ...

## What "done" means
How the owner will know it works: what to plug in, what should happen.

## Not in this revision
What was discussed and left for later, and why.

## Open questions
