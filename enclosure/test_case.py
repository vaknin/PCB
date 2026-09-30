"""Negative tests for the fit gate: each breaks the starter's case one way and checks that the
named check catches it (D-025 Phase B). A gate that only ever passes proves nothing.

    enclosure/.venv/bin/python enclosure/test_case.py [boards/starter/case]

Needs board.json and board.step from a `case` run (board.step is gitignored):
    cargo run --release -p starter -- case
Each case takes ~30 s (the exact distances); the whole run ~4 min.
"""

import copy
import json
import sys
import time
from pathlib import Path

import case

ARGS = [a for a in sys.argv[1:] if not a.startswith("--")]
DIR = Path(ARGS[0] if ARGS else Path(__file__).parent.parent / "boards/starter/case")


def failing(fit):
    return {c["name"] for c in fit["checks"] if not c["ok"]}


def lid_too_low(bj, board):
    bj["case"]["top_gap"] = -1.0  # the lid plate's underside 1 mm below the tallest part
    return {"interference.lid"}


def led_opening_shifted(bj, board):
    fp = next(f for f in bj["footprints"] if f["ref"] == "D2")
    fp["x"] += 1.0  # board.json says D2 is 1 mm right of where board.step has it
    return {"opening.D2"}


def connector_cutout_shifted(bj, board):
    fp = next(f for f in bj["footprints"] if f["ref"] == "J2")
    box = fp["fab"] or fp["courtyard"]
    box["min"][1] += 1.0  # J2 faces the right wall: move its body 1 mm along it
    box["max"][1] += 1.0
    return {"opening.J2"}


def part_missing_from_step(bj, board):
    del board.parts["D1"]
    return {"opening.D1"}


def _battery(bj, h, at=None):
    """The capture-clip cell (36 × 17 × h) at the board's centre, with room under the board."""
    (x0, y0), (x1, y1) = bj["bbox"]["min"], bj["bbox"]["max"]
    bj["case"]["bottom_gap"] = 9.0
    bj["case"]["battery"] = {"size": [36.0, 17.0, h], "at": at or [(x0 + x1) / 2, (y0 + y1) / 2], "rotate": False,
                             "swell": 0.1 * h, "pad": 0.5, "lead": "J2", "fence_height": 3.0, "fence_width": 1.2}


def battery_fits(bj, board):
    _battery(bj, 7.8)  # 7.8 + 0.78 swell + 0.2 < 9: passes
    return set()


def battery_too_thick(bj, board):
    _battery(bj, 8.5)  # 8.5 + 0.85 swell reaches the PCB's underside
    return {"battery.board"}


def battery_on_a_standoff(bj, board):
    h = bj["mounting_holes"][0]
    _battery(bj, 7.8, at=[h["x"] + 20 if h["x"] < bj["bbox"]["max"][0] - 25 else h["x"] - 20, h["y"]])
    return {"battery.case"}


CASES = [battery_fits, battery_too_thick, battery_on_a_standoff] if "--battery" in sys.argv else \
    [lid_too_low, led_opening_shifted, connector_cutout_shifted, part_missing_from_step, battery_fits, battery_too_thick, battery_on_a_standoff]


def main():
    bj0 = json.loads((DIR / "board.json").read_text())
    step = DIR / "board.step"
    if not step.exists():
        sys.exit(f"{step} is missing: run the board's `case` stage first")
    board0 = case.Board.load(step)
    bad = 0

    t = time.time()
    fit = case.run(copy.deepcopy(bj0), board0, out=None, render_pngs=False)
    ok = fit["ok"]
    print(f"{'ok  ' if ok else 'FAIL'} unchanged case passes ({time.time() - t:.0f} s){'' if ok else ': ' + ', '.join(sorted(failing(fit))) + ' ' + str(fit['problems'])}")
    bad += not ok

    for fn in CASES:
        bj = copy.deepcopy(bj0)
        board = case.Board(list(board0.substrate), {r: list(s) for r, s in board0.parts.items()})
        want = fn(bj, board)
        t = time.time()
        fit = case.run(bj, board, out=None, render_pngs=False)
        got = failing(fit)
        ok = (not fit["ok"] and want <= got) if want else (fit["ok"] or print("   ", fit["problems"]))
        print(f"{'ok  ' if ok else 'FAIL'} {fn.__name__}: wants {', '.join(sorted(want))} failing; failing: {', '.join(sorted(got)) or 'none'} ({time.time() - t:.0f} s)")
        bad += not ok

    print(f"test_case: {1 + len(CASES) - bad} of {1 + len(CASES)} pass")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
