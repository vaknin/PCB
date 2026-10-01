# Changing pcbgen, the case code or the scripts

Code layout and what the tests cover: `docs/pipeline.md` "Tests" and "Code layout".

## Tests
- Unit tests, no KiCad needed: `cargo test --release`.
- End to end on the starter (needs kicad-cli and Freerouting, a few minutes):
  `cargo test --release -p starter -- --ignored`.
- After changing `enclosure/case.py`: `enclosure/.venv/bin/python enclosure/test_case.py`
  (~4 min). It breaks the starter's case on purpose and checks each break is caught. The venv
  needs `cd enclosure && uv sync` once.
- After changing this skill: `scripts/skill-check.sh`.

## Comparing output: in scratch, never in place
- Run `cargo run --release -p starter -- <stages> --out <scratch>` and compare against the
  committed `boards/starter/kicad` and `boards/starter/fab`.
- The `.kicad_sch` must stay byte-identical, apart from the date in its title block.
- Compare boards footprint by footprint, matched by reference: KiCad orders them by UUID, so a
  plain diff is noise.
- BOM and CPL should be byte-identical.
- After an in-place run, commit the regenerated `kicad/`, `fab/` and render with the code, and
  tell the owner if the tracks changed (the design didn't).

## Tool notes
- DSN questions: KiCad 10's SWIG module still exports a reference DSN:
  `/usr/bin/python3 -c "import pcbnew; pcbnew.ExportSpecctraDSN(pcbnew.LoadBoard(P), OUT)"`.
- Freerouting settings on the command line are its Java field names
  (`router.scoring.viaCosts`, `router.fanout.enabled`); list them with `javap`
  (`HARDWARE_LESSONS.md`).
- `scripts/fr-violations/run.sh <board.dsn>` lists the clearance violations Freerouting counts.
- New pcbgen features are opt-in fields with defaults, proven on `boards/starter` first, then
  used on a real board. Log each in `DECISIONS.md`.

## When to revisit the stack
Survey: `research/2026-09-29-stack-survey.md`.
- Diode `pcb`/Zener at 1.0 (placement-preserving netlist sync).
- Optional second opinions, never gates: `pcb dfm`, the kicad-happy analysers.
- A paid fallback router for dense boards: the DeepPCB API (owner's OK first; it costs money).
- After a KiCad major upgrade: check that `kicad-cli sch upgrade` / `pcb upgrade` accept the
  format versions in `crates/pcbgen/src/schematic.rs` and `crates/pcbgen/src/pcb.rs`, then run
  the starter board end to end.
