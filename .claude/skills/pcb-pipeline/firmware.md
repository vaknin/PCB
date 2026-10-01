# Firmware

Layout, Kconfig targets and each test layer in detail: `docs/pipeline.md` "Firmware". Why it is
built this way: D-025 in `DECISIONS.md`, `docs/plan.md`.

## Where things are
- ESP-IDF v6.1 at `~/esp/esp-idf-v6.1`. A board's firmware is `boards/<name>/firmware/` (the
  `fw` stage copies `templates/firmware` there once and writes `board_pins.h` from `board.toml`).
- Shared components in `firmware/components/`: `board` (banner, NVS, marking an update good),
  `selftest`, `provision`, `sensirion`, `capture` (the Capture client logic), `clip`
  (capture-clip's device logic). Look in the directory for the current list.
- `simcmd`: the shared `SIM ...` console commands that stand in for pins in QEMU
  (`SIM BUTTON 0|1`, `SIM PRESS <ms>`, a board's own verbs through `simcmd_register`). Use it
  rather than a board's own parser. Only simulator builds (QEMU, Wokwi) compile it; a REAL build
  gets the header alone, so wrap every use in `#if !BOARD_IS_REAL`. Read a button through
  `simcmd_button()` when `BOARD_IS_QEMU`. Both `boards/starter/firmware/main/main.c` and
  `templates/firmware/main/main.c` show the wiring (`simcmd_start`, then `simcmd_serve` in the
  console's line handler).
- Self-test output lines are fixed (`firmware/components/selftest`): `SELFTEST {json}` per
  test, `SELFTEST_DONE`, `SELFTEST_BUSY`, and two prompts for the person at the board (devctl
  shows them; Wokwi steps wait on them): `SELFTEST_LOOK <what>` (look or listen: a light, a
  beep) and `SELFTEST_PRESS "<button>"`. Don't invent other kinds; nothing reads them.
- Put logic in files with no hardware calls, behind small interfaces, so the laptop tests
  reach it.

## Test layers, cheapest first
1. **Laptop** (unlimited): `scripts/fw-test.sh` (gcc tests with sanitizers, then the ESP-IDF
   linux-target app); `--gcc` for gcc only; `--scenarios` adds QEMU scenarios (a few minutes): the `sim`
   stage of every board with `[[sim.scenario]]` (its `sim.json` goes to the log directory), and
   `firmware/sim/run.py` of the others (QEMU against a local mock server).
   A test file's first line names its sources: `// SOURCES: firmware/components/<name>/<file>.c ...`.
2. **QEMU** (unlimited): `cargo run --release -p <board> -- sim` → `firmware/sim.json`. No GPIO,
   I2C, I2S, USB, Wi-Fi or deep sleep, so those tests report `skip`.
3. **Wokwi** (pins only; 50 simulated minutes a month: run sparingly, one at a time):
   `sim --wokwi`. Driven by `board.toml` `[[pin]] sim` parts and `[[sim.wokwi_step]]`s.
4. **Real hardware**: `devctl` (below). A dev-board session only for a specific risk
   simulation can't settle, with plug-in parts and the reason stated.

Never put a real secret into a Wokwi run (its gateway is public and monitored). Laptop and
QEMU tests use a local mock server; at most one real API request per test session.

QEMU sometimes segfaults. The `sim` stage and `run.py` preload `scripts/nodump.sh`'s library so
a crash doesn't raise a desktop crash notice: any new QEMU runner should do the same
(`LD_PRELOAD="$(scripts/nodump.sh)"`).

## Scenarios in board.toml
`[[sim.scenario]]` entries (`name`, `about`, optional `nic` for QEMU's Ethernet, optional
`provision`), run by the `sim` stage in QEMU after its boot check, each on a fresh copy of the
image. Steps (`[[sim.scenario.step]]`), one verb each: `send` (a console line), `sim` (sends
`SIM <it>`, waits for `SIM OK <it>`), `wait` (a regex for the next line), `expect_json` (the
next `<TAG> {json}` line whose `fields` match), `sleep_ms`, `reboot` (a power cycle on the same
image); `timeout_ms` goes with `wait`/`expect_json` (30 s if not given). Full syntax: the
comments in `templates/board.toml`.
- `provision` takes literal test values only (they are in the repo), never a secret or a
  `file:`/`prompt` reference; the BOARD.TOML gate refuses one.
- A QEMU crash (the simulator dying from a signal, not the firmware) reruns the scenario, up
  to 3 times; the count goes into the result as `qemu_crashes`, not as a failure. A firmware
  crash (`Guru Meditation`, `abort()`) is a failure.
- A board's own `firmware/sim/run.py` (for what steps can't say: a mock server, several
  builds) is run by the same stage as a hook, without `--no-build`: its extra builds (e.g.
  capture-clip's update and rollback images) are its own, and its `build-qemu` is the one the
  stage just built, so nothing is rebuilt.
- Results go to `firmware/sim.json` (`scenarios`, logs in `firmware/build-qemu/scenarios/`) and
  the review page's Scenarios table.
- A scenario that shows a requirement is met gets linked under that `[[requirement]]`:
  `[[requirement.proof]] how = "simulated"`, `evidence = "scenario:<name>"`. It is red while
  that scenario fails or has no result (`rounds.md`). Add it even while other requirements have
  no proof yet: the readiness page is red until each has one, so a partial set only helps.
- Firmware that waits for a simulated button must keep reading the console, or the `SIM PRESS`
  never arrives: run such work (a console `SELFTEST`, say) in its own task, as the starter does.

## Hardware: devctl
```
cargo run --release -p devctl -- flash|selftest|provision|monitor <board> [--port DEV] [--qemu] [--seconds N]
```
- `--port DEV` picks the serial device (`/dev/ttyACM0`); `--qemu` runs against the QEMU image
  instead (not with `--port`; results stay in `firmware/build-qemu/`); `--seconds N` stops
  `monitor` after N seconds.
- `flash` refuses any build that isn't `BOARD_TARGET_REAL` and any chip that isn't an ESP32-S3
  with 16 MB, then checks the boot banner against `board.toml`.
- `selftest` writes `bringup/selftest-<date>.json` (shown on the review page) and tells the
  person at the board what to press or look at.
- `provision` sends `board.toml [provision]` values only after the banner matches, and never
  prints them.
- The serial side is UNVERIFIED until a real board is attached.

After bring-up, record what's wrong in `boards/<name>/errata-rev<X>.md` (`templates/errata.md`).
