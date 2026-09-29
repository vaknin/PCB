"""Verification gates: KiCad ERC and DRC, read from their JSON reports.

A gate passes only with zero errors and zero *unwaived* warnings. Waivers live
in the board's layout.py (WAIVERS: list of (violation type, substring, reason)),
so every accepted warning has a written reason in git.
"""

from __future__ import annotations

import json
import subprocess
from collections import Counter
from pathlib import Path


def _cli(*args: str) -> subprocess.CompletedProcess:
    return subprocess.run(["kicad-cli", *args], capture_output=True, text=True)


def _violations(report: dict) -> list[dict]:
    if "sheets" in report:
        return [v for s in report["sheets"] for v in s["violations"]]
    out = []
    for key in ("violations", "unconnected_items", "schematic_parity"):
        for v in report.get(key, []):
            out.append({**v, "_group": key})
    return out


def _describe(v: dict) -> str:
    items = "; ".join(i.get("description", "") for i in v.get("items", []))
    return f"[{v['severity']}] {v['type']}: {v['description']} -- {items}"


def _judge(name: str, violations: list[dict], waivers: list[tuple[str, str, str]]) -> bool:
    open_, waived = [], []
    for v in violations:
        if v["severity"] == "ignore":
            continue
        text = _describe(v)
        w = next((w for w in waivers if w[0] == v["type"] and w[1] in text), None)
        if w and v["severity"] != "error":
            waived.append((text, w[2]))
        else:
            open_.append(text)
    counts = Counter((v["severity"], v["type"]) for v in violations)
    print(f"== {name}: {len(violations)} violations {dict(counts) if counts else ''}")
    for text, why in waived:
        print(f"   waived: {text}\n           reason: {why}")
    for text in open_:
        print(f"   OPEN:   {text}")
    ok = not open_
    print(f"== {name}: {'PASS' if ok else 'FAIL'} ({len(open_)} open, {len(waived)} waived)")
    return ok


def erc(project_dir: Path, name: str, waivers=()) -> bool:
    reports = project_dir / "reports"
    reports.mkdir(exist_ok=True)
    out = reports / "erc.json"
    res = _cli("sch", "erc", "--format", "json", "--severity-all", "--units", "mm",
               "-o", str(out), str(project_dir / f"{name}.kicad_sch"))
    if not out.exists():
        raise RuntimeError(f"ERC did not produce a report:\n{res.stdout}\n{res.stderr}")
    return _judge("ERC", _violations(json.loads(out.read_text())), list(waivers))


def drc(project_dir: Path, name: str, waivers=()) -> bool:
    reports = project_dir / "reports"
    reports.mkdir(exist_ok=True)
    out = reports / "drc.json"
    res = _cli("pcb", "drc", "--format", "json", "--severity-all", "--units", "mm",
               "--schematic-parity", "--refill-zones", "--save-board",
               "-o", str(out), str(project_dir / f"{name}.kicad_pcb"))
    if not out.exists():
        raise RuntimeError(f"DRC did not produce a report:\n{res.stdout}\n{res.stderr}")
    return _judge("DRC", _violations(json.loads(out.read_text())), list(waivers))


def run(project_dir: Path, name: str, waivers=()) -> bool:
    a = erc(project_dir, name, waivers)
    b = drc(project_dir, name, waivers)
    return a and b
