# Firmware

Layout, Kconfig targets and each test layer in detail: `docs/pipeline.md` "Firmware". Why it is
built this way: D-025 in `DECISIONS.md`, `docs/plan.md`.

## Where things are
- ESP-IDF v6.1 at `~/esp/esp-idf-v6.1`. A board's firmware is `boards/<name>/firmware/` (the
  `fw` stage copies `templates/firmware` there once and writes `board_pins.h` from `board.toml`).
- Shared components in `firmware/components/`: `board` (banner, NVS, marking an update good),
  `selftest`, `provision`, `sensirion`, `capture` (the Capture client logic), `clip`
  (capture-clip's device logic). Look in the directory for the current list.
<!-- pending: lands with 2a/2b/2c/2d -->
- `simcmd`: the shared parser for `SIM ...` console commands (e.g. `SIM BUTTON 1`) that stand in
  for the pins in QEMU. Use it rather than a board's own parser.
<!-- /pending -->
- Put logic in files with no hardware calls, behind small interfaces, so the laptop tests
  reach it.

## Test layers, cheapest first
1. **Laptop** (unlimited): `scripts/fw-test.sh` (gcc tests with sanitizers, then the ESP-IDF
   linux-target app); `--gcc` for gcc only; `--scenarios` adds every board's
   `firmware/sim/run.py` (QEMU against a local mock server, a few minutes).
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

<!-- pending: lands with 2a/2b/2c/2d -->
## Scenarios in board.toml
`[[sim.scenario]]` entries, run by the `sim` stage in QEMU. Each has steps with the verbs
`send` (a console line), `sim` (a `SIM ...` command for a pin), `wait` (a console line),
`expect_json` (a JSON line with given fields), `sleep_ms`, `reboot`. A board's own
`firmware/sim/run.py` is run by the same stage through a hook. Results go to
`firmware/sim.json` and the review page. A readiness proof with
`evidence = "scenario:<name>"` is red if that scenario fails (`rounds.md`).
<!-- /pending -->

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
