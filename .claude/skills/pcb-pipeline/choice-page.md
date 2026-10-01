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
silently applied. A fee-bearing part the owner named themselves, with no fee-free substitute,
needs no choice page: say its fee in plain words when adding it, and it shows in the next cost
summary.

## How to ask
Don't ask these as plain text in the terminal.
1. Load the `artifact-design` skill; write the HTML in the scratchpad.
2. One card per option, the recommended option first and marked.
3. A picture of each option, as cheap as shows the difference: a shape or material change gets
   the `case` stage's renders (re-run `case` per variant with a scratch `board.toml` and `--out`
   to scratch); a case colour gets the existing renders recoloured (`COL_CASE` in
   `enclosure/case.py` overridden from a scratch script: colour is not a `board.toml` key); a
   board colour gets `kicad-cli pcb render`; a part that doesn't change the look gets its
   product photo or a simple drawing, not a full pipeline run per option. A picture that isn't
   the real design says so.
4. The price difference (live JLCPCB/JLC3DP numbers, or marked INFERRED) and what it changes
   in plain words: look, feel, strength, lead time.
5. Group choices that come up together on one page; update the same Artifact for the next
   batch.
6. Then `AskUserQuestion` with the same options, the recommendation first, so the owner answers
   in one click.
7. Record the answer with the owner's words in `DECISIONS.md`, and in `board.toml` where it
   has a key (`[case] material`); otherwise as a comment there (a case colour) and in the order
   notes. Unknown `board.toml` keys fail the BOARD.TOML gate.

Variants for one page are independent work: give each to its own subagent.

## When they come up
- At the spec: size, battery, what the device might be wanted to do later (`rounds.md`).
  Open brainstorm questions (what should it do, where does it live, how many) are plain chat;
  any either/or with a cost or look difference (battery or USB power, size, a budget tier) goes
  on the choice page, even during the brainstorm.
- At the first round with a case: material, colour, finish.
- At every round's cost summary: fab, fee-bearing parts and their substitutes.
- Before the order: board colour, quantities, the A/B/C cost options.
