# Decisions

Newest first. Each entry: what was decided, why, and status (proposed / confirmed by owner).
Research behind these: `research/2026-09-29-landscape.md`.

## D-025 Plan: shared firmware and enclosure tooling (proven on the starter), then capture-clip (DECIDED, owner agreed, 2026-09-29)
- **Owner's words (relayed by session pcb-df):** "plan everything, including enclosure, firmware, etc."
- **The plan:** `docs/plan.md`. Phases: A shared firmware base (`firmware/components/`, template, `sim` stage, `devctl` bring-up tool); B enclosure tooling (`case` stage, CadQuery template, fit gate); C workflow changes (the case moves into the design rounds; battery and sleep budgets in `board.toml`); D capture-clip firmware in simulation; E its circuit, layout and case in rounds; F order, bring-up, next revision.
- **Owner's answers:** agreed the plan. "I can plug my esp32s3 once, but not now, save it for later phase" → one dev-board session in Phase D.5, after the simulated firmware passes and before the circuit is written. "50 minutes sound like nothing, are you sure Wokwi is the best one? no open-source method?" → see Simulation below.
- **Research:** `research/2026-09-29-simulators.md`, `research/2026-09-29-wokwi.md`, `research/2026-09-29-esp32-firmware.md`, `research/2026-09-29-enclosure-tooling.md`.
- **Technical choices in it (Claude's, final once the plan is agreed):**
  - **Simulation: open source first, four layers.**
    1. ESP-IDF's `linux` target plus plain gcc tests for the logic.
    2. Espressif's QEMU (GPL, already listed in IDF v6.1's tools) for the whole image: boot, partitions, PSRAM, NVS, LittleFS, the encoder.
    3. Wokwi only for GPIO/ADC checks (press vs hold, LED colours, battery thresholds), which QEMU can't emulate.
    4. The real board, and the owner's dev board once, for I2S, Wi-Fi, USB, deep sleep and timing.
    - Why: layers 1–2 are unlimited and local. Wokwi's free 50 simulated minutes a month then only has to cover short pin checks (~300 runs of 10 s). No simulator models the S3's I2S or deep sleep, so the firmware has sim-only switches (injected WAV, fake sleep) and a target choice `BOARD_TARGET_{REAL,WOKWI,QEMU}`; flashing refuses non-REAL builds. Renode has no real S3 support.
    - Real secrets never go into Wokwi (public, monitored gateway); layers 1–2 test against a local mock server.
  - **Enclosure CAD:** CadQuery 2.8.0 in a project uv venv. OpenSCAD has no release after 2021.01 and can't read STEP; build123d is pre-1.0. Python because OpenCascade, the only mature open CAD kernel, is reached from it.
  - **3D models:** fetched one by one (KiCad GitLab tag 10.0.6, plus easyeda2kicad for USB-C, the RGB LED and JST PH) into `lib/3dmodels/`. `kicad-library-3d` (D-001's deferral) is not installed.
  - **Audio:** OGG/Opus 16 kHz mono 32 kbps via `esp_audio_codec` 2.6.2 + `esp_muxer` 1.2.3 (closed, Espressif-only licence; test build on v6.1 first). Fallback AAC-ADTS or our own Ogg writer.
  - **Storage:** LittleFS, one file per recording, fsync about every second; 16 MB partition table with two 3 MB OTA slots and ~9.8 MiB of storage (~37 min).
  - **Secrets:** HMAC-eFuse NVS encryption, no flash encryption, no secure boot, so USB re-flashing stays normal.
  - **Provisioning:** a generic `devctl provision` reading `board.toml [provision]` references, instead of the `capture-notes` subcommand D-024 mentioned.
  - **OTA:** USB only for Rev A; two slots and rollback kept so a network update needs no partition change.
  - **Circuit consequences for capture-clip:** mic powered straight from a GPIO (no P-FET), battery divider 3:1, a VBUS-sense divider, a 10 kΩ pull-up on GPIO0, CHRG through ~100 kΩ, brown-out 2.84 V, no upload below 3.45 V and no recording below 3.3 V, and a hold with no last note makes a new note.
- **Built in Phase A so far (Claude's choices):**
  - The `fw` stage writes the Wokwi files (`diagram.json`, `wokwi.toml`, `wokwi-selftest.yaml`) from `board.toml` whenever a pin has a `sim` part, and they are committed like `board_pins.h`. Why: they are derived facts, so they are regenerated every run and can't drift; `sim --wokwi` only builds and runs.
  - `sim --wokwi` runs `wokwi-cli lint` first (free, catches wrong pins before any quota is spent) and logs every run's simulated seconds in `~/.config/wokwi/usage.jsonl`, printing the month's total against the 3000 s allowance. `SELFTEST_DONE` now carries the uptime (`ms`), which is the billed time of a finished run.
  - A run without `--wokwi` keeps the last Wokwi result in `sim.json`, since redoing it costs quota.
  - **Laptop tests (layer 1): `scripts/fw-test.sh`.** gcc tests in `firmware/test/test_*.c` (and later `boards/*/firmware/test/`), one program each, with a header-only runner (`firmware/test/unit.h`, prints a `UNIT {...}` line the script reads besides the exit code) and ASan + UBSan; stub `board.h`/`esp_timer.h` in `firmware/test/stubs` let the real `selftest.c` run unchanged. Code needing NVS runs in `firmware/test/linux`, an ESP-IDF linux-target app, also sanitized. Why a home-made runner rather than Unity/CMock: ~80 lines, no dependency, and the same header works in the linux app; ESP-IDF's Unity can be added if a test ever needs mocks.
  - To make code testable, pure logic moved into its own files: `provision_text.c` (the line parser, now rejecting extra words), `provision_cmd.c` (commands against NVS; also builds for linux), `provision.c` (console only). New shared component `sensirion` (CRC-8 and SHT4x conversion, from the starter's `main.c`). The console now refuses a line longer than its buffer (`PROV ERR line`) instead of storing a cut-off value, and `provision_get` no longer drops the last byte of a value that exactly fills the buffer; the QEMU run checks the long-line refusal.
  - **`devctl` (`crates/devctl`, bring-up tool):** `flash`, `selftest`, `provision`, `monitor`, over a serial port (`serialport` 4.10.1, Espressif VID 303a found automatically) or `--qemu` (the sim image, a private copy per run). It reuses pcbgen's `board.toml` parser and banner/self-test checks (`pcbgen::sim::{report, banner, Console}` made public), so the sim stage and bring-up can't disagree. Safety rules: `flash` builds `firmware/build`, refuses unless its `config/sdkconfig.json` is `BOARD_TARGET_REAL` for an esp32s3 with 16 MB, checks the chip with `esptool flash-id`, writes from that same build directory (nothing rebuilds between check and write), then checks the boot banner. `provision` resets the board and checks the whole banner before sending anything; values (`zeroize`d, `rpassword` 7.5.4 for `prompt`) never reach the screen or a log (`>> PROV SET key (n bytes hidden)`); `file:` refs are read like Capture's `conf()` (`key=value`, last line wins). `--qemu` results go to `firmware/build-qemu/`, so `bringup/` holds only real-board results. UNVERIFIED until the dev-board session (Phase D.5): DTR/RTS handling and reset timing on USB Serial/JTAG, reopening after re-enumeration, esptool v5's `flash-id` wording, ~1 KB lines over USB.
  - **Phase A is done (2026-09-29):** `cargo run --release -p starter -- fw sim review` is green, `scripts/fw-test.sh` passes, devctl passes its QEMU tests.
- **Built in Phase B so far (Claude's choices):**
  - `enclosure/pyproject.toml` + `uv.lock`: CadQuery 2.8.0 and easyeda2kicad 1.0.1 (both still the latest stable on 2026-09-29) on uv's Python 3.13, `uv sync` into a gitignored `enclosure/.venv` (1.7 GB).
  - `lib/3dmodels/` (11 MB, committed) holds every model the starter needs plus the capture-clip ones known so far (1TS009, TS-1088, ICS-43434, JST PH), each under the path its KiCad footprint already uses, so footprints stay unchanged. `enclosure/models.py` refetches them and redoes the EasyEDA alignments from a table; `MANIFEST.json` records source and sha256 (`--check` compares). Why committed rather than cached: the case gate must give the same answer from a clean checkout, and 11 MB is small next to the 3.4 GB library. Why the alignment is baked into the STEP files rather than `(offset)` lines in footprint copies: the KiCad footprints stay stock, and the one-time fix lives in one table.
  - **`case` stage (B.3-B.6, 2026-09-30):** `cargo run --release -p <board> -- case` (named only, after `sim`). pcbgen writes `case/board.json` (KiCad board mm, y down; outline, mounting holes with the room to their courtyard edge, every footprint's side/position/rotation/courtyard and F.Fab boxes, `[case]` resolved with the material's limits and the screw's holes) and `case/board.step` (`--user-origin 0x0mm`: STEP x = x, y = -y, PCB bottom at z = 0; checked numerically every run as `step_matches_board`). It fails on any `Could not add 3D model` line. Then `enclosure/case.py` builds and checks; the stage passes only on fit.json `"ok": true`, and deletes the old fit.json first so a stale pass can't be read. Why the case geometry comes from board.json and the check from the STEP: two sources that must agree make a real gate (the research's point); a footprint/model mismatch shows as an alignment failure.
  - **`board.toml [case]`** is part of the BOARD.TOML gate: material (resin | nylon sets wall/hole/skin/fit minimums from JLC3DP's guide), wall, floor, edge_gap, top_gap, bottom_gap, min_clearance, screw (M2 | M2.5 | M3), `[[case.opening]]` ref + kind (usb_c, connector, button, pinhole, led, vent) with per-kind overrides; keys a kind doesn't use, unknown refs, duplicates and sub-minimum walls/holes are errors. Defaults live in Rust (one source); case.py only reads the resolved numbers.
  - **Choices in the shell:** split plane = PCB top face; standoffs (tray) and bosses (lid) clamp the PCB, a self-tapping screw from below; boss OD = max(pilot, clearance) + 2 × 1.5 mm and must fit inside the mounting hole's courtyard. Where the counterbore is deeper than the floor, a wider foot surrounds it. The screw length is picked from standard lengths so it engages 2 × d in the lid (INFERRED rule for thread-forming screws in plastic); **the starter uses M2** because M3 would need 6 mm of thread and its lid has ~4.6 mm inside (result: 4 × M2 × 8 mm, 4.0 mm engagement). A cap's moving fit is **per side** (resin 0.5 mm: hole = cap + 1.0), not the "0.5 smaller" in the brief: JLC's clearance is a gap between surfaces, and holes print ±0.3 mm, so 0.25 per side could jam. The LED window skin is 0.8 mm (resin), not 0.6: the JLC upload checker says "thinnest part ≥ 0.8 mm". The cap has a Ø1.5 × 1 mm stem so, pressed, only the switch's actuator is touched (a flat cap would come within 0.05 mm of the TS-1187A's frame). A connector's mouth faces the board edge nearest its F.Fab box; its cutout is the F.Fab width + margin and the `height` from board.toml (USB-C default 3.26 INFERRED from the HRO model; the Qwiic JST SH 2.95 INFERRED from KiCad's model).
  - **Fit gate robustness:** OCCT's boolean common silently returns nothing for the EasyEDA USB-C model (verified), so interference uses the exact B-rep distance first, a boolean only where it is 0, and a vertex-inside test for parts buried in a wall; connector outlines come from the clipped triangulation, not booleans. Each part group's bounding box gives a lower bound, so only a few exact distances run (the check went from ~100 s to ~29 s; the whole stage ~47 s with renders).
  - **What is committed:** board.json, fit.json, the case STEP/STL (order files, like `fab/`) and the three PNGs (~0.9 MB, small, and they show in git history how the case looked each round); `case/board.step` (5 MB, derived) is gitignored.
  - **Starter result (2026-09-30):** `cargo run --release -p starter -- case` passes all 30 checks: no interference; clearance tray 0.30 / lid 0.30 (U1's antenna edge) / cap 0.20 mm (its switch, at rest); edge gap 0.296; USB-C outline 0.25 mm inside its cutout, mouth 1.15 mm behind the recess face; Qwiic 0.295 mm; buttons, LEDs and vent centred within 0.001 mm; pressed cap 0.25 mm from anything but the actuator.
  - **Phase B finished (2026-09-30):**
    - **Review page:** a "The case" section reads `case/fit.json` and embeds its three renders (data URIs, like the board pictures): a pass/fail chip, the checks count, the smallest clearances, each opening in words, the pieces to print with their size and material volume. A missing fit.json, a failing check, a listed problem, or a board newer than fit.json each go to "things to check".
    - **Negative tests:** `enclosure/test_case.py` (plain script, no pytest: one dependency less) loads the starter's board.step once and runs the unchanged case (must pass) and 7 broken ones (each must fail its named check): lid below the tallest part → `interference.lid`; D2 1 mm off in board.json → `opening.D2`; J2's body moved 1 mm along its wall → `opening.J2`; D1 deleted from the STEP → `opening.D1`; the capture-clip cell in a 9 mm bottom gap passes; an 8.5 mm cell → `battery.board`; the cell over a standoff → `battery.case`. All pass (~4 min; `--battery` runs only the battery ones).
    - **Battery pocket:** `[case.battery]` (size, at, rotate, swell, pad, lead). The cell lies on the tray floor under the board (the capture-clip plan: battery behind the board) inside a rib fence (material's wall_min thick, half the cell's height up to 3 mm, INFERRED), `pad` (0.5) per side, open up to 10 mm on the side facing the `lead` connector. Why a fence on the floor and not foam from the lid: the lid is on the other side of the PCB, and a fence needs no extra parts. The BOARD.TOML gate fails if `bottom_gap` < thickness + swell + min_clearance; the fit gate adds `battery.case` (the cell, lifted 1 mm off the floor it lies on, to the tray and lid ≥ min_clearance) and `battery.board` (cell + swell to every part and the PCB ≥ min_clearance). Swell defaults to 10 % of the thickness (INFERRED). The cell shows blue in the renders.
    - **STEP files no longer change on every run:** OCCT writes the time into the header; `case.py` keeps the date only (like fit.json), so a re-run the same day leaves git clean.
    - **Not done (not needed for capture-clip's first round):** the light-blocking rib around LED windows, a channel for the battery lead, and the ~10 mm of room a JST PH plug needs in front of its mouth (checked by eye on the renders until a board needs it).
    - **UNVERIFIED:** screw pilot/clearance/head table, 2 × d engagement, the USB-C overmold size, the switch travel (0.25), whether thread-forming screws crack 9600 resin bosses, and the renders' fidelity to a printed part.
- **Phase C (workflow and pipeline changes), built 2026-09-30 (Claude's choices):**
  - **The case joins the rounds** (`docs/workflow.md` steps 4–5): every round from the first layout on runs `sim` and `case`; the case freezes with the board (`case/` is inside the board directory, so `check-frozen.sh` already covers it) and its STL/STEP go in the same order summary. The pcb-pipeline skill says the same.
  - **`board.toml [power]` is now per source:** `[[power.source]]` (name, what, optional `sleep_ua`) with `[[power.source.limit]]`s; the budget is the smallest limit, so a battery source lists the LDO's rating and the cell's peak and the page names which one binds. `[[power.load]]` gains `from` (default: every source; a charger is `from = ["usb"]`). `[[power.sleep_load]]` (ua, source, from; default every source with a `sleep_ua`) must say INFERRED or VERIFIED in `source`. The gate fails on a load over any budget, a sleep total over `sleep_ua`, a `sleep_ua` with no sleep loads (a budget nothing is counted against proves nothing), unknown or duplicate source names. Why limits as a list rather than one `budget_ma`: the reason for the number is kept with its source, and the smallest wins automatically.
  - **The starter was migrated, not kept backward compatible.** It was the only board.toml; one format means one parser path and no guessing which form a file uses. Its USB source now also lists the fuse (610 mA at 50 °C) as a second limit; the budget stays 500 mA.
  - **Requirements can be covered by `power:<source>` and `sleep:<source>`.** That is how the sleep total sits next to R2 on the review page: a chip "Asleep: X of 20 µA", amber while any figure is an estimate, red when over. capture-clip's R2 will use `sleep:battery`.
  - **Secret expiry lives in `[provision]`:** a value may be a table `{ from = "file:...#github_token", expires = "2027-09-18" }`; the date only, never the secret. The review page lists it under "things to check" from 60 days before (renewing means re-provisioning the clip, the phone and the laptop, which share the token) and after it lapses; `devctl provision` prints a warning for an expired one. The table rejects unknown keys, so a `value = ...` can't slip in.
  - **Review page:** a table per power source with its limits, a sleep table, a "Bring-up" section from the newest `bringup/selftest-*.json` ("not run yet" until a board is built; a failing one goes to "things to check"), and the power chip turns red when over.
  - **Tests:** `boardfile.rs` `power_sources_and_sleep`, `provision_expiry`, `day_numbers`; `review.rs` `power_section_per_source_and_asleep`, `bringup_reads_the_newest_selftest`. `cargo test --release --workspace` and clippy pass; `cargo run --release -p starter -- check review` passes (BOARD.TOML: usb 460.9 of 500 mA).
- **Owner's choices get a choice page (owner's request, 2026-09-30):** "add to the skill to create an artifact when choosing certain things that I need to pick myself ... the board type the case, type the materials and stuff like that." Added to `docs/workflow.md` and the pcb-pipeline skill: an Artifact with a picture per option (real `case` renders or board renders per variant), price difference and trade-offs, the recommendation first, then a one-click question; the answer goes to DECISIONS.md and `board.toml`.
- **Phase D.1 (codec spike), done 2026-09-30 (Claude's choices): OGG/Opus from Espressif's libraries works on IDF v6.1; no fallback needed.**
  - **Where:** `firmware/spikes/codec/` (a standalone IDF project, not in `boards/capture-clip/firmware/`, which doesn't exist until the `fw` stage makes it in Phase D.3). `run.sh` builds it, boots it in QEMU with the `sim` stage's flags and otadata entry, copies each OGG out of the flash image and runs `ffprobe`.
  - **Components:** `espressif/esp_audio_codec` 2.6.2 and `espressif/esp_muxer` 1.2.3, still the latest on the registry on 2026-09-30, pinned with `==` in `main/idf_component.yml` and `dependencies.lock`. Both resolve and build on v6.1 for the esp32s3.
  - **Input:** Capture's `tools/samples/two-ideas.ogg` (21.4 s of the owner's speech), converted to 16 kHz mono WAV by `run.sh` into the gitignored `out/`. Not committed: this repo is public.
  - **Encoder settings:** 16 kHz mono, 32 kbps CBR, 20 ms frames, VOIP mode, complexity 0. Why CBR: a fixed 4.0 KB/s makes the offline-queue size (R5) exact. Complexity stays 0 until Phase D.5 measures the real CPU cost. In QEMU, complexity 5 cost 2.3× complexity 0 (INFERRED to scale similarly on silicon).
  - **Muxer settings:** a custom `esp_muxer_file_writer_t` (open/write/seek/close), and `page_cache_size = 4096`, which gives about 1 s of audio per Ogg page. Without it, every 20 ms packet gets its own page: 35 % overhead (43 kbps). With it, the overhead is 2 % (32.6 kbps, 87 KB for 21.4 s). The muxer wrote once per page and never seeked, so the real app can append pages to a LittleFS file and `fsync` after each one.
  - **Checked on the laptop** (`ffprobe`/`ffmpeg`): opus, mono, 48 kHz decode rate, 21.40 s, decodes with no errors. A file cut at any byte decodes up to its last complete page, so a power cut loses at most about 1 s.
  - **One Gemini request** with Capture's `gemini_smoke.sh` (gemini-3.5-flash-lite, HTTP 200): the transcript matches the phone's original of the same recording almost word for word, with a title and summary.
  - **Accepted quirks of the muxer's Ogg** (all harmless for ffmpeg and Gemini):
    - pre-skip is 0 (libopus uses 312)
    - the OpusHead input rate says 48000
    - the last page has no end-of-stream flag, and its granule is one packet short, which gives an ffmpeg dts warning
    - Our own Ogg writer stays the fallback if a consumer ever rejects them.
  - **Resources:** the encoder allocates about 24 KB, and IDF puts it in PSRAM (76 B internal). The encode task used 23 KB of stack (48 KB given; 32 KB is enough for the app). QEMU timing (real-time factor 0.04) says nothing about real silicon; Phase D.5 measures it.
- **Phase D.2 (Capture client logic), done 2026-09-30 (Claude's choices): the clip's side of Capture is pure C in `firmware/components/capture`, with 118 laptop tests.**
  - **Shape:** a shared component with no ESP-IDF calls (`cap_text.c`, `cap_time.c`, `cap_note.c`, `cap_gemini.c`, `cap_github.c`; contract in `include/capture.h`), so `scripts/fw-test.sh` covers all of it with ASan and UBSan. HTTP goes through a function the app supplies (`cap_http_fn`); the gate and retry state are plain data so they survive deep sleep.
  - **Own JSON reader and calendar maths:** IDF v6.1 ships no `json` component (HARDWARE_LESSONS), and the reader is small: strict RFC 8259, depth limit 32, duplicate keys last wins (as kotlinx), a lone surrogate or `\u0000` becomes U+FFFD. Only JSON strings count for title/summary/transcript/status, and only integers for token counts.
  - **Note file:** rendering is byte-exact with `NoteFile.kt`; parsing is tolerant (hand edits, CRLF, BOM). Kotlin's `trim` is Unicode whitespace while Java's regex `\s` is ASCII; the C keeps both.
  - **Gemini request** is built as prefix + base64 audio + suffix, so the app can stream the recording from flash in blocks with a fixed Content-Length. The schema is re-serialised without whitespace, as kotlinx prints it.
  - **Differences from the phone, all deliberate:**
    - `source: clip`, and commit messages `clip: add <title>` / `clip: add to <title>` / `clip: next number N` (the one-line title as the file has it).
    - Reads are `GET contents/notes/<id>.md?ref=<branch>`; the clip reads no tree.
    - A 404 on the counter GET means "no counter yet"; a missing repo then shows as the PUT's 404 (terminal).
    - Create: 409/422 → GET the note; there = done (an earlier try whose answer was lost), absent = retry later. The content is not compared.
    - Update: PUT 404 → `CAP_GONE`; 409/422 → GET, merge like the laptop's `merge_note` (union by id, oldest first then id; title and summary made here win), up to 3 tries.
    - An addition with an id the note already has is a no-op (a repeated try).
    - "Typed note" is INFERRED as blank transcript and no `duration_ms`, since the clip can't see the phone's flag.
    - HTTP code 0 gives plain "Network error" (retry). Retry delay uses integer maths (may differ from Kotlin by 1 ms). Error messages are cut at 200 code points, and at about 250 bytes, so a message in a non-Latin script is shorter.
    - Base64 decode skips anything outside the alphabet and stops at `=`; a dangling single character is unreadable.
    - `cap_backoff_ms`: 30 s doubling up to 5 h, standing in for WorkManager.
  - **D.3's policy:** a hold whose last note is gone from GitHub (`CAP_GONE`) becomes a new note.
  - **Prompts:** `prompts/system_prompt.txt`, `system_prompt_append.txt` and `response_schema.json` are copies of Capture's `res/raw`. The owner allowed publishing them (2026-09-30: "I truly don't care about the prompts texts ... as long as it doesn't contain passwords"). A local-only test fails when they or the `CAP_GEMINI_*` values drift from `~/Projects/capture`.
  - **Tests:** `test_capture_note.c` (53: every `NoteFileTest.kt` and `AdditionsTest.kt` case, 7 synthetic golden files in `firmware/test/fixtures/capture/`, and a local-only round trip of the real files in `~/Ideas/notes`, which prints names and offsets only), `test_capture_gemini.c` (40: `GeminiTest.kt`, `RateGateTest.kt`, exact request strings, quota reset across both DST changes), `test_capture_github.c` (25: `GitHubTest.kt`, and every loop against a scripted mock server). Written by three subagents in parallel; they found one bug (`cap_iso_parse` accepted offsets past ±18:00). The GitHub test was also run against five deliberately broken copies and caught each.
  - **Not verified:** out-of-memory paths (no fault injection); the expected request JSON comes from reading `Gemini.kt`, not from running it (Phase D.5's real request settles that); kotlinx accepting token counts as quoted strings; the merge against a run of `capture-notes` itself.
- **Parallel work (owner's request, 2026-09-30):** "parallelism sounds like a good idea and I'm not really sure why we haven't done that ... change the PCB skill to say that we should definitely use parallelism and sub agents whenever suitable and possible". Added to the pcb-pipeline skill and `docs/workflow.md` ("Working in parallel"): firmware and the circuit/layout/case rounds are parallel tracks once the spec exists; independent work goes to subagents; money, quota, one board's gates and owner questions stay serial. For capture-clip: Phase E's rounds start alongside D.3; the order still waits for D.5.
- **Phase E.1 (capture-clip circuit), built 2026-09-30 (Claude's choices): `boards/capture-clip` is a crate; ERC 0 violations, NETLIST 34/34, NETCLASSES and BOARD.TOML pass (usb 467.5 of 500 mA; battery 363.9 of 400 mA; asleep 18.4 of 20 µA).** No layout yet, so `check`'s DRC and `gates.json` don't exist for it.
  - **Charge current 100 mA (RPROG 10 kΩ), not the plan's 300 mA.** The power path has no input-current limit, so on USB the charge current adds to the board's own: 355 mA Wi-Fi peak + 300 mA is over the 500 mA a USB port gives. A 400 mAh cell then fills in about 5 hours (INFERRED). Revert: `R3` in `circuit.rs` and the load in `board.toml`.
  - **No fuse** (unlike the starter): the charger and LDO limit themselves and a PTC would be an eighth Extended part; the TVS sits directly on VBUS. For the blind reviewer to weigh.
  - **R18, 0 Ω between GPIO6 and the mic's VDD:** a rework point for "mic powered from a pin", and it lets ERC pass without a waiver.
  - 0805 passives throughout (known LCSC numbers and 3D models); RT9080 EN tied to VSYS; spare GPIO7/21 and UART0 on test points; the charge light is red from VBUS through 1 kΩ into CHRG.
  - **Sleep budget has 1.6 µA of margin** and two INFERRED lines (Schottky leakage 3 µA, divider 1.4 µA); the module's 8 µA is a typical figure. Measured at bring-up.
  - **`[provision]` keys:** `wifi_ssid`, `wifi_pass`, `notes_repo` (prompted), `gemini_api_key`, `github_token` (file references; the token `expires = "2027-09-18"`). The firmware uses these NVS names.
  - **UNVERIFIED, for the datasheet check (E.2):** J2 polarity (pad 1 = +), LED pad 4 = anode, AO3401A pad numbers, the ICS-43434 land pattern, RB160M-30 leakage at 4 V, the TP4057/LDO capacitor values, the SMF5.0A's 9.2 V clamp against the RT9080's 6.5 V limit.
  - **Open for layout/case:** the mic is bottom-port, so its sound hole is on the board's battery side: the case needs a tray-side opening outside the cell's footprint (plan E.3's chimney).
- **Right the first time (owner's request, 2026-09-30):** "we expect it to work on the first try because if we make any mistake it will cost money for another shipment ... what is a good workflow to make sure we've done everything correctly once and for all in a single order". Added to the pcb-pipeline skill and `docs/workflow.md`: risks sorted by fix cost, hardware hooks for later wishes, Wi-Fi update with rollback in Rev A (this replaces "OTA: USB only for Rev A" above), the board reporting on itself, a readiness page before freeze, independent reviewers. **To build:** the readiness page in pcbgen's `review` stage (from `board.toml` requirement coverage plus a proof field), and the red-team agent's brief.
  - **capture-clip consequences:** battery level, firmware version and queue length go to a status file in the notes repo after each upload (owner: "battery life we wanted to be able to somehow send or ping its battery percentage"); the firmware checks for an update while on USB power and Wi-Fi; charge current is switchable by firmware (`CHG_FAST` on GPIO7: 100 mA by default, about 300 mA when Wi-Fi is off; owner: "wifi send is just a second, why not make it dynamic?"; INFERRED to suit the TP4057 until the datasheet check confirms).
- **Phase D.3, first slice (the clip app's logic), built 2026-09-30 (Claude's choices): `firmware/components/clip`, pure C, 86 laptop tests.** The device glue (LittleFS, recorder, HTTP, pins, sleep) is the next slice; `clip.h` lists the interfaces it must implement.
  - **State machine** (`clip_sm.c`): plain data for RTC memory; the mic starts at button-down, release before 1 s = new note, 1 s held = addition; recording wins over an upload in flight; never sleeps on USB; CHG_FAST is on only with USB present, Wi-Fi off and awake (a 200,000-event random walk checks it is never on with the radio); an update-check hook fires once per USB session when Wi-Fi is already up.
  - **Queue** (`clip_queue.c`): `<id>.ogg`, `<id>.meta` (key=value lines with a CRC-32), `<id>.ans` once Gemini answered, and `last`; small files are written as `.new` then renamed. Recovery at boot: a recording cut by power loss is queued up to its last complete Ogg page. Tests cut the file system at every operation and fail every GitHub request; each run ends with exactly one note or one addition.
  - **Policies (INFERRED as right for the owner, easy to change in firmware):** "last note" is the last note recorded here, set when queued, so a hold works offline (differs from D-024's "uploaded"); a gone target becomes a new note; a network error at Gemini uses no attempt; GitHub failures never give up (terminal ones wait 5 h and blink red); a Gemini give-up keeps the audio as "failed"; nothing heard in an addition is dropped with a red blink; 50 mV battery hysteresis, limits ignored on USB.
  - **Status file:** `devices/clip.json` in the notes repo after each saved note (battery percent and mV, USB, charging, queued, firmware). **Unchecked: whether the phone and laptop tolerate a `devices/` directory; check `~/Projects/capture` before turning it on.** It adds one commit per note. The percent curve is INFERRED.
  - **Not done:** Ogg page CRCs aren't checked in the scan; no storage-full policy; amber may need PWM on green.
- **Datasheet check (E.2), 2026-09-30:** `research/2026-09-30-datasheet-check-capture-clip.md`, 33 rows verified. J2 polarity verified (pad 1 = +). Fixes: R6 100k → 10k; D2's leakage is ~6 µA typical, not 3 (asleep total then just over 20 µA: a lower-leakage diode is being chosen, `research/2026-09-30-power-path-fix.md`); no internal pull-up on CHRG/IO4 ever; the RT9080's 6.5 V limit against the TVS's 9.2 V clamp is open (same research). Fast charge is ~333 mA; CHG_FAST is open-drain only, no pulls.
- **Power-path fix (Claude's choice, 2026-09-30; `research/2026-09-30-power-path-fix.md`), applied in `circuit.rs` and `board.toml`:**
  - D2 → ROHM RB168MM-40TR (C509936, SOD-123FL): ≤ 0.55 µA reverse leakage against the RB160M-30's ~6 µA. No Basic or Preferred Schottky rated ≥ 0.5 A leaks under 20 µA.
  - U4 → HEERMICR HE9073A33M5R (C723789, SOT-23-5): input absolute maximum 9 V, above what the TVS's 9.2 V clamp leaves after D2; 0.3 µA typical. C4 → 10 µF. R6 stays 10k.
  - Asleep total 12.7 of 20 µA (was 21.4, over budget). USB 467.9 of 500 mA, battery 363.9 of 400 mA.
  - **Costs, for the round's cost summary:** one more Extended part (+$3.07 per assembly order). The new regulator drops more, so Wi-Fi uploads need the cell at about 3.5–3.55 V under load (INFERRED from a typical figure); below that the clip records only. About 5–10 % of the cell's charge becomes record-only (INFERRED).
  - **Weak points, for the readiness page:** HE9073 pin numbers INFERRED (its figure has a dot, no numbers); its current limit has no guaranteed minimum (550 mA typical against a 355 mA peak); small vendor with a thin datasheet; margin at a full surge is 0.1 V. At bring-up: 3V3 during a Wi-Fi burst on USB and on a cell near 3.6 V, VSYS on hot-plug, sleep current at 3.3–3.5 V.
- **Status file `devices/clip.json` is on (Claude's choice, 2026-09-30).** A read-only check of the Capture project found that the phone and the laptop tool both accept only `notes/<id>.md` and ignore every other path (tests `treeListsOnlyNoteFiles`, `pathWhitelist`; its spec: "Other paths are ignored"). Side effects: each status commit makes the laptop's next sync a full fetch once, and a write landing at the same moment as another client's can get a 409, which every client retries. So the clip writes status when something changed or once a day, not with every note.
- **Phase E.3/E.4 (capture-clip layout and case), 2026-09-30 (Claude's choices; gates re-run by the main session):** 30 × 60 mm, one-sided, a 4 × 2.5 mm notch in the right edge for the battery lead (`BoardSpec::notches`, new in pcbgen). Every gate passes on the default 8 orders (kept order 3; ERC 0; DRC 0 open, 16 waived cosmetic silk warnings, 0 unconnected, parity 0; 0 unrouted; 72 vias).
  - **Case:** tray + lid + button cap, 33.6 × 63.6 × ~21 mm, 2 × M2 × 8; `fit.json` 30 of 30 checks pass (resin). The cell lies under the board, clear of the standoffs, USB-C legs, charger and microphone.
  - **Microphone opening in the tray:** the microphone is bottom-port, so `[[case.opening]] side = "bottom"` (new) puts a floor hole and a chimney up to the PCB under the footprint's unplated hole, checked by `opening.<ref>` and `seal.<ref>`, with three tests in `test_case.py` (one fit, two deliberate breaks). The shell follows the outline's convex hull so the notch stays open.
  - **Cost (live JLCPCB, 5 bare + 2 assembled):** $66.89 per design, $96.48 alone in a parcel; 8 Extended parts ($24.56 of it in fees). Thinnest stock: the microphone (C5656610), 1,271.
  - **Open, for the readiness page and the reviewers:** the ~12 mm sound path (PCB hole + chimney + floor) may resonate near 6–7 kHz (INFERRED; only a real board settles it); no dedicated TP4057 copper (GND pours on both layers; the chip throttles itself, so fast charge may run under 333 mA); GND pours are in 6 and 3 pieces; rotations UNVERIFIED for D1–D4, J2, MK1, SW1, SW2, U1; the boss-in-courtyard margin is 0.04 mm. Thickness (21 mm) and material are the owner's.
  - **Docs still to update:** `templates/board.toml`, `docs/pipeline.md` and the skill don't mention `side = "bottom"` or `Notch` yet.
- **Cost trade-offs are the owner's (owner's words, 2026-09-30):** "are we certain we're doing it optimally? perhaps there's a better dispatcher ... perhaps we can use strictly modules/parts that don't need this loading fee? even if we'll need to 'pay' for it with a smaller battery ... these are the kind of decisions I want to be left to me, not to Claude". So the fab, fee-bearing parts against fee-free substitutes, and cheaper-but-less options go on a choice page; the power-path fix's extra Extended part above is open until the owner picks. Research so far (JLCPCB side only; other fabs and the off-the-shelf route not done): `research/2026-09-30-cost-options-capture-clip.md`. To do: put this rule in the skill and `docs/workflow.md`.
- **Work in progress, stopped at the owner's request for a fresh session (2026-09-30), committed unverified except where said:**
  - Firmware app (D.3 slice 2 + D.4): `boards/capture-clip/firmware/`, state and next steps in its `PROGRESS.md`. Laptop tests 238 pass (re-run by the main session). QEMU scenarios press, hold, offline, power_loss, low_battery, selftest passed on their last runs; update (3 of 12) and rollback (7 of 10) are not repeatable, cause not found (one candidate is a real fault: the button loop runs below the worker's priority). Upload thresholds are now 3.65 V at rest / 3.55 V with Wi-Fi up, recording 3.4 V (INFERRED); `board.toml` notes still carry the old numbers.
  - Dev board: `firmware/spikes/codec/RESULTS-devboard.md`. Opus at complexity 0 or 1, 240 MHz: 0.27–0.31 of real time, worst frame 9–11 ms; 160 MHz works, 80 MHz overruns. Boot time, deep-sleep wake, OGG copy-back and the Wi-Fi scan were not run.
- **Owner-choice rule also in `CLAUDE.md`** (2026-09-30), so it loads every session, not only with the skill.
- **Independent reviews before round 1 (2026-09-30): not ready to order; fixes batched into round 2 with the owner's picks (Claude's choice).** Three fresh agents, none given the designer's reasoning:
  - **Datasheet check of the power-path swap** (`research/2026-09-30-datasheet-check-power-path.md`): nothing wrong. HE9073A33M5R pins 1 VIN, 2 GND, 3 CE, 4 NC, 5 VOUT are now VERIFIED (datasheet p.2 plus LCSC's numbered symbol), CE on VSYS; D2 pad 1 = cathode on VSYS. Dropout is ~0.45 V at 350 mA and ~0.77 V at 500 mA (typical, figure), not "500 mA at 0.5 V"; `board.toml` corrected. No guaranteed current limit; no hot leakage figure for D2.
  - **Blind review** (`research/2026-09-30-blind-review-capture-clip.md`): no pin, polarity or footprint error. Must: the cell's 25 mm lead probably doesn't reach J2 (path ~30–35 mm, INFERRED). Should: the module's ground hangs on one via; 0.3 mm power tracks over ~72 mm; D3 is last on VBUS; the cell's end lies under two USB shell legs; microphone stock 1,271.
  - **Red team** (`research/2026-09-30-red-team-capture-clip.md`, all INFERRED): the 3V3 rail at Wi-Fi start with ~32 µF behind the HE9073 (wants ≥ 100 µF); the button cap's 0.2 mm gap; pocket presses; dim light on a low cell; firmware: no loop breakers against crash/brown-out/held-button drain, unauthenticated updates, `update_url` not provisioned, a mount failure formats the queue.
  - **Round 2 layout list:** J2 next to the cell's lead end plus a lead-length check in the case stage; more fee-free 22 µF at the module and a pad pair for bulk; GND stitching at the module and a single-via check; VBUS past D3 first; wider battery-to-module tracks; battery pocket clear of the USB legs; cap gap 0.5 mm. Done together with whatever the owner picks (button, lights, case), since those move the layout too.
  - **Firmware list before the order:** loop breakers, signed updates and `update_url` in `[provision]`, no silent format of the queue, the measured wake time instead of `WAKE_BOOT_MS 150`.
- **Readiness data filled in (2026-09-30):** `boards/capture-clip/board.toml` has `[[requirement.proof]]` for R1–R11 and 21 `[[risk]]`s. Six are `new_board` and red until fixed or accepted (lead length, 3V3 step, single ground via, cell under USB legs, microphone stock, rotations). `board.toml` never carried the old 3.45 / 3.3 V thresholds (checked); the firmware's 3.65 / 3.55 / 3.4 V stand.
- **Quotes from other fabs and shops: not obtained (2026-09-30).** The owner approved uploads, sign-ups with their email and checkout walks ("1)ok 2)ok to use my email 3)ok"), but the `headless-browser` skill carries the owner's own read-only rule (no carts, no forms, no sign-in), so the agent stopped. No account, cart item or password file exists from either attempt. Open question for the owner: lift the rule for this job or drop those quotes.
- **Simulator crashes (owner: "i want them to not occur ... so omarchy won't see it as a crash"):** QEMU runs as a non-dumpable process everywhere (`scripts/nodump.sh`; HARDWARE_LESSONS), now also in `boards/capture-clip/firmware/sim/run.py`. The crashes themselves are QEMU's (two host threads in its translator and flash-mapping code during the first-boot format); they are counted per run, not hidden.
- **Not verified yet:** assembled boards and 3D prints in one JLC parcel (ask JLC before the first order); HTTPS from Wokwi to Gemini and GitHub; the Opus encoder's real CPU load on silicon (Phase D.5).

## D-024 First project: capture-clip, a battery voice-note button for Capture (DECIDED, brainstorm closed by the owner, 2026-09-29)
- **Owner's words:** "let's add another user: ESP32, which will be able to capture voice ideas after pressing a button"; then "Build B, add an LED indicator light … I don't want it ordered yet, but maybe some day when I have a few more projects ready to be ordered with it … battery … use it around the house, maybe take it with me … one button suffices … make difference between press and hold, so we can have two functions for it".
- **Spec:** `boards/capture-clip/spec.md` (R1–R11). The recording length wasn't answered, so it defaults to Capture's own 15-minute cap.
- **Decided by Claude:**
  - **The device is a full Capture client.**
    - It records, calls Gemini itself and writes `notes/<id>.md` to the GitHub notes repo through the REST API, like the phone (Capture SPEC §6, §9: same request, schema, note format, `next-number` counter).
    - Rejected: uploading raw audio for the laptop to process (it needs the laptop on, and a new inbox path in Capture); streaming to the laptop over the LAN (the same, plus pairing).
    - Cost: the device holds a Gemini key and a GitHub token, stored in encrypted NVS.
    - **Owner's call (2026-09-29): reuse Capture's own key and token** ("use same gemini key", "use also the same github token as capture if possible").
      - The setup step reads `gemini_api_key` and `github_token` from `~/.config/capture-notes/config` and writes them to the device over USB. Claude never prints or copies them elsewhere.
      - Checked without printing it: the token is fine-grained, expires 2027-09-18, and has push access to `vaknin/capture-notes`.
      - Its access to other repos was not checked; the permission system refused that as credential exploration.
      - Risk: a lost device leaks both. The owner revokes them on GitHub and in Google AI Studio, and re-keys the phone and laptop too, since they share them.
    - **Wokwi CI token** (simulation only, never on the device): `~/.config/wokwi/token` (mode 600), loaded as `WOKWI_CLI_TOKEN` per run.
  - **Press and hold:**
    - A press = a new note.
    - A hold ≥ 1 s = an addition to the last note this device uploaded (Capture SPEC §9 Additions).
    - The owner asked for two functions; this pair matches the app's two actions. Easy to change in firmware.
  - **Audio:**
    - OGG/Opus, 16 kHz mono, 32 kbps: the phone's exact format, already proven with Gemini (Capture SPEC §3), so no Capture-side change.
    - It is encoded on the ESP32-S3 (Espressif's audio codec component) and stored in a flash partition: 15 min ≈ 3.6 MB.
    - The N16R8 module's 16 MB leaves room for ≥ 30 min queued plus two OTA slots.
  - **Module:** ESP32-S3-WROOM-1-N16R8 again (verified pads, footprint and pipeline). The smaller MINI-1 would save ~5 mm but needs a new GPIO table and footprint check, and the battery sets the size anyway.
  - **The main button is on IO0.** It doubles as BOOT (hold while powering up = download mode), and IO0 is an RTC GPIO, so it wakes the chip from deep sleep. That saves a second button.
  - **Setup over USB.** Wi-Fi credentials, keys and the repo are pushed from the laptop over USB serial (a `capture-notes` subcommand later), so there is no on-device UI or captive portal.
  - **Note source:** the device writes `source: clip`.
    - VERIFIED 2026-09-29 that Capture carries any `source` value through unchanged: the phone's `sync/NoteFile.kt` reads it as a free string (default "phone") and writes it back as read; the laptop's `desktop/capture-notes` does the same (default "laptop"); nothing branches on it.
    - No Capture change is needed.
- **Not ordered;** it waits to share a parcel (D-021). Parts research: `research/2026-09-29-parts-capture-clip.md`.

## D-023 Workflow tooling built: board.toml gate, firmware pin header, cost and review stages, templates, draft/freeze tags (DECIDED, owner approved building it, 2026-09-29)
- **Owner's words:** "I like how you handled each, especially the .toml file. let's do that. write all of these suggestions, regarding your questions, go with your recommendation and best practices."
- **Where things live:** in the board's crate, not `projects/<name>/`: `boards/<name>/{board.toml, spec.md, round.md, errata-rev<X>.md, firmware/}`. One directory per board keeps the spec next to the code that must match it.
- **`board.toml`** (`crates/pcbgen/src/boardfile.rs`, serde with unknown keys rejected): `[board]` name/revision/module, `[[pin]]` signal → module pin, net, GPIO, direction, active-low, `[power]` budget and `[[power.load]]`s each with a source, `[[requirement]]` ID/text/`covered_by`, `[firmware] self_test`, optional `[order]` boards/assembled/budget_usd (default 5/2, D-021).
- **BOARD.TOML gate** (in `sch`, `check` and `fw`; fails the stage). It checks:
  - name and revision equal the circuit's; the module part exists and pcbgen has a GPIO table for it
  - signals unique and upper-case C identifiers; GPIOs and module pins unique
  - each pin exists on the module symbol (by name), is on the named net, and has the right GPIO: `IOnn` = nn; TXD0/RXD0 = 43/44 (VERIFIED, Espressif WROOM-1 datasheet v1.8 Table 3-1); USB_D−/D+ = 19/20 (VERIFIED, KiCad symbol alternates and HARDWARE_LESSONS)
  - every module GPIO the circuit connects is in the map
  - the loads fit the power budget (margin printed)
  - requirement IDs unique and matching `spec.md`'s both ways; each covered, each `part:`/`pin:`/`test:`/`gate:` reference real; each self-test covers a requirement
  - `order` quantities within JLCPCB Economic's 2–50
  - Only ESP32-S3-WROOM-1 has a GPIO table. Another module makes the gate fail with "add one", rather than guess.
- **Existing `Circuit::net_of(part, &Pin)` is used;** the planned `net_of(PinRef)` would have clashed and wasn't needed.
- **`fw` stage:** writes `firmware/board_pins.h` (ESP-IDF: `PIN_<SIGNAL> GPIO_NUM_<n>`, `_ACTIVE_LOW`, `BOARD_NAME`, `BOARD_REVISION`, `BOARD_SELF_TESTS`). Committed like `fab/`. Checked to compile with `-Wall -Wextra -Werror` against a stub `driver/gpio.h` (ESP-IDF itself isn't installed yet).
- **Cost: built now, `cost` stage** (`crates/pcbgen/src/cost.rs`), closing that open question for option A.
  - Why build rather than show "not computed": the owner sees cost against budget every round, and the API from `research/2026-09-29-cost-estimate.md` §1 still answers without a login (checked today).
  - It prices `fab/<name>-bom.csv` line by line, run through `curl` like kicad-cli (no HTTP crate).
  - Order quantity = placements for the assembled boards + attrition, at least the minimum (INFERRED model, from the cost estimate). Joints = pads of the assembled parts (INFERRED).
  - Fixed fees are the VERIFIED ones in HARDWARE_LESSONS. Shipping and VAT are shown separately, since they are per parcel (D-021).
  - Writes `fab/cost.json` (the fab stage clears `fab/`, so a cost can't outlive its BOM). Needs the network, so it runs only when named. The API is undocumented: a change shows as a fetch or parse error, not a wrong number.
- **`review` stage** (`crates/pcbgen/src/review.rs`): `<board>/review/index.html` (gitignored), one self-contained page for an Artifact publish.
  - Contents: top and bottom renders (kicad-cli SVG, embedded); the round (from git tags) and what changed (`round.md` plus commits since the previous draft tag); "things to check"; requirement coverage; cost against budget; the power budget; the checks; firmware simulation status.
  - "Things to check" is gathered automatically: UNVERIFIED rotations, estimated power figures, waived DRC items, short stock, over budget, stale reports, uncommitted changes, a missing `round.md`.
  - It reads what other stages wrote and re-runs nothing. `check` now writes `kicad/reports/gates.json` for it. Anything missing shows as "not run", and a board newer than the last check shows as stale.
- **Tags:** `scripts/draft.sh <board>` tags `<board>-draft-<n>` (clean tree). `scripts/freeze.sh <board>` tags `<board>-rev<rev>-freeze`, only on a commit that carries a draft tag, so the frozen version is one the owner saw. `scripts/check-frozen.sh <board>` runs before any order: the freeze tag exists and the board directory is unchanged since it. Tags are local; nothing is pushed.
- **Templates:** `templates/{spec.md, board.toml, round.md, errata.md}`.
- **Wokwi: documented, not set up.** It needs ESP-IDF, wokwi-cli and a token. Set up with the first firmware (`firmware/wokwi.toml`; the review page notices it).
- **Starter retrofit:** `boards/starter/{board.toml, spec.md, round.md, firmware/board_pins.h}`. 8 pins, 8 requirements, 4 self-tests, 461 of 500 mA. The power loads the datasheets don't give are marked INFERRED.

## D-022 Workflow: brainstorm first, spec as Markdown + TOML, simulate instead of prototyping, review rounds before ordering (DECIDED with the owner, 2026-09-29)
- **Owner's input:**
  - Brainstorming comes first and should last a while.
  - Firmware (ESP-IDF) comes before the PCB.
  - The owner has a dev board, but no time or energy to buy modules or solder.
  - "By iteration I meant before placing the order."
  - Wants efficiency and accuracy.
- **Decided:** `docs/workflow.md` (it refines the brief's phases 2–7; the brief stays verbatim):
  - a brainstorm phase that ends when the owner says so
  - `spec.md` plus `board.toml` as the single source of pins, power and requirement IDs, read by both the circuit code and the firmware
  - firmware in Wokwi simulation; the first assembled PCB is the prototype, built for rework
  - design rounds, each ending in a review page, tagged in git, until the owner says "freeze"
- **Why Markdown + TOML, not a custom language:**
  - Markdown is what the owner reads.
  - TOML is what code checks.
  - A new language would add a parser and its bugs without adding accuracy.
  - The accuracy comes from gates that fail when the spec, circuit and firmware disagree.
- **Built since:** the `board.toml` reader and gate, the firmware header, the cost and review stages, templates and tag scripts (D-023).

## D-021 Ordering several different boards per shipment: separate orders in one parcel (A) for now; a shared panel (B) documented (DECIDED by the owner, 2026-09-29)
- **The need:** 1–2 copies each of about 5 different boards per shipment, never 5 copies of one board.
- **Name clash:** these A/B options are about *how orders are grouped*. The A/B/C table in `docs/brief.md` is about *who assembles* (fab / mixed / owner); they are separate choices.
- **Owner's words:** "option a is quite expensive. document both, but for now implement/decide A."
- **Facts:** from JLCPCB help pages read 2026-09-29 (VERIFIED unless marked).
- **Option A: each design its own order, shipped together (DECIDED for now)**
  - Each order has 5 bare PCBs, 2 of them assembled.
    - Economic PCBA takes 2–50 boards (VERIFIED, jlcpcb.com/capabilities/pcb-assembly-capabilities).
    - 5 bare with 2 assembled is allowed; the blank ones may come back with solder on them (VERIFIED, jlcpcb.com/help/article/pcb-assembly-faqs-part-2).
    - That 5 is the smallest bare quantity is NOT confirmed.
  - Orders can share a cart (jlcpcb.com/help/article/How-to-order-multiple-different-PCBs-together). "Combine Shipping" holds finished orders and ships them as one parcel, by weight, count or a weekly day; storage is free for the first 15 days (VERIFIED, jlcpcb.com/help/article/combine-shipping-service). So the designs don't have to be finished at the same time.
  - **Paid per design:** PCB $4, setup $8.18, stencil $1.53 (VERIFIED, jlcpcb.com/help/article/pcb-assembly-price), $3.07 per unique Extended part, and the parts for 2 boards.
    - That setup and stencil are charged per order is INFERRED.
    - Extended fees are paid again in every order, e.g. once more for the ESP32 module in each design.
  - **Paid once per parcel:** shipping, ~$30 FedEx, more with weight.
  - **Cost (INFERRED, `research/2026-09-29-cost-estimate.md` §5):** about $72 per design for the starter; $52–70 for a typical small design, VAT included; about $260–350 for five. The parcel is always over the $75 VAT line.
  - **Ways to cut it:**
    - Use Basic parts instead of Extended where possible.
    - Keep a recurring module in "My Parts Lib" by pre-ordering it. Pre-ordered, global-sourcing and consigned parts are stored free and used first in later orders (VERIFIED, jlcpcb.com/help/article/smt-reorder-process-overview). Leftovers of ordinary library parts are discarded (VERIFIED, pcb-assembly-faqs-part-2).
    - Whether pre-ordering avoids the repeated Extended fee is UNKNOWN; check it before the first real shipment.
  - No pipeline work is needed: each board already produces its own fab package.
- **Option B: several designs on one assembled panel (documented; not now)**
  - Up to 10 designs per board (VERIFIED, jlcpcb.com/help/article/pcb-panelization). A PCBA panel with more than one design adds $8.21 (VERIFIED, pcb-assembly-price). Mixed panels can be assembled (VERIFIED, jlcpcb.com/help/article/in-what-cases-will-there-be-charged-extra).
  - Economic PCBA doesn't allow V-cut, so boards are joined by tabs or mouse-bites.
  - The bare-PCB fee per extra design is NOT confirmed; the quote page shows it only once a count is picked.
  - **Saves:** one setup and one stencil for all designs, and each Extended part's fee once instead of once per design. Roughly $40–70 per shipment (INFERRED, not priced).
  - **Costs:**
    - Every design must be ready at the same time.
    - They must share thickness, colour and finish.
    - The owner breaks them apart, which leaves rough edges.
    - A mistake in one design can mean redoing the whole panel.
    - pcbgen would need a new panelization step.
  - **Revisit when:** the owner often finishes several designs at once, or A's cost starts to bite.
- **Nothing is ordered.** Any real order still needs a summary and the owner's OK.

## D-020 pcbgen: when routing fails, try more on its own, then report what failed and why; a failure log per board (DECIDED, owner approved the scope, 2026-09-29)
- **Owner's words:** "I agree, let's implement this error pipeline." It lives in the route stage, not in a Claude Code hook, because the pipeline is what detects the failure.
- **Escalation, cheapest first:** it runs only when none of the best `drc_checks` orders is clean. Code: `route::next_step`.
  1. DRC-check every other order already routed (~15 s each, `max(parallel, drc_checks)` at once).
  2. Route `RouteOptions::extra_rounds` rounds (default 1) of `extra_tries` new orders (default 8, seeds after the last), and check them.
  3. Stop. Keep the best-ranked clean order, else the one with the fewest open items. Deterministic: seeded orders, ties go to the lower order.
  - `tries.json` records the steps taken.
- **Failure report:** when nothing is clean, `route/failure.json` is written and printed. For each distinct open DRC item across the checked orders it gives:
  - its type, severity and the parts involved
  - its position in layout mm (from the board's top-left, as `layout.rs` places parts)
  - how many checked orders hit it: all of them means placement or rules; some of them means routing luck
  - a suggested fix
  - Items are grouped across orders by type plus items, with track lengths taken out.
  - The fixes are a small table in `crates/pcbgen/src/failure.rs`, with "move the parts involved" as the fallback.
- **Failure log:** `<board>/route-failures.jsonl` is committed. It has one line per failed order: date, board, a layout fingerprint (hash of the DSN), the order, and each error's type and parts.
  - A re-run of an unchanged board adds nothing.
  - The stage prints the board's error types by frequency.
  - It also reads the sibling boards' logs and prints `RULE (D-020)` for any error type that has failed on two or more boards.
  - Orders that failed but were rescued by escalation are not logged; `tries.json` has them.
- **Rule:** when the same error type fails on two boards, turn it into a prevention rule the router gets (a DSN constraint, opt-in in `RouteOptions`/the layout) and add a `HARDWARE_LESSONS.md` entry.
- **First prevention rule: `RouteOptions::pad_rings`** (`PadRing::new("J1", "SH")`, margin 1.0 mm).
  - It is a no-tracks, no-vias keep-out in the DSN: the pad's rectangle grown by the margin, on the pad's copper layers. The board and its DRC don't see it.
  - It refuses a pad that isn't on a poured net, since the ring would leave that pad unroutable.
  - **Verified on D-018's board, J1's 4 shield pads ringed, all 8 orders checked:** `starved_thermal` in 0 of 8 orders (1 of 8 without rings; 2 of 16 in the forced-failure run). Every gate passed. The rings in the DSN match the pads (1.0 × 2.1 mm pad → 3.0 × 4.1 mm ring).
  - **Cost:** it blocks routing channels near the connector. 5 of 8 orders left a signal unrouted (2 of 8 without), and each Freerouting run took ~145 s instead of ~75 s. So it stays opt-in, for a board that hits the error; it is not a default.
- **Tests:**
  - 6 new unit tests: the ladder's decisions, choosing the kept order by global rank, the worker pool's order, reading parts/pads/fixes from a sample DRC JSON, grouping across orders, and the log's dedupe, tally and two-board rule. 32 in all.
  - Scratch runs on D-018's board (R5 and the SHT40 on the back):
    - `drc_checks: 1`: order 5 failed (2 `starved_thermal`); escalation checked the other 7 and kept order 6; all gates pass.
    - **No waivers (forced failure):** 3 + 5 checked, 8 more routed and checked, 16 in all.
      - The report shows the 9 silkscreen items in 16 of 16 orders as "placement or rules".
      - `starved_thermal` on J1 SH (2 of 16) and the dangling or unfinished SHT40 stubs (1–4 of 16) show as "routing luck".
      - 16 log lines were written.
- **Unchanged starter:** kept order 2 (3 checked, all clean, no escalation).
  - Byte-identical `board.ses`, schematic, BOM, CPL and fab README.
  - The DSN differs only in its own path on line 1.
  - `routing.json` is identical apart from `router`; the board is identical apart from UUIDs.
- `gates::drc_open` now returns KiCad's items rather than text; the check stage's printout is unchanged.

## D-019 pcbgen: the route stage checks its best candidates with KiCad's DRC (DECIDED, technical, 2026-09-29)
- **Why:** the router's own numbers (unrouted, thin track, vias, length) can't see some faults. On D-018's scratch board, the order ranked best ran a track through the ground pour's spokes to the USB-C shell pads, and the full check failed with 2 `starved_thermal` errors. A different order would have passed.
- **What:** after Freerouting has routed every order, the best `RouteOptions::drc_checks` of them (default 3) are each finished the way the kept one always was: tracks and vias, zone fill, stitching, fill. Each runs in its own copy of the project (`route/try-<n>/check/`) and gets the same DRC gate as the check stage: fab rules, schematic parity, the board's waivers.
  - The kept order is the best-ranked one with no open DRC item. If none is clean, it is the one with the fewest open items, and the stage says the check will fail.
  - Every checked order's open count goes into `route/tries.json` (`drc_open`). `drc_checks: 0` keeps the old behaviour.
- **Cost:** about 10 s per run (the three checks run at once), on a ~2.5 min run.
- **Result:** on D-018's scratch board (R5 and the SHT40 on the back), order 5 was dropped (2 open), order 6 kept (0 open), and every gate passed.
  - On the unchanged starter, the kept order is the same as before (order 2), so the committed board stays as it is.
- The check stage still runs its own DRC; this only chooses better among routes that already exist. It adds no new rule.
- Stitching now returns its "no room for a via" notes instead of printing them, so only the kept board's notes are shown.

## D-018 pcbgen: bottom-side parts; Freerouting's "violations" explained (DECIDED, technical, 2026-09-29)
- **Bottom-side parts work end to end.** In a layout, `at_bottom(x, y, rot)` places a part on the back. `rot` is the angle KiCad shows for the flipped part, still CCW as seen from the top. It means: flip the footprint left-right at angle 0 (pcbnew's flip), then turn it to `rot`.
- **Board file:** `footprint::flip` rewrites the library footprint the way pcbnew saves a flipped one:
  - Y negated in the footprint's frame; F.* ↔ B.* layers
  - pad angles negated, trapezoid dy negated, chamfer corners swapped
  - KiCad's rule for text angles, justification and mirroring (rules in `HARDWARE_LESSONS.md`)
  - The part is then placed as before.
  - **Checked:** 23 footprints, 11 of them different library parts plus two synthetic test footprints covering every item type and text case, were placed by pcbgen and compared item by item with a board pcbnew saved after `Flip`. All match, except KiCad re-saving a 30°-turned rectangle as a polygon with its own point order (same corners).
  - Hidden symbol fields on a bottom part go on B.SilkS, turned 180° and mirrored. That is inferred from the flip rule, not checked against pcbnew's netlist update.
- **DSN:** a bottom part is written as KiCad's exporter writes it: the image of the top-side footprint it came from, placed `back` at its angle + 180, so top and bottom copies share an image.
  - **Checked against `pcbnew.ExportSpecctraDSN`** on the same 23 parts: placements, pin positions, rotations and shapes, and keep-outs all match.
  - Duplicate-numbered pins (`SH@1`…) get their `@n` suffixes in a different order on back parts: same shapes, internal names only.
  - **Checked in Freerouting too:** its own reader puts every pin of the bottom parts where the board file has them, on the back layer, and the SHT40's keep-out on the back.
- **Rest of the pipeline:**
  - Escape stubs go on the part's own copper side, against keep-outs on that side.
  - The routing report counts a track in a keep-out only on the keep-out's layers (a via on any layer), and names the footprint.
  - Stitching already avoided courtyards and pads on both sides.
  - **CPL:** bottom rotation = 180 − KiCad's angle + the package correction, position as seen from the top. That is kicad-jlcpcb-tools' rule (its `fabrication.py`), not JLCPCB's own documentation. Every bottom part is listed UNVERIFIED in `fab/README.md` for the placement-preview check.
  - Owner note for later: bottom-side assembly at JLCPCB costs extra (a second side). Nothing uses it yet.
- **Found on the way:** pads whose copper is offset from their hole (`(drill ... (offset x y))`) were written into the DSN centred on the hole. They are now shifted, and named `Oval[A][dx,dy]Pad_...` as KiCad names them. `Pad::dist` (stitching) uses the offset too. No starter part has one.
- **Also:** image keep-outs are rounded to 1 nm in the footprint's frame, so a rotated part's image carries no sub-nm noise.
- **Scratch test:** a copy of the starter board with R5 and the SHT40 (U4) moved to the back, full pipeline into a scratch directory.
  - Passed: ERC 0, schematic parity 0, 0 unrouted, no copper in keep-outs, 11 waived silk warnings as on the starter, CPL as expected. The render shows both parts on the back with mirrored fab text, the keep-out and escape stubs in place.
  - DRC failed on one item: 2 `starved_thermal` errors on the USB-C shield pads. The kept route ran a track through their ground-pour spokes. That is a routing outcome near J1, not a bottom-side placement fault, and the gate caught it.
  - The route stage's score didn't look at DRC; D-019 fixes that, and the same board then passes every gate.
- **Unchanged starter:** a full scratch run gave a byte-identical schematic, BOM, CPL, positions, every DSN body and the session file, and an identical routing report. The committed board is not re-routed. `fab/README.md` changes only in a heading ("…, or bottom side").
- **Freerouting's ~20 "clearance violations" (open since D-017): explained, harmless.** `scripts/fr-violations/run.sh <dsn>` loads the DSN with Freerouting's own reader (`DsnReader.readBoard`) and prints `Item.clearanceViolations()` with both items and their places. On the starter's DSN they are:
  - 12: the ESP32 EPAD's thermal holes overlapping the pad, same net
  - 4: the HRO USB-C footprint's stacked pad pairs, same net, same place
  - 4: the locked SHT40 escape stubs inside the sensor's keep-out notch (D-010)
  - They are all by design: present before routing, and never changed by it. KiCad's DRC rightly sees none. Nothing in the DSN is wrong.
  - The probe runs on Freerouting's Java 25 runtime through a copy of its launcher, because javac 21 can't read Java 25 class files; hence the reflection.
- **Tests:** 25 unit tests (new: `flip` against pcbnew's saved values, a bottom part read back through `board::Footprint`, the DSN of a top and a bottom copy, drill-offset padstacks, CPL rotations).

## D-017 pcbgen: best-of-N routing with fanout off, pad shapes, tests (DECIDED, technical, 2026-09-29)
- **Thin tracks had one cause: Freerouting's fanout stage.** Its code (read from the 2.4.1 jar with `javap`, `FoundConnectionInserter.insertFanoutMicroNeckdown`) narrows a fanout track to 3/4 or 3/5 of its class width: that is where the 0.225 mm (of 0.3) and 0.15 mm (of 0.2) tracks came from. Its `automatic_neckdown` setting changed nothing (identical session file). With fanout off, all 8 footprint orders of the starter board routed with **no** track under its class width (40–49 mm with fanout on) and 19–27 vias instead of 23–31. `RouteOptions::fanout` is now `false` by default.
- **Best of N orders, not first success.** The route stage routes 8 footprint orders (`RouteOptions::tries`), 4 at a time (`parallel`), and keeps the best: fewest unrouted connections on other nets, then on the pour net, then least track under its class width (0.1 mm steps), then fewest vias, then shortest (0.1 mm steps); ties go to the lower order. Each order works in `kicad/route/try-<n>/`; the winner's `board.dsn`/`.ses`/log are copied to `kicad/route/`, and every order's score goes to `route/tries.json` and into `reports/routing.json` as `router`. Order 0 is the board's own footprint order; the others are seeded shuffles, so a run is reproducible.
- **Cost: time.** Freerouting takes ~21 s alone on this laptop (Ryzen 7 5700U, 8 cores); 4 at once take ~54 s each (memory-bound: its thread settings and JVM GC options changed nothing, and results are identical whatever the thread count). A full starter run is now ~2.5 min (was ~35 s). `--tries 1` on the command line gives a quick single route while iterating on placement.
- **Starter result:** order 2 of 8 kept: 0 unrouted, 20 routed vias (was 30), 1.6 mm under class width (only the locked 0.2 mm escape stubs on +3V3/GND; was 45.9 mm). All gates pass. The GND pour on top is now in 3 pieces, 50% of its outline (was 2 pieces, 61%), because more track runs on top; every piece is stitched and DRC shows 0 unconnected. The design (parts, nets, placement, rules) is unchanged; BOM and CPL byte-identical.
- **Via cost flag fixed:** Freerouting 2.4.1's setting is `router.scoring.viaCosts` (field names read from its settings classes); `router.via_costs` was ignored. Same value (50) as before.
- **Freerouting reports ~20 "clearance violations" on every starter route** that KiCad's DRC doesn't see. 4 come from the locked SHT40 escape stubs; the rest are probably overlaps its own model counts (e.g. keep-outs over pads). KiCad's DRC is the gate; the count is kept in `tries.json` for comparison. (Cause found in D-018: all by design.)
- **Pads:** trapezoid pads (KiCad's `rect_delta` corners) and custom pads (convex hull of anchor and primitives, strokes included) now go into the DSN as polygons; stitching's pad distance uses the same outline. Freerouting itself takes a padstack polygon's convex hull (read from its `Library` parser), so the hull loses nothing. Unnumbered pads are named `@1`, `@2`, ... as KiCad does; the first one used to be written with an empty name, which broke the DSN. Checked against KiCad 10's own DSN export (pcbnew's `ExportSpecctraDSN`, still installed with KiCad 10; used only as a reference) on 11 library footprints: all 104 pin shapes present, KiCad's copper at most 0.4 µm outside ours, ours at most 10 µm larger. Chamfered roundrects stay rounded (covers more copper; conservative). Bottom-side parts are still refused.
- **Netlist exported once per run** (the netlist and net-class gates and the pcb stage each used to export it).
- **Tests:** 21 unit tests without KiCad (DSN naming, quoting, shapes and conventions; SES reading; stitch grid origin; log parsing and score order; glob; pad distances; convex hull) and one opt-in end-to-end test (`cargo test --release -p starter -- --ignored`, needs kicad-cli and Freerouting).
- **A board crate outside the repo works:** a throwaway crate in a scratch directory with `pcbgen = { path = "/home/kivan/Projects/PCB/crates/pcbgen" }` ran the full pipeline (see the skill).
- **Not changed:** `schematic::today()` still runs `date +%F`: the title-block date is the local date, and `date` knows the time zone without a new dependency.

## D-016 pcbgen ported to Rust, SWIG removed (the KiCad 11 port) (DECIDED, technical, 2026-09-29)
- **Why now:** KiCad 11 removes the SWIG `pcbnew` module, and Arch upgrades KiCad on a normal `pacman -Syu`. Python is in the stack only because of SWIG, and the owner's rule for long-lived tools is Rust. Doing it before Phase 2 means no second board is written against the Python API. It costs no money.
- **Target:** a Rust `pcbgen` that writes `.kicad_sch` and `.kicad_pcb` itself as S-expressions and never loads KiCad as a library. kicad-cli stays for what it does well: netlist export, ERC, DRC, zone fill (`drc --refill-zones --save-board`), gerbers, drill, positions, and `sch upgrade` / `pcb upgrade --force` so KiCad itself re-saves every file we write.
- **Toolchain:** Rust 1.98.1 (current stable, 2026-09-01), pinned in the repo's `mise.toml`. Crates, each at its latest stable version (checked 2026-09-29): `anyhow` 1.0.104, `serde` 1.0.229, `serde_json` 1.0.151 (with `preserve_order`, so `.kicad_pro` keeps KiCad's key order), `uuid` 1.26.1 (`v5`), `regex` 1.13.1, `csv` 1.4.0, `zip` 8.6.0 (9.0 is still a pre-release). No geometry, S-expression or KiCad crate: none is 1.0 and maintained, and what is needed here is small.
- **Crate layout:** a Cargo workspace at the repo root.
  - `crates/pcbgen`: the library. Modules: `sexpr` (reader/writer), `symlib` (`.kicad_sym`, flattening `extends`), `circuit`, `schematic`, `project` (`.kicad_pro`, `.kicad_dru`, lib tables), `footprint` (`.kicad_mod` loading and placement), `pcb` (the board writer), `board` (a typed read-only view of any saved `.kicad_pcb`), `geom`, `dsn`, `ses`, `route` (Freerouting and stitching), `gates`, `report`, `fab`, `cli`.
  - `boards/<name>`: one small binary crate per board: `src/circuit.rs`, `src/layout.rs`, and a `main.rs` that hands both to `pcbgen::cli`. Run with `cargo run --release -p <name> -- [sch] [pcb] [route] [check] [fab]`.
- **Board format: Rust, circuit and layout both.** A circuit needs code (helper functions for R/C, loops over test points), and layout coordinates are computed from each other (`W / 2`, `H - 3.7`) with a comment on nearly every line. TOML would lose both. Rust also type-checks the board: a misspelled field or a missing placement fails to compile or fails at the line that wrote it (`#[track_caller]` panics for authoring errors such as an unknown pin).
- **S-expressions:** a ~200-line reader/writer. Unquoted atoms keep their exact text (numbers are never re-rounded on a round trip); quoted strings are unescaped and re-escaped (`\"`, `\\`, `\n`).
- **Schematic:** a line-by-line port of `schematic.py`, with the same UUIDs (uuid5 over the same keys), the same layout arithmetic (Python's `round()` is half-to-even, so Rust uses `round_ties_even`) and the same KiCad 9 output followed by `kicad-cli sch upgrade`. Check: the upgraded `.kicad_sch` and KiCad's exported netlist must be identical to the Python ones apart from the date and file paths.
- **Footprints:** read from the project's `fp-lib-table` (`/usr/share/kicad/footprints`, `lib/footprints/*.pretty`). Placing one in a board follows KiCad's file conventions (read from the Python-made board): pad and graphic positions stay in the footprint's own frame, pad and text angles get the footprint's rotation added, and zones inside a footprint (keep-outs) are stored in board coordinates. Every UUID inside is replaced by a deterministic one. Bottom-side placement is refused with a clear error until a board needs it.
- **Board writer:** builds the `.kicad_pcb` tree (footprints with nets and schematic paths from kicad-cli's netlist, outline, pours, `CopperZone`s, keep-outs, texts, locked escape stubs, aux/grid origin), writes it, then runs `kicad-cli pcb upgrade --force` so KiCad parses and re-saves it at once. Check: DRC before routing gives parity 0 and only the expected unrouted items and dangling stubs, and the render looks the same as the Python board.
- **Routing:** the route stage deletes unrouted tracks and vias from the saved board (locked stubs stay), writes the Specctra DSN itself from the saved board (KiCad's own DSN conventions: µm, Y up, `(resolution um 10)`, `Via[0-1]_<dia>:<drill>_um` padstacks, one image per footprint with its keep-outs, NPTH holes as keep-outs of drill + 2 × hole clearance, classes from the `.kicad_pro` net-class patterns, locked tracks as `(type fix)` wires). Pours are never written, as before (D-012). Freerouting gets the same arguments as before. The SES reader adds its wires and vias to the board. Check: the DSN's classes, keep-outs, pins and padstacks match the Python DSN.
- **Stitching:** reads the filled zones from the board kicad-cli has just filled (`filled_polygon`) and applies the same rules as `route.stitch`: 3 mm grid, then one via per isolated piece, then `stitch_local`; a via must sit fully inside the fill on both layers (point-in-polygon plus distance to the polygon's edges ≥ the via radius + 0.05 mm, instead of SWIG's `Deflate`), outside courtyards (courtyard-fallback: ≥ 0.2 mm from every pad), ≥ 0.5 mm hole to hole and ≥ 0.25 mm from other-net tracks.
- **Order:** the Python stays working until the Rust pipeline reproduces the starter board and every gate passes on the same reports; only then do the docs, the skill and the lessons switch, and the Python is deleted.
- **Result (same day): done, Python removed.** Checked stage by stage against the Python pipeline on the starter board:
  - `sch`: `.kicad_sch`, `.kicad_pro`, lib tables and `.kicad_dru` byte-identical; KiCad's netlist identical apart from path and timestamp; ERC 0.
  - `pcb`: same footprints, pads, nets, zones, texts and escape stubs (only a 1 nm difference on three outline-arc midpoints). DRC before routing gave the same counts on both (parity 0, 47 unrouted, 3 dangling stubs, the 11 known silk warnings). The render matches.
  - DSN: same boundary, vias, rules, keep-outs, every pin's position, rotation and shape (roundrect pads as polygons at most 1 µm outside the copper, as KiCad's), nets, classes and locked stubs.
  - Full run in place: netlist and net classes PASS, ERC 0, DRC 0 open (the same 11 waived silk warnings; 0 unconnected, parity 0), 0 unrouted, no copper in keep-outs, 20 +3V3 cooling vias, smallest drill 0.3 mm. BOM, CPL and positions byte-identical to the Python ones.
  - The tracks differ: a new router run, which happened on every Python run too. The design (parts, nets, placement, rules) is unchanged.
- **Found on the way: Freerouting is deterministic.** The same DSN routed identically on repeated runs (both DSNs, twice each). The Python pipeline's run-to-run variation came from pcbnew's random footprint UUIDs reordering the DSN. With deterministic UUIDs every retry would repeat the same failure, so try 1 uses the file order and tries 2 and 3 shuffle the footprint order with a fixed seed: each try differs, each run is reproducible.
- **Known limits, all refused with a clear error rather than guessed:** bottom-side parts, trapezoid and custom pad shapes in the DSN. Add them when a board needs them.
- **Not changed:** Freerouting's arguments, including `--router.via_costs=50`, which Freerouting 2.4.1 ignores ("Unknown settings property"); noted in the lessons.

## D-015 Starter board: the three "before a real order" fixes from D-014 (DECIDED, technical, 2026-09-29)
The owner said "just fix for now": no order and no cost table yet. Research: `research/2026-09-29-holes-and-fuse.md`, done by a separate agent with sources tagged VERIFIED/INFERRED.
- **Regulator cooling copper:**
  - U2's tab (+3V3) now sits in a +3V3 pour on both layers, x 1–13 / y 14–26 mm. It is filled above the GND pours (priority 10) and tied together by 20 vias on a 2 mm grid.
  - New pcbgen features, reusable on any board: `CopperZone` in `BoardSpec.zones`, and `RouteOptions.stitch_local` (net → via pitch). Stitching runs per net. The router still sees no pours, so +3V3 is fully routed with tracks and the pour comes on top. Unlike GND, unrouted +3V3 connections still count.
  - `routing.json` now reports every pour by net and layer (`pours`, which replaces `gnd_pours`) and `vias_by_net`.
  - Heat estimate (INFERRED, not measured): about 12×12 mm of copper on both sides plus vias should roughly halve the reviewer's 50–70 °C rise at 0.4 W. Measure it on a real board.
- **ESP32 EPAD holes: local footprint with 0.3 mm drills** (`lib/footprints/pcbgen.pretty/ESP32-S3-WROOM-1_EPAD-Drill0.3`). It is KiCad's footprint with only the 12 drills changed (0.2 → 0.3 mm) plus a new name and description. The positions, the 0.6 mm pads and the window-pane paste all stay.
  - Why: JLCPCB's capabilities page says a 0.2 mm hole on a pad of 0.45 mm or more is free. But on its quote form, the "0.2 mm" min-via option adds about $50 (drill, via covering and Kelvin test) to a $4 board. 0.3 mm is the form's stated free size, so the risk goes away.
  - Espressif recommends vias in the EPAD gaps (datasheet Fig. 11-1, 12 vias, no size given).
  - It is a separate library, so DRC `lib_footprint_mismatch` compares it with itself and no waiver is needed.
  - The annular ring is now 0.15 mm, below JLCPCB's 0.25 mm PTH recommendation but above the via minimum, and DRC with JLCPCB's rules passes (the ring INFERRED to be fine, since these are via-like holes inside copper).
  - The board's smallest hole is now 0.3 mm.
- **Fuse: F1 is now Bourns MF-NSMF075-2 (C89653)**, a 0.75 A hold / 1.5 A trip part, 6 V, 0.40 Ω max. It replaces the JK-nSMD050-30.
  - Bourns' own derating table gives 0.61 A hold at 50 °C and 0.52 A at 60 °C. A 0.5 A PTC holds only 0.35–0.40 A there (Bourns and Littelfuse tables), which is no margin over the ~0.35 A average load.
  - It still trips about 1 s at 2 A and 0.2 s at 3 A (read off a chart, INFERRED), so it still protects against a board short.
  - No fee change: the old part was already Extended, and JLCPCB has no Basic PTC fuses at all. $0.037 each, 17,850 in stock (2026-09-29).
  - 6 V rating over USB's 5.25 V; the TVS after it clamps spikes. Fallback if a higher rating is ever wanted: BHFUSE BSMD1206-075-16V (C883128). Its derating was not checked.
- **Result:** clean full run. Netlist and net classes PASS, ERC 0, DRC 0 open (the same 11 waived silk warnings), 0 unrouted, no copper in keep-outs.
- **Still open before any order:** the 8 UNVERIFIED rotations in JLCPCB's preview (D3 matters most), and the A/B/C cost table. At upload, confirm the quote keeps "Min via hole size" at 0.3 mm with no extra lines.

## D-014 Starter board changes after the datasheet check and blind review (DECIDED, technical, 2026-09-29)
Reports: `research/2026-09-29-datasheet-check.md` (every pin VERIFIED) and `research/2026-09-29-blind-review.md`.
- **Capacitors on 3V3:** C3 at the module is now 22 µF and C2 at the regulator 1 µF (were 10 µF and 22 µF). This follows Espressif's 22 µF + 0.1 µF at the module. It also brings the total (~23 µF nominal, less under 3.3 V bias; inferred) back to the edge of ST's LDL1117 stability plot, which only covers 1–22 µF. The board had 32 µF.
- **Fuse before TVS:** D3 (SMF5.0A) now sits on +5V after F1. A faulty charger trips the fuse instead of burning the TVS.
- **C1 moved** under U2, next to its input pin (it was ~7 mm away, reached through vias).
- **USB net class never applied:** its patterns `USB_D+` didn't match KiCad's `/USB_D+`. Now `*USB_D*`. A new `netclasses` gate (in `sch` and `check`) fails when any pattern matches no net, and was checked against the old patterns.
- **Stitching vias** now check their distance to other-net tracks directly (≥ 0.25 mm). One via ended up 0.185 mm from a track even though it was inside the pour; cause not found.
- **Deferred to "before a real order"** (all three done in D-015):
  - Extra 3V3 copper under the regulator tab for cooling. The reviewer estimates a 50–70 °C rise at 0.4 W (inferred).
  - The fuse's hot-derating margin (no derating curve in its datasheet).
  - The 12 × 0.2 mm thermal-via holes in KiCad's ESP32-S3-WROOM-1 footprint. JLCPCB allows them but charges extra for holes under 0.3 mm. Check the quote.
- **Before paying:** check the 8 UNVERIFIED rotations in JLCPCB's preview. D3 matters most: a reversed TVS shorts +5V.

## D-013 Stack survey confirms the stack (DECIDED, 2026-09-29)
- Session pcb-99 surveyed the alternatives (`research/2026-09-29-stack-survey.md`). Nothing is both better and within the owner's rules (1.0+, local CLI, no lock-in), so D-004 stands.
- **Revisit:** Diode `pcb`/Zener at its 1.0 (placement-preserving netlist sync).
- **Optional second opinions, never gates:** `pcb dfm` and kicad-happy's analysers.
- **Optional paid fallback router for dense boards:** DeepPCB's API, with the owner's OK.
- **D-005 note:** SKiDL's UUIDs are now deterministic, but the drawing-quality reason still holds.

## D-012 Routing and fab details learned on the starter board (DECIDED, technical, 2026-09-29)
- **The router never sees the copper pours.** When the DSN includes the GND pours, Freerouting counts every GND pad as connected and routes none of them. Tracks then cut the top pour into pieces, and pads were stranded (DRC `unconnected_items`, the 5 errors in `2cca03f`). The export step now removes the pours from the DSN, so every GND pad gets a real track. The pours and stitching vias come on top of that.
- **Power net class: 0.3 mm (was 0.4).** The committed `2cca03f` board was routed with net classes silently missing: every net was at 0.2 mm. With the classes applied, a 0.4 mm track could not reach the SHT40's 3V3 escape stub in 3 tries. At 0.3 mm it routes. 0.3 mm on 1 oz outer copper carries about 1 A at a 10 °C rise (IPC-2221), twice the ~0.5 A peak.
- **Freerouting retries:** up to 3 tries while any net other than the pour net is left unrouted. The ones seen so far failed the same way every try, so a retry mostly helps when luck is involved; the DRC gate decides.
- **Stitching fallback:** a pour piece with no free spot outside courtyards may get its via inside a courtyard, if it clears every pad by 0.2 mm (a tented via under a part body). A via in a pad would wick solder.
- **CPL rotations:** corrections come from the community table used by kicad-jlcpcb-tools (JLCKicadTools `cpl_rotations_db.csv`). Only unpolarised R/C/fuse parts are assumed to need no correction. Every other part without a table entry is listed as UNVERIFIED in `fab/README.md`, to check in JLCPCB's placement preview before paying.
- **Gates now:** netlist round trip (after `sch` and in `check`), ERC, DRC, and a routing report (`reports/routing.json`). The routing report fails on any unrouted connection or any track/via in a keep-out.

## D-011 SWIG segfaults: root cause fixed; SWIG steps isolated (DECIDED, owner approved the scope, 2026-09-29)
- **Root cause, found with glibc's heap checker** (`LD_PRELOAD=/usr/lib/libc_malloc_debug.so GLIBC_TUNABLES=glibc.malloc.check=3 PYTHONMALLOC=malloc`):
  1. pcbnew's Python `Remove()` sets `thisown=1`, so Python frees each removed track while KiCad still points at it (connectivity, DSN export). `route.py` removed old tracks at the start of every run, so every later step read freed memory. Removed tracks are now leaked on purpose (the process is short-lived), and connectivity is rebuilt.
  2. `zone.Outline()`, `fp.GetCourtyard()` and polygon `COutline(i)` come back with `thisown=True`, so Python would delete KiCad's own polygons. `pcbgen.kicad.borrowed()` marks them as KiCad's.
- **Isolation (the owner approved steps 3 and 4 of pcb-99's plan):**
  - Every stage runs in its own process when several stages are run together.
  - Inside `route`, each SWIG step (DSN export, SES import, stitching) runs in its own process (`kicad.run_step`). A crash reads "step X crashed (SIGSEGV)" and can't corrupt later steps.
  - Zones are filled by kicad-cli (`pcb drc --refill-zones --save-board`), not by SWIG's `ZONE_FILLER`.
- **Deferred to the KiCad 11 port:** writing the `.kicad_pcb` directly as S-expressions (step 2). With the root cause fixed, it is a large rewrite with no bug behind it, and the owner wants this test project kept lean.
- **Result:** the full pipeline runs clean under the heap checker.

## D-010 Layout and routing automation in pcbgen (DECIDED, technical, 2026-09-29)
Everything below lives in the generator, so every future board gets it.
- **References go to the fab layer.** Owner-facing silk labels (5V, 3V3, GND, TX, RX, SDA, SCL, PWR, LED, RESET, BOOT, Qwiic) are `Text` entries in `layout.py`. The title goes on the back silk. JLCPCB's minimum text height is 1.0 mm.
- **GND pours:** SMD pads connect solid (the fab reflows them) and through-hole pads get thermal reliefs (`ZONE_CONNECTION_THT_THERMAL`). With thermals everywhere, small pads (USB-C GND, SHT40) failed DRC's two-spoke minimum.
- **Escape stubs:** some footprints carry their own copper keep-out. The SHT40 DFN does ("no copper under the sensor"), leaving only a pad-sized notch. Freerouting always aims for pad centres and gives up on those pads.
  - `pcb._escape_stubs` adds a locked 0.2 mm track from each connected pad centre, along the pad's long axis, 0.5 mm past the pad end. The router connects to the free end.
  - `route.py` keeps locked tracks when it clears old routing.
- **GND stitching** after routing (`route.stitch`), because Freerouting doesn't stitch and a top pour cut by tracks leaves disconnected pieces:
  - A 3 mm grid of 0.6/0.3 vias, placed only where a via fits fully inside the filled pour on both layers, outside every courtyard and ≥0.5 mm hole-to-hole.
  - Then one via per remaining pour piece, found by a fine search. (It crashed Python until the SWIG ownership bugs were fixed: D-011.)
- **Net classes (starter):**
  - Default 0.2 mm track / 0.15 mm clearance, so signals escape the SHT40's 0.3 mm pads.
  - Power 0.3/0.2 (was 0.4, see D-012): about 1 A at a 10 °C rise per IPC-2221, against a ~0.5 A peak.
  - USB 0.3/0.15.
  - JLCPCB's floor is 0.1/0.1.
- **Waivers:** an accepted warning needs a `(type, substring, reason)` entry in the board's `WAIVERS`. The `check` stage now passes them to the gates. The starter waives only cosmetic silk items:
  - KiCad's test-point silk ring sits 0.14 mm from the pad (JLCPCB guideline: 0.15).
  - The module and USB-C outline silk run to the board edge, where both parts are flush by design.
  - Library footprints are *not* modified, so the lib-footprint parity check stays meaningful.
- **`scripts/render.sh boards/<name>`** renders the top side to PNG. Look at the render after every placement change.

## D-009 Firmware: C on ESP-IDF; the ESP32-S3 stays (DECIDED, 2026-09-29)
- Agreed with session pcb-99. The owner leans toward C on ESP-IDF.
- **ESP-IDF v6.1 is stable** (2026-08-27).
- **Rust is not ready here.** Rust's Wi-Fi driver `esp-radio` is only 1.0.0-beta.1 (2026-09-16). The `esp-idf-*` Rust crates have been community-maintained since Feb 2025. The S3's Xtensa core also needs Espressif's forked compiler.
- Closes the open question "Firmware language".

## D-008 USB ESD: H5VUT2U + SMF5.0A instead of USBLC6-2SC6 (DECIDED, technical, 2026-09-29)
- **The parts:**
  - H5VUT2U (C20615824): 0.6 pF clamp on D+/D−.
  - SMF5.0A (C19077497): a 200 W TVS on VBUS.
  - Both are JLCPCB **Preferred Extended**, so no loading fee. USBLC6 (C7519) would add $3.07 per order.
- **KiCad has no symbol for either part.** U3 uses `Device:D_TVS_Dual_AAC` (pin 1 → D+, pin 2 → D−, pin 3 → GND). D3 uses `Diode:SMF5V0A` (pin 1 = cathode → VBUS) on `Diode_SMD:D_SOD-123F`.
- **Pin mapping checked against the datasheet:** H5VUT2U pins 1/2 are the I/O lines and pin 3 is the common pin, from the datasheet's function diagram (image). The claim that pin 3 is GND is inferred from the "C(I/O–GND)" spec plus the diagram. SMF5V0A: pin 1 is the cathode, read from the symbol graphic, which matches KiCad's diode pad 1 = K.
- **Still to do:** the full datasheet verification by a separate agent (next steps).

## D-007 LDO: ST LDL1117S33R instead of AMS1117-3.3 (DECIDED, technical, 2026-09-29)
- **Dropout:** AMS1117 drops 1.1–1.3 V. USB can sag to about 4.4 V at the connector, less the fuse drop, which leaves the 3.3 V rail below the ESP32's 3.0 V minimum. LDL1117 drops 0.35 V typ / 0.6 V max at 1.2 A.
- **Capacitors:** AMS1117's datasheet asks for a 22 µF tantalum. LDL1117 is made for ceramic caps.
- **Cost:** +$3.07 per order (Extended). It fits the same SOT-223 footprint as AMS1117.
- **Symbol:** `Regulator_Linear:LD1117S33TR_SOT223`, which has the same pinout (1 GND, 2 OUT/tab, 3 IN). KiCad has no LDL1117 symbol.
- **UNVERIFIED:** LDL1117's maximum stable output capacitance (the datasheet figure is an image). The board has 22 µF + 10 µF + 100 nF on 3V3.

## D-006 Freerouting runs from its own Linux bundle, not Docker (DECIDED, technical, 2026-09-29)
- `freerouting-2.4.1-linux-x64.zip` ships its own Java 25.0.4 runtime. `scripts/fetch-tools.sh` fetches it into `tools/` (gitignored) and checks its sha256.
- Docker would need sudo: the daemon is stopped and the user isn't in the `docker` group. mise Java 25 would add a toolchain. Neither is needed now.
- Analytics are off (`--usage_and_diagnostic_data.disable_analytics=true`).

## D-005 Circuit code emits KiCad schematics directly; SKiDL rejected (DECIDED, technical, 2026-09-29)
- **Spike:** SKiDL 2.3.0 has a KiCad 10 generator, and its output is ERC-checkable. But the drawing is poor (the ESP32 symbol is rotated, labels overlap pin names, parts are crammed together), and part UUIDs are random unless every part is tagged by hand.
- **Our own `pcbgen` package** (about 900 lines, no dependencies beyond KiCad's own):
  - It reads KiCad's library symbols and flattens `extends`.
  - Parts are placed in titled blocks. Each pin gets a wire stub plus a net label; power nets use power symbols; unused pins get no-connect flags.
  - UUIDs are deterministic (uuid5).
  - It writes KiCad 9 format, then runs `kicad-cli sch upgrade`, so KiCad itself writes the final file.
  - It also writes project-local sym/fp lib tables and a `.kicad_pro` (net classes and rules).
- **Pre-check:** every pin must be on a net or marked nc before any file is written.
- **Result:** the starter schematic passes KiCad ERC with 0 violations (no waivers).
- **The PCB comes from `kicad-cli sch export netlist`,** so it is built from exactly what ERC checked, and each footprint carries its symbol's UUID for the schematic-parity DRC.
- **Fab DRC rules:** Cimos/KiCad-CustomDesignRules moved to **Cimos/kicad-druid** (MIT). `rules/JLCPCB-2L-1oz.kicad_dru` is from v1.2.0.

## D-004 Pipeline = KiCad-native; full bake-off replaced by a routing check (CONFIRMED 2026-09-29)
- **Owner's direction:** "zero preferences, only care about quality and ease; assume Opus 5.5 can handle any tool." So tools are judged on output quality, how well the checks catch mistakes, and durability. How easy they are for an AI to use doesn't count.
- **atopile is out, whatever the skill level.** Its problems are platform risk, not difficulty:
  - its local tool is being retired
  - part picking goes through a closed, login-only backend that already broke once (July 2026)
  - it is locked to KiCad 9
- **tscircuit is not the pipeline.** What it adds is autorouting and convenience, which only saves effort. It costs quality:
  - its router can report success on a board that still has shorts
  - it has no real ERC
  - its KiCad export has pad-merging bugs
  - it is pre-1.0, with several releases a day
- **KiCad-native wins on quality:**
  - KiCad is the industry-standard format.
  - KiCad's own ERC and DRC check the real design files, using the fab's rules.
  - Its fab outputs are accepted everywhere (JLCPCB, NextPCB, PCBWay).
  - The owner, or any EE, can open the design in a free GUI.
  - The pieces are maintained and don't depend on any vendor.
- **How it works:**
  - The circuit is written as Python code, and the generator emits a real KiCad schematic, so ERC runs on it.
  - The choice between SKiDL and emitting the KiCad files directly is made in a Phase 1 spike. The requirement is a real `.kicad_sch` that KiCad's ERC can check.
  - Parts are placed by code (explicit coordinates and constraints).
  - Freerouting does the routing, run headless in Docker. Quilter's free tier and DeepPCB are the fallback routers.
- **Tooling:** plain project scripts plus a project skill, called through Bash. An MCP server adds nothing for a single-agent CLI workflow; build one only if a real need appears.
- **The bake-off shrinks to what is actually open:**
  - whether Freerouting's result on the starter board is good enough
  - whether the whole pipeline runs from code to checked fab files without the owner
- This supersedes D-003.

## D-003 Bake-off lineup: drop atopile, add a KiCad-native lane (SUPERSEDED by D-004)
- **atopile is out.**
  - Its local CLI (0.15.9) is soft-deprecated, needs an atopile account for part picking, and only works with KiCad 9. KiCad 10 files break it (#1822).
  - Its public repo has been frozen since March. The supported 0.16 runs only in a browser with its own agent, which bypasses Claude Code, git and our gates.
  - Building a "repeatable pipeline" on a tool being retired is a bad bet.
- **Lane T: tscircuit end to end.**
  - The toolchain: pinned versions, Bun, the official skill, and its own autorouter and checks.
  - Its output is then re-checked independently with KiCad 10 DRC using JLCPCB rules.
- **Lane K: KiCad-native.**
  - The circuit is written in Python (SKiDL). Parts come from the JLCPCB catalog, and their footprints are pulled from LCSC.
  - Parts are placed by a script, then Freerouting (Docker) routes the board.
  - KiCad 10 runs ERC and DRC, and kicad-cli produces the fab outputs.
- **Cross-check:** the Lane T board is also routed with Freerouting, to compare the two routers on an identical netlist.
- **Test board = draft starter board.** Using the real candidate means the winner's output feeds phase 4. Nothing is ordered during the bake-off.

## D-002 Starter board size >= 50x50 mm (DECIDED, technical)
NextPCB's Rev 0 free-assembly offer rejects boards smaller than 50×50 mm. A starter
board that size or larger keeps that option open, and it costs essentially nothing extra
at JLCPCB (the 5-board price covers up to 100×100).

## D-001 Laptop toolchain (DONE 2026-09-29: KiCad 10.0.6 + kicad-library installed, user in `uucp`)
- **KiCad 10.0.6 from Arch `extra`** (`kicad`, `kicad-library`). The 3D library
  (`kicad-library-3d`, 3.2 GB) is deferred to the enclosure phase.
  KiCad 10 over 9: it is current, kicad-cli has JSON DRC/ERC, and Freerouting and the MCP server target it. The one tool that needed 9 (atopile) is dropped (D-003).
- **Freerouting 2.4.1 in Docker.** It needs Java 25; the laptop has Java 21 for Android work, and Docker avoids a second global JDK.
- **Bun and Python pinned per project** (`mise.toml`, `uv`), nothing global, as with other projects.
  A Python venv that must import KiCad's `pcbnew` has to be created from system Python 3.14 with system site-packages.
- **Serial access:** add the user to the `uucp` group so the ESP32-S3's `/dev/ttyACM*` can be flashed without root.
- **Deferred:**
  - OpenSCAD: Arch ships the 2021.01 release, which is too old. Pick the tool in phase 8.
  - Firmware toolchain: phase 3, after the language choice.

## Open questions
- (Firmware language: closed by D-009.)
- **Before a real order (skipped while this is a tooling test, agreed with pcb-99):**
  - a 4-layer routing variant for comparison
  - vendoring the verified footprints into the repo (`lib/footprints/` exists since D-015; only the modified ESP32 footprint is there)
  - (a JLCPCB parts/cost script: the `cost` stage, D-023; the A/B/C comparison is still open)
  - (project skill: done as the global draft `~/.claude/skills/pcb-pipeline/SKILL.md`, at the owner's request)
  - check every UNVERIFIED rotation in JLCPCB's placement preview (list in `boards/<name>/fab/README.md`)
  - the fab's own manufacturability check on upload; current fab promotions (NextPCB Rev 0 vs JLCPCB); the A/B/C cost table from `docs/brief.md`
  - (D-021) whether 5 is JLCPCB's smallest bare-PCB quantity with 2 assembled, and whether a module pre-ordered into "My Parts Lib" still pays the $3.07 Extended fee in each later order
- (KiCad 11 port: closed by D-016. When KiCad 11 arrives, check that `sch upgrade` / `pcb upgrade` still accept the format versions pcbgen writes, then run the starter board end to end.)
- (Routing quality between tries: closed by D-017.)
- **Unexplained, harmless for now:** why the `2cca03f` board was routed without its net classes (a fresh `sch pcb route` applies them), and why one stitching via landed 0.185 mm from a track while inside the pour (now checked directly; the Rust stitcher keeps the same direct check).
