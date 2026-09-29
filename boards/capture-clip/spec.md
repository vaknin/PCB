# Capture clip: spec, revision A

- **Board:** `boards/capture-clip` (its `board.toml` holds the pin map, power budget and what covers each requirement)
- **Status:** draft (rounds are tagged `capture-clip-draft-<n>`; the frozen version `capture-clip-revA-freeze`). Not to be ordered yet: the owner wants to wait until a few more designs can ship in the same parcel (D-021).

## Purpose
A third device for Capture (`~/Projects/capture`), next to the phone and the laptop: press the
button, speak, and the idea becomes a note on every device. It works like the phone does: it
records, sends the audio to Gemini, and saves the note to the private GitHub notes repo. The
laptop doesn't need to be on, and there is no server of our own (D-024).

## Where it lives
Around the house, and sometimes carried in a pocket or bag. Indoors, dry. Out of Wi-Fi range it
still records and sends the notes when it next reaches a known network.

## Power
- A small rechargeable battery (single-cell LiPo, bought separately, plugs into the board) that
  charges from any USB-C charger or computer port. INFERRED: a 500–1000 mAh cell lasts weeks of
  normal use, because the board sleeps between presses. Measured at bring-up.
- Runs from USB while plugged in, charging at the same time.

## Inputs and outputs
- **One button:**
  - **Press:** start a new recording; press again to stop and send it.
  - **Hold (about 1 s):** record something to add to the last note this device made (like the
    app's "add to note"); press again to stop.
  - The same button also puts the board in flashing mode if it is held while the board powers up.
- **One colour light:**
  - red while recording
  - blue while sending
  - a green flash when the note is saved
  - amber when a note is waiting for Wi-Fi
  - red blinking on an error or a low battery
- A small separate charge light that is on while the battery charges.
- A microphone.
- Wi-Fi, from the module.
- USB-C: charging, flashing and setup. Wi-Fi name and password, the Gemini key and the GitHub
  token are loaded from the laptop over USB, not typed on the device.
- A small reset button, reachable with a pin through the case.

## Size and enclosure
- As small as the module and battery allow; target about 30 × 60 mm for the board, with the
  battery behind it. Final size is set by the layout rounds.
- A 3D-printed case with a button cap, openings for the microphone, light and USB-C, and a
  reset pinhole. It is designed after the board layout settles.

## Budget
No order planned yet. When it is ordered, it goes in the same parcel as other boards (D-021). As
a guide, about $150 or less for this design's share, delivered to Israel with VAT (INFERRED from
the brainstorm estimate). Each round's review page shows the live estimate.

## Requirements
- **R1** Runs from a single-cell LiPo battery and from USB-C, and charges the battery from any USB-C charger or computer port (5 V, no USB-PD).
- **R2** Sleeps between uses, so the battery lasts weeks (whole board under 20 µA asleep).
- **R3** One button: a press starts or stops a new recording; a hold of about 1 s starts an addition to this device's last note.
- **R4** Records speech clearly from about arm's length with an on-board digital microphone.
- **R5** Keeps at least 30 minutes of recordings when there is no Wi-Fi (one recording up to 15 minutes, like the app), and sends them once connected.
- **R6** Sends each recording to Gemini and saves the note to the GitHub notes repo in Capture's note format, so it shows up on the phone and laptop.
- **R7** Shows its state with one colour light, plus a charge light.
- **R8** Knows its battery level and whether USB is plugged in, and warns when the battery is low.
- **R9** Flashed, set up and debugged over the same USB-C cable, without extra hardware; survives static and surges on the USB port.
- **R10** Built around a pre-certified ESP32-S3 module (Wi-Fi); no antenna design.
- **R11** Fully assembled by JLCPCB with in-stock parts, passing their design rules.

## What "done" means
- Plugged in, the charge light comes on and the board shows up as a USB serial port.
- Its self-test reports:
  - the microphone hears sound
  - each colour of the light works
  - the button is seen
  - the battery voltage makes sense
  - USB is detected
  - Wi-Fi connects
- Unplugged: press, say a sentence, press again. The light goes red, then blue, then green, and
  within a minute or so a note with a title and summary appears on the phone and laptop.
- It still works after a week in a drawer.

## Not in this revision
- On-device speech recognition, a wake word, a speaker, vibration (the brainstorm chose a light only).
- Playback, a screen, typed notes.
- Wireless charging, USB-PD.
- An on/off switch: the board sleeps instead (INFERRED to be enough; revisit if the battery life
  measured at bring-up is poor).

## Open questions
- None for the owner yet; technical ones are in `DECISIONS.md` (D-024).
