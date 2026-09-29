"""Load symbols from KiCad `.kicad_sym` libraries, flattening `extends`.

Coordinates returned here are in *library* space (mm, Y up), exactly as stored.
"""

from __future__ import annotations

import copy
import os
from dataclasses import dataclass
from functools import cache
from pathlib import Path

from .sexpr import Sym, find, find_all, parse

SYMBOL_DIRS = [
    Path(os.environ.get("KICAD10_SYMBOL_DIR", "/usr/share/kicad/symbols")),
    Path(__file__).resolve().parent.parent / "lib" / "symbols",
]


@dataclass(frozen=True)
class Pin:
    number: str
    name: str
    etype: str          # input, output, bidirectional, power_in, power_out, passive, no_connect, ...
    x: float            # connection point, library space
    y: float
    angle: int          # direction from connection point into the body
    hidden: bool
    unit: int


@dataclass
class Symbol:
    lib_id: str
    node: list          # flattened, renamed "Lib:Name", ready to embed in lib_symbols
    pins: list[Pin]
    bbox: tuple[float, float, float, float]  # xmin, ymin, xmax, ymax (library space, body + pins)
    is_power: bool

    def prop(self, key: str) -> str | None:
        for p in find_all(self.node, "property"):
            if p[1] == key:
                return p[2]
        return None

    def prop_at(self, key: str) -> tuple[float, float, float]:
        for p in find_all(self.node, "property"):
            if p[1] == key:
                at = find(p, "at")
                return float(at[1]), float(at[2]), float(at[3]) if len(at) > 3 else 0.0
        return 0.0, 0.0, 0.0


@cache
def _library(lib: str) -> dict[str, list]:
    for d in SYMBOL_DIRS:
        path = d / f"{lib}.kicad_sym"
        if path.exists():
            root = parse(path.read_text())
            return {s[1]: s for s in find_all(root, "symbol")}
    raise FileNotFoundError(f"symbol library {lib!r} not found in {SYMBOL_DIRS}")


def _flatten(lib: str, name: str) -> list:
    raw = _library(lib)[name]
    ext = find(raw, "extends")
    if not ext:
        return copy.deepcopy(raw)
    parent = _flatten(lib, ext[1])
    pname = parent[1]
    out = [Sym("symbol"), name]
    for c in parent[2:]:
        if not isinstance(c, list):
            continue
        if c[0] == "property":
            continue
        if c[0] == "symbol":
            sub = copy.deepcopy(c)
            assert sub[1].startswith(pname + "_"), sub[1]
            sub[1] = name + sub[1][len(pname):]
            out.append(sub)
        else:
            out.append(copy.deepcopy(c))
    # child's own flags and properties win; insert them before the sub-symbols
    child_items = [copy.deepcopy(c) for c in raw[2:] if isinstance(c, list) and c[0] != "extends"]
    child_keys = {c[0] for c in child_items if c[0] != "property"}
    out = [c for c in out if not (isinstance(c, list) and c[0] in child_keys)]
    first_sub = next(i for i, c in enumerate(out) if isinstance(c, list) and c[0] == "symbol")
    return out[:first_sub] + child_items + out[first_sub:]


def _unit_style(subname: str) -> tuple[int, int]:
    _, u, b = subname.rsplit("_", 2)
    return int(u), int(b)


@cache
def load(lib_id: str) -> Symbol:
    lib, name = lib_id.split(":", 1)
    node = _flatten(lib, name)
    node[1] = lib_id
    pins: list[Pin] = []
    xs: list[float] = []
    ys: list[float] = []
    for sub in find_all(node, "symbol"):
        unit, style = _unit_style(sub[1])
        if style not in (0, 1):
            continue  # De Morgan alternates
        for g in sub[2:]:
            if not isinstance(g, list):
                continue
            kind = g[0]
            if kind == "pin":
                at = find(g, "at")
                x, y, a = float(at[1]), float(at[2]), int(float(at[3]))
                length = float(find(g, "length")[1])
                hidden = Sym("hide") in g or (find(g, "hide") is not None and find(g, "hide")[1] == "yes")
                pins.append(Pin(find(g, "number")[1], find(g, "name")[1], str(g[1]), x, y, a, hidden, unit))
                xs.append(x); ys.append(y)
                dx, dy = {0: (1, 0), 90: (0, 1), 180: (-1, 0), 270: (0, -1)}[a]
                xs.append(x + dx * length); ys.append(y + dy * length)
            elif kind == "rectangle":
                for k in ("start", "end"):
                    p = find(g, k); xs.append(float(p[1])); ys.append(float(p[2]))
            elif kind in ("polyline", "bezier"):
                for p in find_all(find(g, "pts"), "xy"):
                    xs.append(float(p[1])); ys.append(float(p[2]))
            elif kind == "circle":
                c = find(g, "center"); r = float(find(g, "radius")[1])
                xs += [float(c[1]) - r, float(c[1]) + r]; ys += [float(c[2]) - r, float(c[2]) + r]
            elif kind == "arc":
                for k in ("start", "mid", "end"):
                    p = find(g, k); xs.append(float(p[1])); ys.append(float(p[2]))
    bbox = (min(xs), min(ys), max(xs), max(ys)) if xs else (-1.27, -1.27, 1.27, 1.27)
    is_power = find(node, "power") is not None
    units = {p.unit for p in pins if p.unit}
    if len(units) > 1:
        raise NotImplementedError(f"{lib_id}: multi-unit symbols not supported yet ({units})")
    return Symbol(lib_id, node, pins, bbox, is_power)
