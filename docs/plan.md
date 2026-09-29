# Plan: firmware and enclosure tooling, then capture-clip (D-025, PROPOSED 2026-09-29)

Status: **proposed, waiting for the owner's agreement.** Nothing below is built yet. Work is
ordered by dependency, not by date. Each step says what "done" means, so the review pages can
show progress. Research behind it:
- `research/2026-09-29-parts-capture-clip.md` (parts)
- `research/2026-09-29-wokwi.md` (simulation)
- `research/2026-09-29-esp32-firmware.md` (firmware facts)
- `research/2026-09-29-enclosure-tooling.md` (case tooling and printing)

Rule for the whole plan: **build each tool generic, prove it on `boards/starter`, then use it on
capture-clip**, the way pcbgen was built. Nothing here costs money; the only spending is the
eventual order (boards + cases in one parcel, D-021), which needs its own summary and OK.

## What the research changed (2026-09-29)
- **Wokwi can't simulate the microphone or deep sleep on the ESP32-S3.** The S3's I2S peripheral
  isn't modelled, so no custom mic chip can work, and `esp_deep_sleep_start()` ends in a watchdog
  reset. The firmware gets two sim-only switches: injected test audio (a WAV in flash) and a
  "fake sleep" that waits for the button in a loop. Real I2S and real wake are checked on the first
  board (bring-up self-test).
- **Wokwi's free plan is 50 simulated minutes a month.** Short tests (5–15 s) are cheap; network
  runs cost a minute or more. So most sim runs stop before the network, and the Capture protocol
  code is unit-tested on the laptop.
- **Wokwi's internet path is a public, monitored gateway.** The real Gemini key and GitHub token
  never go into a simulation. At most one network run per round, with no secrets or with
  throwaway ones.
- **Enclosure CAD: CadQuery 2.8.0** (Python, in a project uv venv): STEP import of the KiCad board,
  exact interference and clearance, STL/STEP export, headless renders. OpenSCAD still has no
  release after 2021.01 and can't read STEP; build123d is pre-1.0. Python is the rule's exception
  here: the only mature open CAD kernel (OpenCascade) is reached from Python.
- **KiCad's 3D models:** no need for the 3.4 GB `kicad-library-3d`; the ~6 models needed are fetched
  individually. USB-C, the RGB LED and the JST connector have none in KiCad; `easyeda2kicad` 1.0.1
  supplies them (one-time offset fix). `kicad-cli pcb export step` exits 0 with models missing, so
  pcbgen must read its output and fail.
- **JLC3DP:** ~$2–5 per case in white resin, ~$4–10 in nylon (INFERRED from the demo prices). Bare
  PCBs + 3D prints ship together (VERIFIED); **assembled boards + 3D prints in one parcel is
  UNVERIFIED** (ask JLC chat before the first order, or budget a second small parcel).
- **Firmware facts** (`research/2026-09-29-esp32-firmware.md`): sleep current ~13–17 µA typ for the
  whole board (under R2's 20 µA on typical figures; measure at bring-up); OGG/Opus via Espressif's
  `esp_audio_codec` 2.6.2 + `esp_muxer` 1.2.3 (closed, Espressif-only, needs a v6.1 test build;
  fallback AAC-ADTS or our own ~200-line Ogg writer); LittleFS for the queue (~37 min); secrets in
  HMAC-eFuse NVS encryption without flash encryption or secure boot, so USB re-flashing stays
  normal; press vs 1 s hold works once the PSRAM memory test is off.

## Phase A: shared firmware base, proven on the starter
Goal: any board gets a working, simulated, self-testing firmware from its `board.toml`.
1. **Repo layout.** `firmware/components/` at the repo root, shared by boards through
   `EXTRA_COMPONENT_DIRS`:
   - `board_io`: pins from the generated `board_pins.h`, LED/button helpers, press vs hold.
   - `selftest`: runs the tests named in `board.toml [firmware] self_test` and prints one JSON line
     per test plus a summary line (`SELFTEST {...}`), over the USB console.
   - `provision`: a no-echo line protocol on USB Serial/JTAG that writes named keys into
     encrypted NVS and answers only "ok"/"error"; it never echoes a value back.
   - `sim`: Kconfig switches `SIM_AUDIO` (flash WAV instead of I2S) and `SIM_SLEEP` (loop instead of
     deep sleep). Off in real builds; the fw stage refuses to flash a board with them on.
   - `ota`: two slots plus rollback (app marked good after its self-test passes). USB flashing
     only for now; the network pull comes later without a partition change.
2. **Template** `templates/firmware/`: `CMakeLists.txt`, `sdkconfig.defaults` (16 MB flash, octal
   PSRAM, memtest off, USB Serial/JTAG console, brown-out level), `sdkconfig.sim`, `partitions.csv`
   (nvs, otadata, phy, nvs_keys, ota_0/ota_1 3 MB, storage, coredump), `main/`. The fw stage copies
   it once into `boards/<name>/firmware/` if that directory has no project yet, and keeps writing
   `board_pins.h` every run.
3. **`board.toml` additions** (the BOARD.TOML gate checks them):
   - `[[pin]] sim = "button" | "led_r" | "led" | "pot" | ...`: which Wokwi part stands in for it.
   - `[[sim.scenario]]`: named scenarios (press, hold, selftest) with expected serial lines.
   - `[provision]`: NVS key → where the value comes from (`file:~/.config/capture-notes/config#gemini_api_key`,
     `prompt`, or a literal for non-secrets). Only the reference is stored; values never enter the repo.
4. **pcbgen `sim` stage**: writes `firmware/diagram.json`, `wokwi.toml` and the scenario YAMLs from
   `board.toml`; builds with `idf.py` (sim config); runs `wokwi-cli` per scenario with a tight
   `--timeout`, raised simulated CPU clock and a serial log; writes `firmware/sim.json` (pass/fail,
   simulated seconds used). Runs only when named (it spends quota). The review page shows it.
5. **Host unit tests** for pure-C logic (no ESP32 needed): built with the laptop's gcc and a tiny
   runner, run by `scripts/fw-test.sh`. Used from Phase D on.
6. **Bring-up tool `devctl`** (a small Rust binary in `crates/devctl`): `flash` (idf.py/esptool),
   `selftest` (reads the `SELFTEST` lines and writes `boards/<name>/bringup/selftest-<date>.json`
   for the review page and errata), `provision` (reads `[provision]`, sends the values over serial,
   never prints them), `monitor`. It checks the serial port is the board named in `board.toml`
   (the firmware prints its name at boot).
7. **Proof on the starter:** starter firmware with its 4 self-tests (`sht40`, `i2c_scan`,
   `status_led`, `boot_button`) passing in Wokwi (an SHT40 stand-in if Wokwi has none: checked
   while building), shown on the starter's review page.
   **Done when:** `cargo run -p starter -- fw sim review` goes green from a clean checkout.

## Phase B: enclosure tooling, proven on the starter
Goal: a case is designed, fit-checked and rendered every round, like the board.
1. **Install:** `enclosure/` with `pyproject.toml` + `uv.lock` (CadQuery 2.8.0, easyeda2kicad
   1.0.1), `.venv` gitignored (~1.7 GB, no sudo). A system-notes entry when installed.
2. **3D models:** the needed models fetched from KiCad's GitLab at tag 10.0.6 plus the EasyEDA ones,
   into `lib/3dmodels/` (committed, small) with their offsets fixed once and noted in
   HARDWARE_LESSONS. The footprints in `lib/footprints/` point at them.
3. **pcbgen `case` stage:** writes `case/board.json` (outline, holes, each part's side, position,
   rotation, box and height, and the named features: button, LEDs, mic hole, USB-C mouth, reset,
   battery connector) and `case/board.step` (`kicad-cli pcb export step` with the model dir), and
   **fails on any "Could not add 3D model"** line.
4. **Case template** `enclosure/case.py`: a parametric two-part shell from `board.json` plus a
   `[case]` table in `board.toml` (wall, clearances, material, openings per feature, battery pocket
   size, screw bosses). Rules from the research: walls ≥ 1.2–1.5 mm, ≥ 0.2 mm fixed-fit clearance,
   0.5 mm for moving parts, holes ≥ 1.0 mm (resin), a flanged separate button cap, M2 self-tapping
   screws (no inserts: the owner has no tools for them), a padded battery pocket with swelling room.
5. **Fit gate:** board STEP ∩ case = 0 volume; minimum clearance ≥ the table's value; every opening
   lines up with its feature; the button cap's travel reaches the switch. Writes
   `case/fit.json`, `case/*.stl`, `case/*.step`, and top/side/exploded PNGs for the review page.
6. **Proof on the starter:** a simple box case that passes the gate and shows on its page.
   **Done when:** `cargo run -p starter -- case` passes and the page shows the case renders.

## Phase C: workflow and pipeline changes
1. **The case moves into the design rounds** (was brief phase 8): every round from the first layout
   on renders and fit-checks the case; the case freezes with the board; its STL/STEP go in the
   same order summary. `docs/workflow.md` step 4 and the review page change to match.
2. **Battery boards in `board.toml [power]`:** several `[[power.source]]`s (usb, battery), each with
   its own budget: USB 500 mA; battery side = the LDO's limit (RT9080: 600 mA) and the cell's
   peak; a `sleep_ua` budget with `[[power.sleep_load]]`s (each with a source, INFERRED or
   VERIFIED). The gate fails when the sleep loads exceed it; the page shows the sleep total next
   to R2.
3. **The review page** gains: simulation results, the case renders and fit check, the sleep
   budget, bring-up self-test results (after the order), and a "things to check" line when a
   secret's expiry is near (the GitHub token expires 2027-09-18).
4. **Skill and lessons:** the pcb-pipeline skill documents `sim`, `case`, `devctl` and the new
   `board.toml` tables; HARDWARE_LESSONS gets each verified gotcha as it lands.

## Phase D: capture-clip firmware in simulation (before any circuit)
Order matters: the parts most likely to fail go first.
1. **Codec spike:** a v6.1 build with `esp_audio_codec` + `esp_muxer` encoding the injected WAV to
   OGG/Opus 16 kHz/32 kbps; the output is copied off and checked on the laptop (`ffprobe`, then
   `tools/gemini_smoke.sh` from Capture, one request of the day). If the libraries won't build on
   v6.1: AAC-ADTS, or our own Ogg writer.
2. **Capture client logic, unit-tested on the laptop** (Phase A.5): note rendering exactly like
   Capture's `NoteFile.kt` (`source: clip`), the `next-number` loop (GET → PUT with sha, retry on
   409/422, 5 tries), Additions (`## Added` with the mark line, merge on a changed sha), the Gemini
   request and answer parsing (§6 schema, "Nothing heard"), the rate gate (one request, ≥ 5 s apart,
   429 waits, daily-quota wait), retry rules (terminal vs retry). Golden files come from real
   Capture notes in `~/Ideas` (read-only).
3. **The app:** state machine (sleep → wake → press or hold → record → stop → queue → upload →
   saved/queued/error), LED colours from the spec, LittleFS queue with one file per recording and an
   `fsync` about every second, the 15-minute cap, streaming upload (base64 in blocks, fixed
   Content-Length), stay awake while USB is present (provisioning, charging lights), battery
   thresholds (no upload below 3.45 V, no recording below 3.3 V, brown-out at 2.84 V), a hold
   with no last note = a new note.
4. **Sim scenarios:** press/record/stop, hold → addition, queue while offline, low battery,
   self-test; each a Wokwi scenario with expected serial lines. One network run per round at most,
   no real secrets.
5. **Real network check:** see "Question for the owner" below.
   **Done when:** all scenarios pass in Wokwi, the unit tests pass, and a real OGG from the device's
   encoder turned into a Capture note.

## Phase E: capture-clip circuit, layout and case (design rounds)
1. **Circuit** (`boards/capture-clip`, crate + `board.toml`; drop the `Cargo.toml` exclude). From the
   parts research plus the firmware research:
   - USB-C with the starter's ESD parts; TP4057 charger at 300 mA (RPROG 3 kΩ), charge LED on VBUS,
     CHRG (through ~100 kΩ) and STDBY to GPIOs
   - AO3401A + RB160M-30 power path, 100 kΩ gate pull-down; RT9080-33 LDO; 22 µF + 0.1 µF at the module
   - JST PH, + marked on silk (which pad is + checked against JST's drawing first)
   - ICS-43434 powered from GPIO6 (no P-FET: 0.3 mA against 40 mA), L/R to GND, 100 kΩ on SD
   - RGB LED anode on VSYS, cathodes into GPIO8/9/10 through resistors
   - battery divider 3:1 (2 × 1 MΩ over 1 MΩ, 100 nF), VBUS sense 100k/150k on GPIO2
   - main button on GPIO0 with a 10 kΩ pull-up, reset button, test points for rework
   - Pin table from the firmware research §1; no protection IC (protected cell required).
2. **Datasheet check** by a separate agent before layout (new parts: charger, LDO, FET, Schottky,
   mic, LED, JST, buttons), with every UNVERIFIED item from the parts research closed.
3. **Layout:** ~30 × 60 mm; antenna end clear of the battery and hand; the mic outside the
   battery's footprint with its hole where a printed chimney and gasket can seal it; USB-C flush;
   button and LED where the case wants them; two M2 holes.
4. **Case** from Phase B, with the battery pocket (Adafruit #3898, 36 × 17 × 7.8 mm) behind the board.
5. **Rounds** (`docs/workflow.md` step 4): full run, cost, sleep budget, sim, case fit, blind review,
   `round.md`, draft tag, review page published as one Artifact updated each round, until the
   owner says "freeze".

## Phase F: order, bring-up, next revision (brief phases 6–8)
Not now: capture-clip waits for other boards to share the parcel (D-021).
1. **Before the order:** `check-frozen.sh`; every gate; the rotations in JLCPCB's preview; the fab's
   own check; the A/B/C table (only A is realistic: module and LGA mic); NextPCB vs JLCPCB promos;
   the two D-021 questions (smallest bare quantity; the pre-ordered module's fee); ask JLC whether
   assembled boards and 3D prints can share the parcel; a real JLC3DP quote. Then one cost summary
   with boards, cases, shipping and VAT, and the owner's OK.
2. **What the owner buys separately:** a protected 400 mAh LiPo with a JST PH lead (Adafruit #3898
   or similar); shipping a lithium cell to Israel still to check. A multimeter check of its polarity
   before plugging it in (the one physical step).
3. **Bring-up:** plug in, `devctl flash`, `devctl selftest` (mic hears sound, each LED colour, button,
   battery voltage, USB sense, Wi-Fi), `devctl provision`, the first real note, then a sleep-current
   reading (needs a meter; the owner is asked only if the battery life looks wrong). Findings go to
   `errata-revA.md`.
4. **Firmware updates** stay over USB until a network update is worth building.

## Open questions this plan closes or keeps
- Closes: the enclosure tool (D-001), Wokwi setup (D-023), where the provisioning lives (a generic
  `devctl provision`, not a `capture-notes` subcommand as D-024 first said: the secrets file is only
  referenced from `board.toml`).
- Keeps (before any real order): 4-layer variant (drop unless a board needs it), vendoring the
  verified footprints (done for new parts as part of Phase B.2), the A/B/C table, promos, D-021's
  two questions, the unexplained `2cca03f` items.

## Questions for the owner (features and money only)
1. **Do you agree with this plan and its order** (shared tools first, proven on the starter, then
   capture-clip)?
2. **One real-hardware test, no buying and no soldering:** plugging your existing ESP32 dev board
   into the laptop once, so the recording's encoder, the Wi-Fi upload and a real Gemini note can be
   tried on real hardware (the simulation can't do the network safely). If it isn't an ESP32-S3, the
   test is smaller (network only).
3. **Wokwi:** the free 50 minutes a month should do; if the rounds run out of it, the Hobby plan is
   about €5.6 a month. You'd be asked before it's bought.
