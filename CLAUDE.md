# PCB pipeline

Goal: owner describes a hardware idea; Claude designs the circuit as code, runs
deterministic checks, prepares a fab order, writes firmware and an enclosure.
Owner is not an EE. Full brief: `docs/brief.md`.

## Start of every session
1. Read `HARDWARE_LESSONS.md` (verified pinouts, fab rules, tool gotchas, mistakes).
2. Read `DECISIONS.md` (what was decided and why; open questions).

## Rules that matter most
- Nothing that costs money happens without a clear summary and the owner's OK.
- Nothing is ordered until every gate in `docs/brief.md` passes. Read the actual
  ERC/DRC reports; never trust an exit code alone.
- Mark anything inferred rather than verified.
- Plain language to the owner; no schematics unless asked; no calendar timelines.
- Log technical decisions in `DECISIONS.md` instead of asking.
