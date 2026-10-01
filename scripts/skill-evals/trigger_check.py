#!/usr/bin/env python3
"""Usage: trigger_check.py [RUNS] [RESULTS.json]. Costs model tokens (claude -p, 16 queries x RUNS).
Does a fresh `claude -p` session outside the repo pick the pcb-pipeline skill?
Runs each query N times from a scratch dir with write/exec tools blocked; stops a run at its
first tool call (or the timeout) and records whether that call was Skill(pcb-pipeline)."""
import json, os, select, subprocess, sys, tempfile, time
from concurrent.futures import ThreadPoolExecutor
W = os.path.dirname(os.path.abspath(__file__))
RUNS, TIMEOUT = int(sys.argv[1]) if len(sys.argv) > 1 else 2, 90
CWD = tempfile.mkdtemp(prefix="pcb-trigger-")  # outside the repo: the skill must load from ~/.claude/skills
BLOCK = "Bash,Write,Edit,NotebookEdit,Agent,Workflow,WebFetch,WebSearch,Artifact,AskUserQuestion"

def one(q):
    env = {k: v for k, v in os.environ.items() if k != "CLAUDECODE"}
    p = subprocess.Popen(["claude", "-p", q, "--output-format", "stream-json", "--verbose",
                          "--model", "claude-opus-5-5", "--disallowedTools", BLOCK],
                         stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, cwd=CWD, env=env, text=True)
    t0, first = time.time(), None
    try:
        while time.time() - t0 < TIMEOUT and first is None:
            r, _, _ = select.select([p.stdout], [], [], 1.0)
            if not r:
                if p.poll() is not None: break
                continue
            line = p.stdout.readline()
            if not line: break
            try: ev = json.loads(line)
            except ValueError: continue
            if ev.get("type") == "assistant":
                for c in ev["message"].get("content", []):
                    if c.get("type") == "tool_use":
                        first = (c["name"], json.dumps(c.get("input", {}))[:120]); break
    finally:
        p.kill(); p.wait()
    hit = bool(first) and first[0] == "Skill" and "pcb-pipeline" in first[1]
    return hit, first

qs = json.load(open(os.path.join(W, "trigger_set.json")))
jobs = [(i, q) for i, q in enumerate(qs) for _ in range(RUNS)]
with ThreadPoolExecutor(6) as ex:
    res = list(ex.map(lambda j: (j[0], one(j[1]["query"])), jobs))
out, ok = [], 0
for i, q in enumerate(qs):
    rs = [r for k, r in res if k == i]
    rate = sum(h for h, _ in rs) / len(rs)
    good = (rate >= 0.5) == q["should_trigger"]; ok += good
    out.append({"query": q["query"], "should_trigger": q["should_trigger"], "rate": rate, "ok": good,
                "first_tools": [f for _, f in rs]})
    print(("OK  " if good else "MISS"), f"{rate:.2f}", "T" if q["should_trigger"] else "F", q["query"][:80])
print(f"{ok}/{len(qs)} correct")
if len(sys.argv) > 2:
    json.dump(out, open(sys.argv[2], "w"), indent=1)
