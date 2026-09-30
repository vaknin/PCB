# Codec spike on a real ESP32-S3: dev-board results (Phase D.5, D-025)

Status: **PARTIAL, stopped early on 2026-09-30.** The encoder speed and memory figures are
measured. Boot time, deep-sleep wake time, the OGG copied back from the chip and the Wi-Fi scan
are **not measured yet**; see "Not done yet".

Board: ESP32-S3 rev v0.2, 16 MB flash, 8 MB octal PSRAM, built-in USB Serial/JTAG on
`/dev/ttyACM0`. ESP-IDF v6.1, `esp_audio_codec` 2.6.2, `esp_muxer` 1.2.3.
Code: `real/` (firmware `real/main/real.c`, laptop driver `real/bench.py`, `real/build.sh`,
`real/run-real.sh`). Raw answers: `out/real/*.jsonl` (gitignored).

VERIFIED = measured on the board, n given. INFERRED = reasoned from those numbers, not measured.

## 1. Opus encode speed (VERIFIED)

Opus 16 kHz mono, 32 kbps CBR, 20 ms frames, VOIP mode, OGG muxer with a 4096-byte page cache.
Real-time factor (RTF) = time inside `esp_audio_enc_process` ÷ audio time; under 1.0 keeps up.
Frame times are per 20 ms frame, from `esp_timer_get_time()` around each call. The encoder task
is alone on core 1 at priority 5; Wi-Fi is off; input comes from memory, not I2S.

Inputs: "speech" is 21.42 s (1071 frames) of a real recording, embedded only in the `speech-*`
build; "synth" is 20 s (1000 frames) of a speech-like signal made on the chip.

Build `speech-fast` (`sdkconfig.fast`: `-O2`, 32 KB instruction cache, 64 KB data cache).
n is runs per cell; repeats agreed to 4 digits of RTF and within 2 µs on the worst frame.

| CPU | Complexity | RTF speech | RTF synth | Median frame (speech / synth) | Worst frame (speech / synth) | Frames over 20 ms (speech / synth) | n |
|---|---|---|---|---|---|---|---|
| 240 MHz | 0 | 0.267 | 0.307 | 5.4 / 6.1 ms | 9.1 / 10.8 ms | 0 / 0 | 3 / 3 |
| 240 MHz | 1 | 0.276 | 0.313 | 5.4 / 6.2 ms | 9.6 / 11.1 ms | 0 / 0 | 3 / 2 |
| 240 MHz | 3 | 0.527 | 0.595 | 10.0 / 11.7 ms | 18.5 / 20.2 ms | 0 / 2 | 3 / 2 |
| 240 MHz | 5 | 0.657 | 0.722 | 12.5 / 14.5 ms | 21.0 / 22.9 ms | 4 / 40 | 3 / 2 |
| 240 MHz | 10 | 1.124 | 1.235 | 21.0 / 25.3 ms | 36.5 / 41.6 ms | 759 / 922 | 3 / 2 |
| 160 MHz | 0 | 0.371 | 0.427 | 7.4 / 8.4 ms | 12.8 / 15.5 ms | 0 / 0 | 2 / 2 |
| 160 MHz | 1 | 0.384 | 0.437 | 7.4 / 8.7 ms | 13.7 / 15.8 ms | 0 / 0 | 2 / 2 |
| 160 MHz | 3 | 0.757 | 0.856 | 14.2 / 16.9 ms | 26.9 / 29.5 ms | 115 / 261 | 2 / 2 |
| 160 MHz | 5 | 0.949 | 1.045 | 18.0 / 21.0 ms | 30.7 / 33.6 ms | 491 / 579 | 2 / 2 |
| 160 MHz | 10 | 1.649 | 1.814 | 30.7 / 37.2 ms | 53.8 / 61.6 ms | 1050 / 976 | 2 / 2 |
| 80 MHz | 0 | 0.681 | 0.788 | 13.4 / 15.3 ms | 24.1 / 29.3 ms | 29 / 102 | 2 / 2 |
| 80 MHz | 1 | 0.707 | 0.808 | 13.6 / 15.9 ms | 26.0 / 30.0 ms | 19 / 121 | 2 / 2 |
| 80 MHz | 3 | 1.445 | 1.641 | 27.1 / 32.6 ms | 52.2 / 57.4 ms | 913 / 960 | 2 / 2 |
| 80 MHz | 5 | 1.824 | 2.013 | 34.5 / 40.5 ms | 59.6 / 65.6 ms | 1049 / 981 | 2 / 2 |
| 80 MHz | 10 | 3.226 | 3.552 | 59.9 / 73.0 ms | 106.0 / 121.7 ms | 1071 / 1000 | 2 / 2 |

The planned third repeat was cut short (66 of 90 runs), hence n = 2 on most rows.

Other findings, all at 240 MHz on the speech input:

- **Build settings matter.** With IDF's defaults (`-Og`, 16 KB instruction cache, 32 KB data
  cache; build `speech`) RTF was 0.336 / 0.345 / 0.596 / 0.726 / 1.192 at complexity
  0 / 1 / 3 / 5 / 10, worst frame 10.6 / 11.3 / 20.2 / 22.7 / 38.2 ms (n = 1). `sdkconfig.fast`
  is 6–20 % faster, most at low complexity. Which of the three settings gives the gain was not
  separated.
- **Where the encoder's memory sits makes no difference.** State in internal RAM instead of
  PSRAM: RTF 0.2671 vs 0.2672 (complexity 0) and 0.6568 vs 0.6570 (complexity 5), n = 1 each.
- The muxer costs 35–46 ms per ~20 s recording (under 0.3 % of audio time); 40–42 writes, no
  seeks.
- QIO flash mode was requested in `sdkconfig.fast` but IDF still reports `dio`; its effect is
  unmeasured.

## 2. Memory (VERIFIED, 66 runs, same values every run unless noted)

| What | Value | How |
|---|---|---|
| Encoder task stack high-water mark | 22.0–22.2 KB at complexity 0; 23.3–23.5 KB at 1–5; 25.6 KB at 10 | `uxTaskGetStackHighWaterMark` in a fresh 48 KB task per run |
| Encoder heap | 24,580 B in PSRAM + 76 B internal (IDF sends allocations over 16 KB to PSRAM); 24,656 B internal when forced there | free-heap difference around `esp_audio_enc_open` |
| Muxer heap | 4,532 B internal at open, 5,596 B at the end of a recording, 0 PSRAM | free-heap difference around muxer open, and again before close |
| Internal heap free while encoding | 216 KB free, largest block 168 KB, lowest ever 179 KB | `heap_caps_get_free_size` / `_largest_free_block` / `_minimum_free_size` |
| PSRAM | 8,388,608 B found, largest free block 7.2 MB; pattern test over 7,208,960 B: 0 bad words, 1.36 s | `psram` command, n = 1 |

The heap figures are for this bench (no Wi-Fi, no TLS, no file system), so the free amounts do
not carry over to the product; the encoder's and muxer's own sizes do.

## 3–6. Not measured

See "Not done yet".

## What this means for the product

- **Ship Opus complexity 0 at 240 MHz while recording** (VERIFIED basis: RTF 0.27–0.31, so the
  encoder uses under a third of one core; worst frame 9–11 ms, never over 20 ms). Complexity 1
  costs about 3 % more and is equally safe. Complexity 3 and above sometimes takes longer than a
  frame lasts, and 10 cannot keep up at all.
- **160 MHz also works at complexity 0 or 1** (RTF 0.37–0.44, worst frame 15.8 ms).
  **80 MHz does not:** the average keeps up (RTF 0.68–0.81) but 2–12 % of frames overrun 20 ms.
- **Use `sdkconfig.fast`'s settings in the product build** (`-O2`, 32 KB instruction cache,
  64 KB data cache).
- **Encoder task stack: 32 KB** (INFERRED: 23.5 KB peak at the shipped complexity plus about a
  third of margin; the 48 KB used here is more than needed).
- **I2S DMA buffer: at least 120 ms of audio**, e.g. 6 buffers of 320 samples (INFERRED). The
  encoder alone would need only one spare frame, but Wi-Fi activity and flash writes during
  recording were not measured and are the likelier cause of a stall.
- The encoder can stay in PSRAM; it costs no speed.

## Dev board versus the final board

Transfers (same chip family, clock, flash and PSRAM type): encode speed, frame times, stack,
encoder and muxer memory. Does not transfer: anything about the microphone and I2S, Wi-Fi range
and scan results (different antenna and room), current draw (cannot be measured here at all),
and boot and wake times to the extent the product app is a different size.

## Not done yet

Nothing below has been run on the board. The commands exist but `dump`, `restart`, `wake`,
`bootlog` and `scan` are **untested on hardware**.

| Remaining | Command (from `firmware/spikes/codec/real/`) |
|---|---|
| Finish the encode matrix to n = 3, plus Opus AUDIO mode, plus the default-settings build | `./run-real.sh enc` (about an hour; appends to `out/real/enc-fast.jsonl`, so move the old file away first) |
| OGG made on the chip, copied back over serial, `ffprobe` + ffmpeg decode | `./run-real.sh ogg` |
| Boot time reset → `app_main`, with and without `CONFIG_SPIRAM_MEMTEST`, with image validation skipped on wake, with quiet bootloader logs | `./run-real.sh boot` |
| Timer deep-sleep wake → `app_main` | same command (`bench.py wake`) |
| Passive Wi-Fi scan: count and duration only | `./run-real.sh scan` |
| Leave the board clean | `./run-real.sh finish` |

Single steps: `./build.sh <variant> [flash]` builds into `build-<variant>/`; variant names join
the `sdkconfig.*` fragments with `-` (`fast`, `speech-fast`, `fast-memtest`,
`fast-skipvalidate`, `fast-quiet-skipvalidate`, `plain`). Then, inside the IDF environment
(`. ~/esp/esp-idf-v6.1/export.sh`): `python bench.py status`,
`python bench.py enc 0 240 synth psram 0`, `python bench.py matrix 1 out.jsonl`.

How boot and wake are meant to be measured: the firmware stores the RTC clock in RTC memory just
before `esp_restart()` or `esp_deep_sleep_start()` and reads it again at the top of `app_main`;
the laptop asks for the result with `status` after the USB port has come back. This avoids the
USB port vanishing in deep sleep and re-enumerating on wake. Both figures include the few
milliseconds the chip spends shutting down, so they are upper bounds. The spike app is smaller
than the product app, and the bootloader's image check scales with app size.

Board state now: flashed with the `fast` build (no recording inside; `status` answers
`"speech":0`), storage slot 0 erased, idle at the command prompt, port free.

Known problems:

- A laptop build was killed once for lack of memory (`cc1` killed) while other builds were
  running; re-running it worked. Build variants one at a time.
- `pkill -f "bench.py matrix"` also kills the shell that runs it; stop a run by its PID.
- `real/.gitignore` ignores `build*/`; nothing under `real/` is committed yet.
- Current draw cannot be measured on this setup.
