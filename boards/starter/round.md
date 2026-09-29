## What changed and why
- Nothing on the board itself. This round adds the paperwork every real project will have: a plain-language spec (`spec.md`), and `board.toml`, which ties each firmware pin, the power budget and every requirement to the circuit. The checks now fail if any of these disagree.
- The firmware's pin list (`firmware/board_pins.h`) is now written from `board.toml`, so the circuit and the firmware can't use a pin differently.

## Open risks
- The starter is a tooling test and won't be ordered unless you ask.
- Parts of the power budget are estimates (marked below); the largest, the Qwiic allowance, is a limit we chose, not a measurement.

## Questions for the owner
- None.
