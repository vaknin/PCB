"""Pipeline driver: python -m pcbgen <board_dir> [stage ...]

Stages run in order: sch, pcb, route, check, fab (default: all).
A board directory holds circuit.py (build() -> Circuit) and layout.py (SPEC: BoardSpec,
RULES: BoardRules); generated KiCad files go to <board_dir>/kicad/.
"""

from __future__ import annotations

import importlib.util
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
    sys.path.insert(0, str(board_dir))
    circuit = _load(board_dir / "circuit.py").build()
    layout = _load(board_dir / "layout.py")
    out = board_dir / "kicad"
    name = circuit.name

    from . import gates, pcb, project, route, schematic, fab
    if "sch" in stages:
        project.write(out, name, layout.RULES)
        print("schematic:", schematic.write(circuit, out))
    if "pcb" in stages:
        print("pcb:", pcb.build(out, name, layout.SPEC))
    if "route" in stages:
        route.route(out / f"{name}.kicad_pcb", layout.ROUTE)
    if "check" in stages:
        ok = gates.run(out, name, getattr(layout, "WAIVERS", ()))
        if not ok:
            return 1
    if "fab" in stages:
        fab.export(out, name, board_dir / "fab")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
