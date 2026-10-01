# pcb-pipeline skill evals

Rerun after a real change to `.claude/skills/pcb-pipeline/` (not needed for small edits;
`scripts/skill-check.sh` covers names and paths).

- `evals.json`: eight owner requests and what a good plan does for each (`assertions`). Run each as a
  **dry run** with the skill and without it (the skill-creator skill's eval loop): subagents
  read the repo and write a plan, never edit it, order, use Wokwi quota or call Gemini/GitHub.
  Grade the plans against the assertions and look for gaps in the skill itself.
- `trigger_set.json` and `trigger_check.py [RUNS]`: whether a fresh `claude -p` outside the repo
  picks the skill for each query (9 should, 7 lookalikes shouldn't). File-changing tools are
  blocked and each run stops at its first tool call.

Results of 2026-10-01 (iteration 1): trigger 16/16. Plans 30/32 with the skill and 30/32
without, since the repo's CLAUDE.md and docs already carry most rules. The skill showed in
exact paths (`kicad/route/failure.json`) and the gaps the grader found, fixed in the same
commit. Harder assertions should test what only the skill says.
