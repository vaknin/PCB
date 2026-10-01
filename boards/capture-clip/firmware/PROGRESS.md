# capture-clip firmware: progress (Phase D.3 slice 2 + D.4), stopped 2026-09-30

Stopped on request, mid-task. Nothing is committed. No hardware was touched, no real request was
made, no secret was used. No QEMU or mock-server process is left running.

**The tree builds:** QEMU target, real target and the two update images built cleanly after the
last source change; the Wokwi target was never built. Laptop tests: 238 pass.

## Repeatability session, 2026-09-30 afternoon (in progress; supersedes what is below where they differ)

**Step 1, baseline, old firmware, `flock <lock> sim/run.py --no-build` x3, no other QEMU running:**

| Run | Passed | Failed |
|---|---|---|
| 1 | 5 of 8 | press (second press never acted on, 14 s), update and rollback (firmware panic, below) |
| 2 | 7 of 8 | selftest (second `SELFTEST` never ran, 60 s) |
| 3 | 6 of 8 | low_battery (a 206 ms press taken for a hold), update (firmware panic) |

So it was never only update and rollback: every scenario with a recording can fail. The laptop was
busy the whole time with things this session does not own (load average 15-17 on 16 cores, 9 GB in
swap: a Java process at 400 % and a headless browser), which is what makes QEMU slow enough to
show these. Four separate causes were found in the logs:

1. **The loop that reads the button is starved (VERIFIED in the logs; a real fault on the board).**
   It ran at priority 1, pinned to core 0. When the recorder (6) or worker (5) is busy on core 0,
   the loop does not run, even with core 1 idle (ESP-IDF does not move a pinned task). Seen as:
   `SIM OK BUTTON 1 @15645`, `BUTTON 0 @15851`, state `pressed` only at 17064, `addition` at 17314
   (low_battery, run 3); and a second press that is never acted on while the recorder cannot keep
   up with real time (press, run 1).
2. **A queued self-test was silently dropped** when it arrived while the worker still had the
   "fast charge" job of the one before (VERIFIED by reading `main.c`; selftest, run 2). The
   `worker_busy` flag was also set by two tasks without a lock.
3. **The firmware panics in the Ethernet stand-in's driver**: `LoadStorePIFAddrError`, core 1,
   `emac_opencores_receive` (esp_eth_mac_openeth.c:271) reading its receive descriptor at
   0x600cd408, while core 0 is mapping or writing flash (update and rollback run 1, update run 3).
   The address is valid and is read on every frame, so this is QEMU failing the read (INFERRED: its
   two emulated cores run as two host threads and the device model is not safe for that). Not the
   board's code: the real build has no OpenETH.
4. **Press length depended on the script's timing**: `BUTTON 1`, a 200 ms host sleep, `BUTTON 0`.

**Step 2, what was changed:**
- `firmware/components/clip/clip_button.c` (new, in `clip.h`): debounce that dates each edge at the
  moment the pin first moved (`CLIP_DEBOUNCE_MS 30`). Laptop test `firmware/test/test_clip_button.c`
  (6 tests, one replays the 220 ms press with a loop 5 s late). `scripts/fw-test.sh --gcc`: 244 pass.
- `main/main.c`: the pin is sampled every 10 ms from the esp_timer task (nothing of ours can hold
  that up; only a flash write can, for one erase), edges go into a queue with their time, and the
  loop gives them to the state machine with that time, before anything else. The sampler starts at
  the top of every wake, so a press during the first part of a USB wake is kept. The loop now runs
  at priority 7 (recorder 6, worker 5): a press starts and stops the recording at once. The
  simulators' console is at 8. `worker_busy` is a count of open jobs (atomic). A `SELFTEST` that
  arrives while something is going on waits instead of vanishing.
- `main/hw_stub.c`: `SIM PRESS <ms>`: down now, up `<ms>` later by the firmware's clock.
- `sim/run.py`: presses use `SIM PRESS`; a firmware panic's register dump and backtrace go into
  the log; on a timeout `STATUS` is asked and logged; `CLIP_QEMU_EXTRA` (env) adds QEMU flags.

**Step 3, no crash notices (done):** `sim/run.py` starts every QEMU with `LD_PRELOAD` set to what
`scripts/nodump.sh` prints (`qemu_env()`), in the QEMU child's environment only: not the script's
own, not the mock server (which is a thread of the script). If `nodump.sh` fails, one warning line
is printed and QEMU runs without it. QEMU's exit status is unchanged (−11 is still seen and counted).

**Step 4, the two QEMU host crashes of today, from their recorded dumps (`coredumpctl info <pid>`):**
both happened in the first boot of a scenario, within 10 ms of the log line `esp_littlefs: mount
failed, (-84). formatting...` (the blank image's storage being formatted: flash erases and cache
remaps on one core while the other runs), and **neither is the `psram_quad_read` crash of
HARDWARE_LESSONS**:
- 12:51:10, pid 947683, scenario rollback (`rollback.log.crash1`): `tb_tc_cmp ← q_tree_find_node ←
  q_tree_lookup_node ← tcg_tb_lookup ← cpu_io_recompile ← io_prepare ← do_ld_4 ← helper_ldul_mmu`:
  QEMU looking up its translated block after a guest device read.
- 12:53:55, pid 962683, scenario update by its log's name (`update.log.crash1`): in libc's
  `realloc ← g_realloc ← g_hash_table_resize ← g_hash_table_insert ← tcg_constant_internal ←
  fold_shift ← tcg_gen_code` on one emulated core's thread, while the other core's thread was in
  `esp32s3_write_mmu_value → blk_pread` (the flash cache being remapped). Two host threads in
  QEMU's translator and flash-mapping code at once.
  (The task brief had both in rollback with the first backtrace; the dumps say the above.)

## Done and verified (morning session)

| What | Command | Result |
|---|---|---|
| Laptop tests | `scripts/fw-test.sh --gcc` | **238 pass** (was 220). New: `test_clip_status` 8, `capture-clip/test_pins_seq` 7, `test_clip_power` 11 → 13, `test_clip_upload` 27 → 28 |
| QEMU build | `cd boards/capture-clip/firmware && sim/run.py` (builds `build-qemu`, `-v2`, `-bad`) | builds; `firmware.bin` 958,016 B |
| Real build (not flashed) | `. ~/esp/esp-idf-v6.1/export.sh && idf.py -B build -D SDKCONFIG=build/sdkconfig build` | **1,324,000 B (0x1433E0) of the 3 MB slot: 58 % free**. Built `-Og`, 160 MHz (IDF defaults) |
| Scenarios press, hold, offline, power_loss, low_battery, selftest | `sim/run.py --no-build press hold offline power_loss low_battery selftest` | **each passed on its last 2 runs** (one full-suite run and one earlier run). `sim/run.py` was edited after those runs (a wait added after a boot without provisioning, used by power_loss; the crash retry) and these six were not rerun. Evidence lines below |
| Scenario rollback | `sim/run.py --no-build rollback` | passed 7 of 10 runs. **Not repeatable yet**, see Known bugs |
| Scenario update | `sim/run.py --no-build update` | passed 3 of 12 runs. **Not repeatable yet**, see Known bugs |

Evidence lines from the last full-suite run (`build-qemu/scenarios.json` is overwritten by every run; logs in `build-qemu/scenarios/<name>.log`):
- press: `note notes/<id>.md committed (number 1, source clip); audio opus 3.2 s 13168 B; commits ['clip: next number 2', 'clip: add Simulated note 1', 'clip: status']`
- hold: `addition in notes/<id>.md (## Added, commit 'clip: add to Simulated note, added to'); 1 status commit for 3 uploads; gone target became note notes/<id2>.md`
- offline: `2 recordings queued offline (timer wake in a minute), both committed after SIM NET 1 (numbers [1, 2]); a lost GitHub answer still gave exactly 3 notes`
- power_loss: `killed 4.5 s into a recording; recover queued=1; note committed with 4080 ms (ffprobe 4.08 s); reboot: still 1 note`
- low_battery: `3600 mV: recorded, no radio, wake_after 0; 3350 mV: refused; sag to 3500 mV under load: Wi-Fi off, queue kept; USB: 2 notes sent, fast charge only with the radio off`
- selftest: `6 tests: mic skip, led_rgb skip, button skip, battery pass, usb_sense skip, wifi pass; summary {"pass": 2, "fail": 0, "skip": 4, "missing": 0}; a 1200 mV cell fails the battery test`
- rollback (when it passes): `sim-bad booted on trial in ota_1, failed its check, rolled back to b916044-dirty (ota_0); not downloaded again (1 download); status file says '... firmware sim-bad failed its check and was rolled back'`
- update (when it passes): `wrong SHA-256 refused; b916044-dirty (ota_0) -> sim-2 (ota_1), 958016 bytes, kept after its check and after a power cycle; 3 notes`

Low-water marks seen in QEMU (bytes free, the least over all scenarios; QEMU, `-Og`, no TLS and no Wi-Fi stack, so **not** the real board's): internal heap 228,292; PSRAM 8,356,780; main task stack 5,248 of 8,192; worker 7,496 of 12,288; recorder 10,784 of 32,768.

## What was built

`boards/capture-clip/firmware/`
- `main/main.c`: the event loop: feeds `clip_sm_step`, carries out its actions; worker task for everything that waits on the network; console commands `STATUS`, `RETRY`, `SELFTEST`; RTC-kept state.
- `main/hw.h`: the seams. `hw_gpio.c` (real pins, ADC; also Wokwi) / `hw_stub.c` (QEMU: `SIM BUTTON|USB|CHRG|BATTERY|LOADED|NET|SKIP|TIMER`, prints `PIN {...}`); `mic_i2s.c` / `mic_synth.c` (made-up tones, no voice recording in the repo); `sleep_deep.c` / `sleep_fake.c`; `net_wifi.c` (+SNTP) / `net_eth.c` (OpenETH, clock from the mock server).
- `main/pins_seq.c/.h`: every pin rule of board.toml as ordered sequences over a tiny ops table; `test/test_pins_seq.c` checks them against a made-up chip (CHG_FAST never driven high or pulled, CHRG never a pull-up, STDBY pull-up only on USB, no clock into an unpowered mic, all off and held before sleep).
- `main/store.c` (LittleFS `clip_store_t`), `main/recorder.c` (mic → Opus → Ogg, fsync per page, stops at 256 KB free), `main/http.c` (GitHub on one kept-open connection; Gemini streamed as base64 with a fixed Content-Length; QEMU-only host swap to the mock server), `main/update.c` (manifest, download, SHA-256, slot switch, trial + rollback), `main/selftests.c` (the six tests).
- `main/idf_component.yml`: `esp_audio_codec ==2.6.2`, `esp_muxer ==1.2.3`, `joltwallet/littlefs ==1.22.3` (each the registry's latest on 2026-09-30, VERIFIED by its API).
- `main/Kconfig.projbuild` (`CLIP_SIM_FAIL_TRIAL`, QEMU only), `sdkconfig.defaults` (+ brown-out level 4 = 2.84 V, main stack 8 K, mbedTLS in PSRAM, 1 kHz tick), `sdkconfig.qemu` (+ OpenETH).
- `sim/mock_server.py` (Python stdlib: Gemini, GitHub contents API with 409/422/401/404 and failures on demand, update manifest + image, clock), `sim/run.py` (the 8 scenarios), `sim/sdkconfig.bad`.

`firmware/components/`
- `clip/include/clip.h`, `clip_power.c`: thresholds are now named constants with their source: `CLIP_UPLOAD_LOADED_MIN_MV 3550`, `CLIP_UPLOAD_SAG_MV 100`, `CLIP_UPLOAD_MIN_MV 3650` (at rest), `CLIP_RECORD_MIN_MV 3400`; `clip_battery_loaded_mv()`.
- `clip/clip_status.c` (new): when the status file is worth a commit (first report, firmware or error changed, battery moved 20 points or crossed the upload limit, a day old) and the mark kept on flash.
- `clip/clip_update.c` (new): manifest parser and install decision.
- `clip/clip_upload.c`: status file gains `last_error`; the status callback gets the try's result and a `status_done` hook; a report also after a give-up.
- `provision`: `provision_serve_with()` hands non-PROV console lines to the app.
- `firmware/test/`: `test_clip_status.c` (new), `stubs/driver/gpio.h` (new), changed `test_clip_power.c`, `test_clip_sm.c`, `test_clip_upload.c`.

`scripts/fw-test.sh`: `--scenarios` runs every `boards/*/firmware/sim/run.py`. **Only syntax-checked (`bash -n`), never run.**

## Written but untested
- **Wokwi target:** never built, never run (no quota was spent).
- **Everything behind the real seams** (`hw_gpio.c`, `mic_i2s.c`, `sleep_deep.c`, `net_wifi.c`): compiles in the real build, never executed. Not provable before hardware.
- **TLS:** the QEMU build talks plain HTTP to the mock server. Certificate bundle, handshake heap and the real hosts are unproven.
- `scripts/fw-test.sh --scenarios`, and the full `scripts/fw-test.sh` (the ESP-IDF linux-target part) after the `provision` change.
- The `sim` stage (`cargo run --release -p capture-clip -- sim`) against this app: **not run.** The app prints `SELFTEST_DONE` then `PROV READY` as `sim.rs` expects, and its QEMU has no `-nic`, so the wifi test should report `skip` (INFERRED).
- `run.py`'s retry when QEMU itself dies (`SimulatorDied`, up to 3 tries): written, and it ran in the last loop, but no crash happened in those runs, so the retry path itself is unexercised.

## Not started
- NVS encryption (HMAC eFuse, D-025): left off. It burns an eFuse on first boot and can't be tried in QEMU; a decision for the dev-board session.
- A wake stub to time the button press exactly (the press is dated back by a constant 150 ms instead).
- Storage-full policy beyond "stop recording at 256 KB free"; Ogg page CRC check in the scan.
- `DECISIONS.md` / `HARDWARE_LESSONS.md` entries (proposals below); `docs/pipeline.md` "Firmware" update.
- Scenario results on the review page (needs pcbgen, below).

## Known bugs and open problems
1. **The update and rollback scenarios are not repeatable.** Failures seen: `timed out ... waiting for "upload"` (59 s), `expected 3 note(s), found 1 or 2`, and once `QEMU exited with code -11`.
   - Evidence in the last `update.log`: `SIM OK BUTTON 1 @4493`, `SIM OK BUTTON 0 @4714`, then `"ev":"addition","t":10021`: a 220 ms press was treated as a hold, because the release was not processed for 5 s. The recording after it has 5,120 ms of audio for about 8 s of wall time, so the recorder did not run either for part of it.
   - Cause not found. Three candidates, none tested:
     a. An orphaned QEMU from an aborted run (since killed) was using a core during every repeat run.
     b. Task priorities: the main loop, which samples the button, runs at priority 1, below the worker (5) and the recorder (6). A busy worker can starve it, and a press then reads as a hold. **This one would also be a real fault on the board.**
     c. QEMU stalling the guest while flash is written.
   - Three earlier failures of these two scenarios were races in the scenario script (pressing before the app's loop ran); those are fixed.
2. **QEMU itself died twice in 22 update/rollback runs** (once with exit −11 during an update download; the first time the exit code was not captured). **Corrected:** not the `psram_quad_read` crash of HARDWARE_LESSONS. The two crashes of the afternoon have backtraces in QEMU's translated-block lookup and code generator (top of this file, step 4); the morning's two left no backtrace that was read.
3. **Button events are dated when the loop processes them, not when they happen** (except the wake press). Any stall of the main loop turns a press into a hold. Related to 1b.
4. **A real bug found by the scenarios and fixed:** the fast charge was switched on in the main loop while the radio-off job was still queued in the worker. It now goes through the worker, after the radio is off; `low_battery` checks it from the `PIN` lines.
5. `status` file on flash has no CRC (a torn write reads as "no report yet", which only costs one extra commit).
6. A good update is rolled back if the network is down for three tries during its trial, and that version is then never retried.

## Proven where
- **In QEMU (when the scenario passes):** boot, LittleFS queue, encoder + muxer to a valid Opus file, streamed Gemini request, GitHub reserve/create/update/merge paths, offline queue and backoff, power-cut recovery, battery levels, status-file throttling, update download + SHA-256 + slot switch + trial + rollback.
- **Laptop only:** pin sequences, state machine, thresholds, status policy, manifest parsing.
- **Not before hardware:** I2S mic and gain, ADC accuracy, deep sleep and both wake sources, wake-to-app time, Wi-Fi, TLS, SNTP, USB console, real heap and stack use, encoder speed, brown-out level, LDO dropout figures.

## INFERRED items
- Upload limit 3.55 V under load (HE9073 dropout ~0.46 V typical from a figure); the 100 mV sag allowance (cell resistance ~0.3 Ω); record limit 3.4 V.
- `WAKE_BOOT_MS 150`; mic gain shift 14; mic settle 50 ms; I2S pins come back as plain GPIO after `i2s_del_channel`.
- Self-tests run at boot only after a non-software reset with USB present (so `devctl selftest` works unchanged); on a cold boot on a charger nobody presses the button and that test fails after 10 s, with no other effect.
- Update manifest format and hosting (`update_url` key, `key=value` text; a GitHub API URL gets the token and the raw media type).
- ~~QEMU's segfault is the known flash/PSRAM bug.~~ Wrong: see step 4 at the top (backtraces read).

## Needed from pcbgen and board.toml (not edited)
- `sim.rs`: run `firmware/sim/run.py` when it exists and put `build-qemu/scenarios.json` into `sim.json` (and the review page); or at least pass `-nic user,model=open_eth`.
- `board.toml [provision]`: optional keys `update_url` and `notes_branch` (default `main`); `sim_base` is QEMU-only and must never be listed.
- `board.toml` notes and D-025 still say 3.45 V / 3.3 V; the firmware now uses 3.65 V at rest / 3.55 V loaded / 3.4 V.
- `devctl`: nothing needed.

## Next steps, in order
1. Rerun clean: `cd boards/capture-clip/firmware && sim/run.py --no-build` three times, with no other QEMU running (`pgrep -a qemu-system-xt`).
2. If update/rollback still fail: test candidate 1b. Raise the main task above the worker and recorder (or sample the button in a timer and date its events), rebuild, rerun 5 times.
3. Read `build-qemu/scenarios/update.log` around the stall for what the worker was doing.
4. Run `scripts/fw-test.sh` (full) and `scripts/fw-test.sh --gcc --scenarios`.
5. Run `cargo run --release -p capture-clip -- sim` (not `--wokwi`).
6. Build the Wokwi target once to check it compiles (no run).
7. Decide NVS encryption with the dev-board session; write the DECISIONS / HARDWARE_LESSONS entries.

## Proposed DECISIONS.md entry (not written)
> **Phase D.3 second slice + D.4 (the clip app and its QEMU scenarios), in progress 2026-09-30.** `boards/capture-clip/firmware/main` implements `clip.h` on ESP-IDF v6.1 behind four seams (pins, mic, sleep, network), each with a real and a simulated half. Battery limits now follow the HE9073: no upload below 3.65 V at rest or 3.55 V with Wi-Fi up, no recording below 3.4 V (replaces 3.45 / 3.3 V; INFERRED until bring-up). The status file is written only when it is news (first report, firmware or last error changed, battery moved 20 points or crossed the upload limit, or a day old). Wi-Fi update: a `key=value` manifest at the provisioned `update_url`, SHA-256 checked, new firmware kept only after its own check (storage, keys, recorder, network, manifest), a rolled-back version is not fetched again. Self-tests run after a reset with USB present and on the `SELFTEST` console command. QEMU talks only to `sim/mock_server.py`. Six of eight scenarios pass repeatably; update and rollback do not yet (see `PROGRESS.md`). Real image 1.32 MB of the 3 MB slot. NVS encryption still off.

## Proposed HARDWARE_LESSONS.md entries (not written; each verified in QEMU 2026-09-30 unless marked)
- **QEMU's OpenCores Ethernet works on the esp32s3 machine:** `CONFIG_ETH_USE_OPENETH=y`, `esp_eth_mac_new_openeth` + `esp_eth_phy_new_generic`, QEMU flag `-nic user,model=open_eth`; DHCP gives 10.0.2.15 and the host is 10.0.2.2. Three "mac filter not supported" error lines at start are harmless.
- **OTA works in QEMU** (`esp_ota_begin/write/end`, `set_boot_partition`, trial boot, `mark_app_invalid_rollback_and_reboot`) on an image that already carries the otadata entry. (QEMU's own crashes: see the entries proposed at the top of this file.)
- **`esp_http_client_read` also fires `HTTP_EVENT_ON_DATA`:** a request made with `open`/`write`/`fetch_headers`/`read` that also collects in the event handler gets every body byte twice.
- **Component `REQUIRES` can't depend on `CONFIG_*`:** requirements are read before the configuration exists; list them all and make only `SRCS` conditional.
- **`joltwallet/littlefs` 1.22.3 builds on IDF v6.1;** a blank partition logs "Corrupted dir pair" once and formats.
- **`gpio_reset_pin` turns the pull-up on;** use `gpio_config` for pins that must never have one (CHRG). (From IDF's documentation, not run: no GPIO in QEMU.)
