"""Objective routing report, read straight from the saved .kicad_pcb (no SWIG).

Numbers a reviewer would otherwise eyeball from a render: how much is routed, vias,
track length, power tracks thinner than their net class, copper inside keep-outs
(the antenna area), and how whole the GND pours are. Two items are gates: any
unrouted connection, and any track or via inside a keep-out. The rest is reported
for comparison between runs (reports/routing.json).
"""

from __future__ import annotations

import fnmatch
import json
import math
from pathlib import Path

from .project import BoardRules
from .sexpr import find, find_all, parse

ORIGIN = (100.0, 100.0)   # board top-left on the KiCad page (pcb.ORIGIN), mm


def _xy(node) -> tuple[float, float]:
    return float(node[1]), float(node[2])


def _pts(poly) -> list[tuple[float, float]]:
    return [_xy(p) for p in find_all(find(poly, "pts"), "xy")]


def _area(pts) -> float:
    return abs(sum(x0 * y1 - x1 * y0 for (x0, y0), (x1, y1) in zip(pts, pts[1:] + pts[:1]))) / 2


def _inside(p, poly) -> bool:
    x, y = p
    inside = False
    for (x0, y0), (x1, y1) in zip(poly, poly[1:] + poly[:1]):
        if (y0 > y) != (y1 > y) and x < x0 + (y - y0) * (x1 - x0) / (y1 - y0):
            inside = not inside
    return inside


def _seg_dist(p, a, b) -> float:
    (px, py), (ax, ay), (bx, by) = p, a, b
    dx, dy = bx - ax, by - ay
    t = 0.0 if dx == dy == 0 else max(0.0, min(1.0, ((px - ax) * dx + (py - ay) * dy) / (dx * dx + dy * dy)))
    return math.hypot(px - ax - t * dx, py - ay - t * dy)


def _hits(a, b, r, poly) -> bool:
    """Does a round-ended segment a-b of half-width r touch polygon `poly`?"""
    if _inside(a, poly) or _inside(b, poly):
        return True
    edges = list(zip(poly, poly[1:] + poly[:1]))
    # close enough to an edge, or crossing it: sample the segment at 0.05 mm steps
    n = max(1, int(math.dist(a, b) / 0.05))
    for i in range(n + 1):
        p = (a[0] + (b[0] - a[0]) * i / n, a[1] + (b[1] - a[1]) * i / n)
        if _inside(p, poly) or any(_seg_dist(p, e0, e1) < r for e0, e1 in edges):
            return True
    return False


def _net_class(net: str, rules: BoardRules):
    for nc in rules.classes:
        if any(fnmatch.fnmatchcase(net, pat) for pat in nc.patterns):
            return nc
    return next(nc for nc in rules.classes if nc.name == "Default")


def build(project_dir: Path, name: str, rules: BoardRules, gnd_net: str = "GND") -> dict:
    board = parse((project_dir / f"{name}.kicad_pcb").read_text())
    drc = json.loads((project_dir / "reports" / "drc.json").read_text())

    segs = [(_xy(find(s, "start")), _xy(find(s, "end")), float(find(s, "width")[1]),
             find(s, "layer")[1], find(s, "net")[1] if find(s, "net") else "")
            for s in find_all(board, "segment")]
    vias = [(_xy(find(v, "at")), float(find(v, "size")[1]), find(v, "net")[1] if find(v, "net") else "")
            for v in find_all(board, "via")]

    # keep-outs: board-level rule areas and those inside footprints (antenna, sensor)
    zones = find_all(board, "zone") + [z for fp in find_all(board, "footprint") for z in find_all(fp, "zone")]
    keepouts = []
    for z in zones:
        ko = find(z, "keepout")
        if ko is not None and find(ko, "tracks")[1] == "not_allowed":
            keepouts.append((find(z, "name")[1] if find(z, "name") else "(footprint)", _pts(find(z, "polygon"))))
    in_keepout = []
    for a, b, w, layer, net in segs:
        for kname, poly in keepouts:
            if _hits(a, b, w / 2, poly):
                in_keepout.append(f"track {net} on {layer} at ({a[0] - ORIGIN[0]:.2f}, {a[1] - ORIGIN[1]:.2f}) in {kname}")
    for p, d, net in vias:
        for kname, poly in keepouts:
            if _hits(p, p, d / 2, poly):
                in_keepout.append(f"via {net} at ({p[0] - ORIGIN[0]:.2f}, {p[1] - ORIGIN[1]:.2f}) in {kname}")

    # tracks thinner than their net class asks for (the router may neck down)
    thin: dict[str, float] = {}
    for a, b, w, layer, net in segs:
        nc = _net_class(net, rules)
        if w < nc.track - 1e-6:
            thin[net] = thin.get(net, 0.0) + math.dist(a, b)

    # pours: pieces and filled fraction per GND layer
    pours = {}
    for z in find_all(board, "zone"):
        if find(z, "net") is None or find(z, "net")[1] != gnd_net or find(z, "keepout") is not None:
            continue
        layer = find(z, "layer")[1]
        outline = _area(_pts(find(z, "polygon")))
        pieces = sorted((_area(_pts(fp)) for fp in find_all(z, "filled_polygon")), reverse=True)
        pours[layer] = {
            "pieces": len(pieces),
            "filled_pct": round(100 * sum(pieces) / outline, 1),
            "largest_piece_pct_of_fill": round(100 * pieces[0] / sum(pieces), 1) if pieces else 0.0,
        }

    unconnected = [v for v in drc.get("unconnected_items", [])]
    length = {}
    for a, b, w, layer, net in segs:
        length[layer] = length.get(layer, 0.0) + math.dist(a, b)
    return {
        "unrouted_connections": len(unconnected),
        "track_segments": len(segs),
        "track_length_mm": {k: round(v, 1) for k, v in sorted(length.items())},
        "vias_total": len(vias),
        "vias_gnd": sum(1 for v in vias if v[2] == gnd_net),
        "vias_signal": sum(1 for v in vias if v[2] != gnd_net),
        "copper_in_keepouts": in_keepout,
        "thinner_than_class_mm": {k: round(v, 1) for k, v in sorted(thin.items())},
        "gnd_pours": pours,
    }


def run(project_dir: Path, name: str, rules: BoardRules) -> bool:
    r = build(project_dir, name, rules)
    (project_dir / "reports" / "routing.json").write_text(json.dumps(r, indent=2) + "\n")
    print(f"== ROUTING: {r['unrouted_connections']} unrouted, {r['vias_signal']} signal vias + "
          f"{r['vias_gnd']} GND vias, track length {r['track_length_mm']} mm")
    for layer, p in r["gnd_pours"].items():
        print(f"   GND pour {layer}: {p['filled_pct']}% of the board filled, {p['pieces']} piece(s), "
              f"largest {p['largest_piece_pct_of_fill']}% of it")
    for net, mm_ in r["thinner_than_class_mm"].items():
        print(f"   note: {mm_} mm of {net} track is thinner than its net class")
    for item in r["copper_in_keepouts"]:
        print(f"   OPEN:   copper in keep-out: {item}")
    ok = r["unrouted_connections"] == 0 and not r["copper_in_keepouts"]
    print(f"== ROUTING: {'PASS' if ok else 'FAIL'} (report: reports/routing.json)")
    return ok
