"""Verification gates: KiCad ERC and DRC, read from their JSON reports.

A gate passes only with zero errors and zero *unwaived* warnings. Waivers live
in the board's layout.py (WAIVERS: list of (violation type, substring, reason)),
so every accepted warning has a written reason in git.
"""

from __future__ import annotations

import fnmatch
import json
import subprocess
from collections import Counter
from pathlib import Path

from .circuit import Circuit
from .sexpr import find, find_all, parse


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


def export_netlist(sch: Path) -> list:
    out = sch.with_suffix(".net")
    res = _cli("sch", "export", "netlist", "--format", "kicadsexpr", "-o", str(out), str(sch))
    if res.returncode != 0 or not out.exists():
        raise RuntimeError(f"netlist export failed: {res.stdout}\n{res.stderr}")
    tree = parse(out.read_text())
    out.unlink()
    return tree


def netlist(project_dir: Path, name: str, circuit: Circuit) -> bool:
    """Round trip: the netlist KiCad reads back from our schematic must group exactly the
    pins that circuit.py connects. Catches a writer bug (a label on the wrong pin, a
    stray wire) that ERC can't see, since a wrong but tidy schematic passes ERC.

    Nets are compared as pin sets (names differ: KiCad prefixes local labels with "/").
    Single-pin "unconnected-(...)" nets are KiCad's own names for no-connect pins.
    Verified to fail when a pin is dropped from a circuit.py net (2026-09-29).
    """
    tree = export_netlist(project_dir / f"{name}.kicad_sch")
    kicad = {}
    for n in find_all(find(tree, "nets"), "net"):
        pins = frozenset((find(x, "ref")[1], find(x, "pin")[1]) for x in find_all(n, "node"))
        nname = find(n, "name")[1]
        if not (nname.startswith("unconnected-") and len(pins) == 1):
            kicad[pins] = nname
    # "#..." refs (PWR_FLAG) are ERC markers only; KiCad leaves them out of the netlist
    ours = {frozenset((p.part.ref, p.pin.number) for p in net.pins
                      if not p.part.ref.startswith("#")): net.name
            for net in circuit.nets.values() if net.pins}
    only_kicad = [f"{kicad[s]}: {sorted(s)}" for s in kicad.keys() - ours.keys()]
    only_ours = [f"{ours[s]}: {sorted(s)}" for s in ours.keys() - kicad.keys()]
    ok = not only_kicad and not only_ours
    print(f"== NETLIST: {len(ours)} nets in circuit.py, {len(kicad)} in KiCad's netlist")
    for line in only_ours:
        print(f"   OPEN:   circuit.py net missing from the schematic: {line}")
    for line in only_kicad:
        print(f"   OPEN:   schematic net not in circuit.py: {line}")
    print(f"== NETLIST: {'PASS' if ok else 'FAIL'}")
    return ok


def netclasses(project_dir: Path, name: str, rules) -> bool:
    """Every net-class pattern must match a net in KiCad's netlist. A pattern that
    matches nothing leaves its nets on Default silently (seen: "USB_D+" vs KiCad's
    "/USB_D+"), and every later gate passes on the wrong rules."""
    tree = export_netlist(project_dir / f"{name}.kicad_sch")
    nets = [find(n, "name")[1] for n in find_all(find(tree, "nets"), "net")]
    dead = [f"{nc.name}: {pat!r}" for nc in rules.classes for pat in nc.patterns
            if not any(fnmatch.fnmatchcase(n, pat) for n in nets)]
    for d in dead:
        print(f"   OPEN:   net-class pattern matches no net: {d}")
    print(f"== NETCLASSES: {'PASS' if not dead else 'FAIL'}")
    return not dead


def run(project_dir: Path, name: str, circuit: Circuit, rules, waivers=()) -> bool:
    from . import report   # reads the board DRC just refilled and saved
    n = netlist(project_dir, name, circuit) and netclasses(project_dir, name, rules)
    a = erc(project_dir, name, waivers)
    b = drc(project_dir, name, waivers)
    r = report.run(project_dir, name, rules)
    return n and a and b and r
