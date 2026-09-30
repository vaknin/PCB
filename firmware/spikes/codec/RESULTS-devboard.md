# Codec spike on a real ESP32-S3: dev-board results (Phase D.5, D-025)

Status: **DONE on 2026-09-30, except the last repeats of the encode matrix.** Encoder speed and
memory, the OGG copied back from the chip, boot time, deep-sleep wake time and the Wi-Fi scan
are all measured. What is left is listed under "Not done yet".

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
- QIO flash mode is in effect in the `fast` builds (see section 4); the `dio` in the image
  header is only how the bootloader itself is loaded.

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

## 3. OGG made on the chip, copied back and decoded (VERIFIED, n = 1 per input)

`./run-real.sh ogg`: build `speech-fast`, complexity 1, 240 MHz; the OGG is written to the flash
storage slot, read back from flash, sent over serial as base64 and checked by length and a
running sum; then `ffprobe` and an ffmpeg decode on the laptop. The files stay in `out/real/`
(gitignored).

| Input | OGG size | Packets | `ffprobe` | Decode |
|---|---|---|---|---|
| speech (21.42 s) | 87,418 B | 1071 | Opus, mono, OGG, 21.40 s, 32.7 kbps | complete: 342,720 samples at 16 kHz (21.42 s), no decode errors |
| synth (20 s) | 81,640 B | 1000 | Opus, mono, OGG, 19.98 s, 32.7 kbps | complete: 320,000 samples (20.00 s), no decode errors |

- Length and sum matched the board's on the first try for both files (the transfer is lossless).
- The speech file is the same size, to the byte, as the one the QEMU spike made (87,418 B), and
  decodes to the same level (mean −47.6 dB, peak −10.2 dB against −47.7 / −10.1 dB for QEMU's
  complexity-0 file). The content was not listened to or transcribed.
- ffmpeg prints one warning per file, "non monotonically increasing dts ... 979200 >= 979200":
  the last OGG page carries the same end position as the one before it. QEMU's file gives the
  identical warning, so it comes from `esp_muxer`'s OGG writer, not from the hardware. Decoding
  is unaffected. INFERRED: a strict server-side parser could object; if one does, the fix is in
  how the muxer is closed, not in the encoder.
- `ffprobe` says 48 kHz because Opus always declares 48 kHz; the audio inside is 16 kHz wideband.

**For the product:** the record → Opus → OGG → flash → read back path is proven on the real
chip, with a file ordinary tools open. Nothing to change.

## 4. Boot time, reset → `app_main` (VERIFIED, n = 5 per build)

`./run-real.sh boot`. The firmware stores the RTC clock just before `esp_restart()` and reads it
again as the first line of `app_main`. The figure therefore includes the chip shutting down and
the ROM loader, so it is an upper bound. App image 1.35–1.38 MB in every build.

| Build | What differs | Reset → `app_main`, median (min–max) |
|---|---|---|
| `plain` | IDF defaults (`-Og`, flash in DIO mode) | 359.1 ms (358.8–359.3) |
| `fast` | `-O2`, bigger caches, flash in QIO mode | 319.3 ms (318.0–319.5) |
| `fast-memtest` | + `CONFIG_SPIRAM_MEMTEST` | 842.1 ms (840.8–842.3) |
| `fast-skipvalidate` | + skip image check on wake (no effect on a reset, as expected) | 319.5 ms (319.4–319.6) |
| `fast-quiet-skipvalidate` | + bootloader and ROM logs off, app log level WARN | 259.7 ms (254.6–259.8) |

Where the time goes, from the boot log's own timestamps (`bench.py bootlog`, n = 1 per build,
`fast`): the second-stage bootloader starts at 24 ms, checks the app image from 30 to 257 ms
(227 ms, i.e. about 170 ms per MB of app), PSRAM is found at 261 ms, `app_main` is called at
275 ms. The remaining ~45 ms of the 319 ms is the restart itself and the ROM loader.

- **The PSRAM memory test costs 523 ms** (842 − 319; the log shows 261 → 784 ms).
- **Log printing costs about 60 ms** (319 → 260).
- **QIO flash mode is on in the `fast` builds** (boot log: `flash io: qio`; `plain` says `dio`).
  The image header still says DIO, which is what the earlier note in section 1 saw; the
  bootloader switches to QIO itself. QIO plus `-O2` makes the image check 36 ms faster
  (263 → 227 ms) and the boot 40 ms faster.

## 5. Timer deep-sleep wake → `app_main` (VERIFIED, n = 5 per build)

Same method: RTC clock stored just before `esp_deep_sleep_start()`, read at the top of
`app_main`, minus the 3000 ms asked for. Includes entering sleep, so an upper bound. Check on
the method: with 1 s and 10 s sleeps the result moved by 0.4 ms (364.2 vs 364.6 ms, `plain`,
n = 3 each), so clock drift during the sleep does not distort it.

| Build | Wake → `app_main`, median (min–max) |
|---|---|
| `plain` | 364.4 ms (364.3–364.5) |
| `fast` | 324.7 ms (321.5–325.0) |
| `fast-memtest` | 848.0 ms (847.7–848.4) |
| `fast-skipvalidate` | 100.3 ms (100.2–100.3) |
| `fast-quiet-skipvalidate` | **40.1 ms** (40.0–40.1) |

- Without the skip option a wake costs the same as a full reset: the bootloader re-checks the
  whole app image (224 ms here) every time.
- Only a timer wake was measured. A button (GPIO) wake goes through the same bootloader path, so
  the same figures are expected (INFERRED); nothing was wired to a pin to try it.

**For the product (sections 4 and 5):**

- Set `CONFIG_BOOTLOADER_SKIP_VALIDATE_IN_DEEP_SLEEP=y`, `CONFIG_BOOTLOADER_LOG_LEVEL_NONE=y`,
  `CONFIG_BOOT_ROM_LOG_ALWAYS_OFF=y`, keep `CONFIG_SPIRAM_MEMTEST=n`, and use the `fast` settings
  including QIO. `CONFIG_BOOT_ROM_LOG_ALWAYS_OFF` burns an eFuse on first boot in some IDF
  setups; it was only set in sdkconfig here and no eFuse was burned, so check what it does before
  shipping it (INFERRED from the IDF option's description, not tested).
- **Press-to-recording latency (INFERRED):** wake → `app_main` is 40 ms with those settings.
  The app image check is skipped on wake, so this does not grow with the product app's size.
  What is left is starting I2S and the microphone (not measured; the mic's own start-up time
  from its datasheet will dominate), so a press should reach "recording" in roughly 40 ms plus
  the microphone start-up. Without the skip option it would be about 170 ms per MB of app on
  every press.
- A cold boot (battery inserted, crash, update) is slower and does grow with app size: about
  260 ms for this 1.35 MB app, so expect 350–450 ms for a 2–2.5 MB product app (INFERRED).
- Skipping the image check on wake means a flash image damaged while asleep is not caught until
  the next full reset. That is the normal trade for battery devices; the full check still runs on
  every power-on and after every update.
- Leaving the PSRAM test off is safe for boot time; run a PSRAM pattern test once in the
  factory/self-test instead (the `psram` command here does 7.2 MB in 0.9–1.4 s).

## 6. Passive Wi-Fi scan (VERIFIED, n = 5, plus 1 trial run that agreed)

`./run-real.sh scan`, build `fast`: passive scan of all 2.4 GHz channels, 120 ms per channel,
on the dev board's own antenna at the owner's desk. Only the count and the timings are kept
(`out/real/scan.jsonl`); no network names, addresses, signal levels or channels are stored.

| What | Value |
|---|---|
| Networks found | 11–14 per scan (11, 13, 14, 12, 14; the trial run found 12) |
| Scan duration | 1693–1694 ms every time (trial: 1691 ms) |
| Wi-Fi driver start before the scan | 76–137 ms for the first scan after a boot (n = 2), 18–27 ms for later ones (n = 4) |
| Errors | none; start, scan, stop and restart of the driver worked 5 times in one boot |

**For the product:** the radio and the Wi-Fi stack work on this chip and IDF version. A full
passive scan costs 1.7 s, so the product should not scan before every upload: connect straight
to the saved network (its channel can be remembered) and scan only when that fails (INFERRED; the
connect time itself was not measured, no credentials were put on the board). The count of
networks says nothing about the product: different antenna, case and room.

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

| Remaining | Command (from `firmware/spikes/codec/real/`) |
|---|---|
| Finish the encode matrix to n = 3, plus Opus AUDIO mode, plus the default-settings build | `./run-real.sh enc` (about an hour; appends to `out/real/enc-fast.jsonl`, so move the old file away first) |
| Button (GPIO) wake → `app_main` | `python bench.py sleep 60000 ext0` with a wire from GPIO0 to ground to wake it; never tried |
| Wi-Fi connect time, upload during recording | needs credentials on the board; deliberately not done here |

Single steps: `./build.sh <variant> [flash]` builds into `build-<variant>/`; variant names join
the `sdkconfig.*` fragments with `-` (`fast`, `speech-fast`, `fast-memtest`,
`fast-skipvalidate`, `fast-quiet-skipvalidate`, `plain`). Then, inside the IDF environment
(`. ~/esp/esp-idf-v6.1/export.sh`): `python bench.py status`,
`python bench.py enc 0 240 synth psram 0`, `python bench.py matrix 1 out.jsonl`.

How boot and wake are measured: the firmware stores the RTC clock in RTC memory just
before `esp_restart()` or `esp_deep_sleep_start()` and reads it again at the top of `app_main`;
the laptop asks for the result with `status` after the USB port has come back. This avoids the
USB port vanishing in deep sleep and re-enumerating on wake. Both figures include the few
milliseconds the chip spends shutting down, so they are upper bounds. The spike app is smaller
than the product app, and the bootloader's image check scales with app size.

Board state now (after `./run-real.sh finish`): the app slot was wiped and flashed with the `fast`
build (no recording inside; `status` answers `"speech":0`), storage slot 0 erased (`dump`
answers "slot 0 is empty"), no Wi-Fi credentials or other secrets were ever written, no eFuse
burned, idle at the command prompt, port free.

Known problems and fixes made on 2026-09-30:

- **Opening the serial port used to reset the chip.** Linux raises DTR and RTS on open; pyserial
  then dropped DTR before RTS, and "DTR low, RTS high" is the reset signal of the USB
  Serial/JTAG bridge. `status` therefore always showed reset reason 11 (USB) and `restart` /
  `wake` could not work. Fixed in `bench.py` (`open_port`: RTS drops first). Consequence for
  sections 1 and 2: every encode run there started from a fresh boot. The figures stand (they
  repeat to 4 digits), but they were not taken on a long-running system.
- A `status` sent while the chip is still booting after a wake arrives damaged ("unknown
  command"). `bench.py` now waits 1.5 s after the wake and asks again until the real answer
  comes.
- `build.sh` now takes a file lock around every IDF build (`$IDF_LOCK`), so builds never run in
  parallel with another agent's, and flashes with `esptool` directly (the old `idf.py flash`
  started a build of its own). `build.sh <variant> flash-clean` wipes the whole app slot first;
  `finish` uses it, because flashing a smaller image leaves the end of a bigger earlier one (the
  speech build) in flash.
- `run-real.sh scan` keeps only the count and timings from the board's answer.
- Not done: reading the flash back to prove byte-for-byte that nothing of the speech build is
  left. The slot was erased (esptool: "region erased successfully") and re-flashed, which is the
  basis for saying it is clean.
- ffmpeg warns about a repeated end position in the last OGG page (section 3); harmless so far.
- A laptop build was killed once for lack of memory (`cc1` killed) while other builds were
  running; the lock above is the fix.
- `pkill -f "bench.py matrix"` also kills the shell that runs it; stop a run by its PID.
- `real/.gitignore` ignores `build*/`; nothing under `real/` is committed yet.
- Current draw cannot be measured on this setup.

## Proposed HARDWARE_LESSONS entries

1. **ESP32-S3 USB Serial/JTAG: opening the port from pyserial resets the chip** unless RTS is
   dropped before DTR (`s.dtr, s.rts = True, False; s.open(); s.dtr = False`). Symptom: reset
   reason is always USB (11) and state is lost between commands. VERIFIED on the dev board.
2. **Deep-sleep wake re-checks the whole app image unless
   `CONFIG_BOOTLOADER_SKIP_VALIDATE_IN_DEEP_SLEEP=y`:** about 170 ms per MB of app on every
   wake. With the option and bootloader logs off, wake → `app_main` is 40 ms; without, 325 ms
   for a 1.35 MB app. VERIFIED, n = 5.
3. **`CONFIG_SPIRAM_MEMTEST` adds 0.52 s to every boot and wake with 8 MB PSRAM.** Keep it off;
   test PSRAM in the self-test instead. VERIFIED, n = 5.
4. **Flashing a smaller image does not remove a bigger earlier one:** erase the app slot
   (`esptool erase-region`) before flashing when the earlier image held anything private.
   INFERRED from how esptool writes (only the sectors of the new image).
5. **`idf.py flash` builds first.** On a shared machine flash with `esptool write-flash
   @flash_args` from the build directory, and serialise IDF builds with `flock`. VERIFIED.
6. **A full passive Wi-Fi scan takes 1.7 s** (120 ms per channel); do not scan on every upload.
   VERIFIED, n = 5.
7. **`-O2` + QIO flash + larger caches (`sdkconfig.fast`)** cut boot by 40 ms and Opus encode
   time by 6–20 %. VERIFIED.
