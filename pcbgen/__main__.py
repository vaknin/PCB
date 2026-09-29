"""Pipeline driver: python -m pcbgen <board_dir> [stage ...]

Stages run in order: sch, pcb, route, check, fab (default: all).
A board directory holds circuit.py (build() -> Circuit) and layout.py (SPEC: BoardSpec,
RULES: BoardRules); generated KiCad files go to <board_dir>/kicad/.
"""

from __future__ import annotations

import importlib.util
import signal
import subprocess
import sys
from pathlib import Path

STAGES = ["sch", "pcb", "route", "check", "fab"]


def _load(path: Path):
    spec = importlib.util.spec_from_file_location(path.stem, path)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def main(argv: list[str]) -> int:
    if not argv:
        print(__doc__)
        return 2
    board_dir = Path(argv[0]).resolve()
    stages = argv[1:] or STAGES
    bad = set(stages) - set(STAGES)
    if bad:
        print(f"unknown stage(s): {bad}")
        return 2
    if len(stages) > 1:
        # one process per stage: a crash in KiCad's SWIG code stops that stage by name
        # and can't leave a corrupted heap for the stages after it
        for stage in (s for s in STAGES if s in stages):
            res = subprocess.run([sys.executable, "-X", "faulthandler", "-u", "-m", "pcbgen",
                                  str(board_dir), stage], cwd=Path(__file__).resolve().parent.parent)
            if res.returncode < 0:
                print(f"stage {stage} crashed ({signal.Signals(-res.returncode).name})")
                return 1
            if res.returncode:
                print(f"stage {stage} failed (exit {res.returncode})")
                return res.returncode
        return 0

    sys.path.insert(0, str(board_dir))
    circuit = _load(board_dir / "circuit.py").build()
    layout = _load(board_dir / "layout.py")
    out = board_dir / "kicad"
    name = circuit.name

    from . import gates, pcb, project, route, schematic, fab
    if "sch" in stages:
        project.write(out, name, layout.RULES)
        print("schematic:", schematic.write(circuit, out))
        if not (gates.netlist(out, name, circuit) and gates.netclasses(out, name, layout.RULES)):
            return 1
    if "pcb" in stages:
        print("pcb:", pcb.build(out, name, layout.SPEC))
    if "route" in stages:
        route.route(out / f"{name}.kicad_pcb", layout.ROUTE)
    if "check" in stages:
        ok = gates.run(out, name, circuit, layout.RULES, getattr(layout, "WAIVERS", ()))
        if not ok:
            return 1
    if "fab" in stages:
        fab.export(out, name, circuit, board_dir / "fab")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
