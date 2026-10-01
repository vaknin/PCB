# Independent review agents

Three agents check a design without the designer's reasoning: the datasheet checker, the blind
reviewer and the red team. They are required before freeze and before any order
(`rounds.md`). This setup worked well on the starter board.

## How to run them
- Background general-purpose agents, run in parallel, each told it did not build the design
  and should be skeptical. Give no design rationale beyond the brief and the files.
- Read-only except for one output file each in `research/` (`research/<date>-<what>.md`),
  which it fills with a table.
- Tell each not to run the pipeline in place (it rewrites the board); `--out` to a scratch
  directory is fine.
- Read every report before relying on it.

## Datasheet checker
- List every claim to verify: module pads, sensor pins, regulator pins and capacitor limits,
  connector pins and pin order, ESD pin roles, every power number in `board.toml` marked
  INFERRED.
- Require a primary source per item (manufacturer PDF, page or table) and a verdict:
  VERIFIED / WRONG / UNVERIFIABLE.
- Have it trace symbol pin → footprint pad → net in the `.kicad_pcb`, so it checks what gets
  soldered, not what the code meant.
<!-- pending: lands with 2a/2b/2c/2d -->
- Include every part `lib/IMPORTED.toml` lists as UNVERIFIED (imported with `pcb lib import`);
  a VERIFIED verdict is what lets it lose that tag.
<!-- /pending -->

## Blind reviewer
- Give it the circuit, layout, KiCad files, reports, fab outputs, parts research and
  `docs/brief.md`, plus `scripts/render.sh` to look at the board.
- Ask for must/should/note findings, each with a location and a fix; a cost estimate; and a
  verdict. Areas: power, manufacturability, layout, cost.

## Red team
- One question: "find what breaks this board". Power-up order, brown-out, a stuck button, a
  flat battery, a USB cable plugged in while asleep, ESD, a part out of stock, a case that
  doesn't close.
- Same files as the blind reviewer, plus `spec.md`, `board.toml` and `firmware/`.

## After the reports
Fix the cheap findings, re-run the whole pipeline, and log in `DECISIONS.md` what changed and
what was deferred. Findings that stay open become `[[risk]]` entries in `board.toml`.
