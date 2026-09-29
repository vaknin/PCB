# ESP32-S3 starter board: spec, revision 0

- **Board:** `boards/starter` (its `board.toml` holds the pin map, power budget and what covers each requirement)
- **Status:** tooling test (Phase 1 closed, D-004 to D-020). Not to be ordered unless the owner asks. Written after the fact as the worked example of `templates/spec.md` (D-023).

## Purpose
Prove the pipeline end to end on the simplest useful board: the one the owner's brief
proposes when no project is chosen yet.

## Where it lives
On a desk, indoors, plugged into a computer or a phone charger.

## Power
From the USB-C socket, 5 V, at most 500 mA (USB's default; INFERRED that every charger
gives at least this). The worst case adds up to about 461 mA, including 100 mA set aside
for whatever is plugged into the Qwiic port.

## Inputs and outputs
- Temperature and humidity sensor on the board.
- Qwiic socket for plug-in sensors (SparkFun/Adafruit style).
- Two lights: power (green) and status (red).
- Two buttons: reset and boot (for flashing).
- Wi-Fi and Bluetooth, from the module.
- Test pads for measuring power and for a serial cable.

## Size and enclosure
50 × 50 mm (the smallest NextPCB's free first assembly takes, D-002), four M3 holes. No enclosure in this revision.

## Budget
About $72 per design delivered to Israel, VAT included (INFERRED, `research/2026-09-29-cost-estimate.md` §5).

## Requirements
- **R1** Runs from any USB-C charger or computer port (5 V, no USB-PD).
- **R2** Survives common USB faults: too much current, surges on 5 V, static on the data lines.
- **R3** Flashed and talked to over the same USB-C cable, without extra hardware.
- **R4** Measures temperature and humidity.
- **R5** Has a Qwiic port for plug-in sensors, on the same bus as the on-board sensor.
- **R6** Debug aids: power light, status light, reset and boot buttons, test points on power, UART and I2C.
- **R7** Built around a pre-certified ESP32-S3 module (Wi-Fi and Bluetooth); no antenna design.
- **R8** Fully assembled by JLCPCB with in-stock parts, passing their design rules; at least 50 × 50 mm.

## What "done" means
Plugged into a computer, the power light comes on, the board shows up as a USB serial
port, and its self-test reports: the sensor answers with a sensible temperature, the
Qwiic scan lists any plugged-in board, the status light blinks, and pressing BOOT is seen.

## Not in this revision
- Battery, USB-PD, anything above 5 V (the brief's first-revision rules).
- An enclosure.

## Open questions
- None for the owner; technical ones are in `DECISIONS.md`.
