# Choices the owner makes: the choice page

## What is the owner's
- What they see, hold or pay for: the case's material (resin or nylon), colour, finish and
  shape; the button's feel; the board's soldermask and silkscreen colour; size against battery
  life; features; cost tiers and quantities.
- Money against function: which fab or assembler; fee-bearing (Extended) parts against
  fee-free substitutes that cost some function (e.g. a smaller battery); cheaper-but-less
  options; an off-the-shelf module against a custom board.
- Everything else (tools, parts that don't change look, feel or price) is Claude's, logged in
  `DECISIONS.md`.

For money-against-function choices, research every option with live prices, say in plain
words what each costs in function, recommend one, and design to the owner's pick. A technical
fix that adds a fee-bearing part (e.g. a lower-leakage diode) is proposed with its price, never
silently applied.

## How to ask
Don't ask these as plain text in the terminal.
1. Load the `artifact-design` skill; write the HTML in the scratchpad.
2. One card per option, the recommended option first and marked.
3. A picture of each option: the `case` stage's renders with that option applied (re-run `case`
   per variant with a scratch `board.toml` and `--out` to scratch), a `kicad-cli pcb render`
   of the board in that colour, or a simple drawing. A picture that isn't the real design says
   so.
4. The price difference (live JLCPCB/JLC3DP numbers, or marked INFERRED) and what it changes
   in plain words: look, feel, strength, lead time.
5. Group choices that come up together on one page; update the same Artifact for the next
   batch.
6. Then `AskUserQuestion` with the same options, the recommendation first, so the owner answers
   in one click.
7. Record the answer with the owner's words in `DECISIONS.md` and in `board.toml` (e.g.
   `[case] material`).

Variants for one page are independent work: give each to its own subagent.

## When they come up
- At the spec: size, battery, what the device might be wanted to do later (`rounds.md`).
- At the first round with a case: material, colour, finish.
- At every round's cost summary: fab, fee-bearing parts and their substitutes.
- Before the order: board colour, quantities, the A/B/C cost options.
