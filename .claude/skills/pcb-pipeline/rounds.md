# Rounds, readiness, freeze and the order

The process and its reasons: `docs/workflow.md` steps 4–5 and "Right the first time". The
readiness page's colour rules: `docs/pipeline.md` "The readiness page".

## A round
1. Change the design (circuit, layout, `board.toml`).
2. Full run (`cargo run --release -p <board> --`), then `sim`, then `case` (from the first
   layout on; look at its renders), then `cost`.
3. Research the cheaper options for the cost summary: other fabs, fee-free substitutes for
   fee-bearing parts and what each costs in function. They go on a choice page.
4. A reviewer agent checks the round (`review-agents.md`).
5. Write `boards/<name>/round.md` (`templates/round.md`): what changed and why, open risks,
   questions for the owner, the reviewer's findings.
6. Commit, then `scripts/draft.sh <board>` (tags `<board>-draft-<n>` on a clean tree).
7. `review` → publish `review/index.html` as an Artifact, updating the same one each round.
   Publish `review/readiness.html` as its own Artifact (also updated in place).

The page also shows the newest `bringup/selftest-*.json` once a built board has been tested
(`firmware.md`), and warns 60 days before a `[provision]` secret `expires`. Order quantities and
budget come from `board.toml` `[order]` (`boards`, `assembled`, `budget_usd`; default 5 bare, 2
assembled).

## Right the first time
A mistake found after the order costs a second shipment.
- Sort every risk by fix cost: `firmware` (free, forever, over USB or Wi-Fi) / `rework` (jumper,
  test pad on the delivered board) / `new_board`. The work before freeze is moving items out of
  the last group.
- Ask the owner on a choice page "what might you want this to do later?" Give each plausible
  wish its hardware hook now (a sense line, a spare pin on a pad, flash room, two update
  slots), even when its firmware waits. Firmware can change after delivery; copper can't.
- Rev A firmware ships with a Wi-Fi update path and rollback; USB stays the fallback.
- The board reports on itself: battery level, firmware version and last error, where the owner
  already looks (for capture-clip: a status file in the notes repo).
- Independent eyes: datasheet checker, blind reviewer and red team, each fresh, without the
  designer's reasoning (`review-agents.md`).
- Say plainly what simulation can't prove: radio range, microphone sound, real sleep current,
  how the case feels. The dev-board session and rework hooks cover what they can; the order
  includes spare boards.

## The readiness page (before the freeze question)
Its data is in `board.toml`: `[[requirement.proof]]` per requirement (`how` = simulated |
datasheet | devboard | gate | unprovable, `evidence`) and `[[risk]]` (`what`, `tag` =
UNVERIFIED | INFERRED, `fix` = firmware | rework | new_board, `miss`, optional `check`,
optional `accepted` = the owner's own words). Every open UNVERIFIED or INFERRED item gets a
`[[risk]]`. Red (a requirement with no proof, or a `new_board` risk not accepted) blocks
freeze unless the owner accepts it by name.
<!-- pending: lands with 2a/2b/2c/2d -->
A proof with `evidence = "scenario:<name>"` is checked against `firmware/sim.json`: it is red if
that `[[sim.scenario]]` failed or never ran.
<!-- /pending -->

## Freeze
On the owner's "freeze": `scripts/freeze.sh <board>`. It needs a draft-tagged HEAD, a clean
tree, and a current `review/readiness.json` with nothing red. It tags
`<board>-rev<X>-freeze`. The case (`case/`) freezes with the board.
Freeze also stores in the tag the sha256 of every footprint and symbol the board uses, and
the kicad-cli version (`scripts/lib-hashes.sh`). `scripts/check-frozen.sh` refuses when any of
them changed since, and lists which: a KiCad update or an edited `lib/` footprint would
otherwise slip into a regenerated board unseen. `scripts/freeze.sh --dry-run <board>` runs
every check and shows the tag message without tagging.

## Before any order
In this order, every one required:
1. `scripts/check-frozen.sh <board>` prints OK.
2. All gates PASS (read the reports, `results.md`).
3. A datasheet check by a separate agent that didn't build the design.
4. A blind review (power, manufacturability, layout, cost).
5. A plain-language summary with the cost options (below), boards and case together (the
   case's STL/STEP go to JLC3DP in the same summary).
6. The owner's explicit OK.
Also check every UNVERIFIED rotation (listed in `fab/README.md`) in JLCPCB's placement preview,
and run the fab's own manufacturability check, before paying.

## The A/B/C cost table
- A: fully assembled. B: the fab assembles SMD; the owner does through-hole and modules.
  C: bare board, hand soldering (only if every part is hand-solderable with a basic iron).
- Include PCB, parts, assembly, Extended-part fees, shipping to Israel (FedEx ~$30) and 18% VAT
  above the $75 goods exemption (re-check it; `HARDWARE_LESSONS.md` "Fab rules").
- Check stock and price on jlcpcb.com's part page (the `headless-browser` skill renders it).
- The starter board came to ~$105 delivered for 5 assembled; only A was realistic there.
- Which option is the owner's: put it on a choice page (`choice-page.md`).
