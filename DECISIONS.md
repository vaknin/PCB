# Decisions

Newest first. Each entry: what was decided, why, and status (proposed / confirmed by owner).
Research behind these: `research/2026-09-29-landscape.md`.

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

## Open questions (for phase 2 spec)
- **Firmware language.** Rust (esp-hal 1.0) runs on the ESP32-S3, but the S3's Xtensa core needs
  Espressif's forked compiler (installed with `espup`). RISC-V parts (ESP32-C3/C6) use the standard stable Rust compiler.
  The alternative is C with ESP-IDF, or Arduino/ESPHome. Decide with the owner's feature goals.
