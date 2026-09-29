"""Autorouting with Freerouting via Specctra DSN/SES (SWIG pcbnew; kicad-cli can't do DSN).

Freerouting 2.4.1's Linux bundle carries its own Java 25 runtime, so nothing is
installed system-wide. Fetch it with scripts/fetch-tools.sh.
"""

from __future__ import annotations

import json
import re
import subprocess
import sys
import time
from dataclasses import asdict, dataclass
from pathlib import Path

from .kicad import borrowed, fill_zones, pcbnew, run_step

ROOT = Path(__file__).resolve().parent.parent
FREEROUTING = ROOT / "tools" / "freerouting-2.4.1-linux-x64" / "bin" / "freerouting"


@dataclass
class RouteOptions:
    max_passes: int = 100
    edge_clearance_um: int = 500     # KiCad's DSN export omits board-edge clearance
    via_costs: int = 50
    timeout_s: int = 900
    tries: int = 3                    # Freerouting varies run to run; retry while any is unrouted
    stitch_net: str | None = "GND"   # add vias tying this net's pours together after routing
    stitch_pitch: float = 3.0         # mm grid for stitching vias
    stitch_via: tuple[float, float] = (0.6, 0.3)   # diameter, drill (mm)
    stitch_clearance: float = 0.25    # mm from a stitching via to other-net tracks


def route(pcb_path: Path, opts: RouteOptions) -> Path:
    """Export DSN, run Freerouting, import SES, fill, stitch, fill.

    Each pcbnew step runs in its own short-lived process (`run_step`) and zones are
    filled by kicad-cli, so a crash in KiCad's C++ reads as "step X crashed" and cannot
    corrupt the steps after it.
    """
    if not FREEROUTING.exists():
        raise FileNotFoundError(f"{FREEROUTING} missing; run scripts/fetch-tools.sh")
    work = pcb_path.parent / "route"
    work.mkdir(exist_ok=True)
    dsn, ses, log = work / "board.dsn", work / "board.ses", work / "freerouting.log"

    run_step("pcbgen.route", "export-dsn", str(pcb_path), str(dsn))
    cmd = [str(FREEROUTING), "-de", str(dsn), "-do", str(ses), "-mp", str(opts.max_passes),
           "--gui.enabled=false",
           f"--router.copperToEdgeClearanceUm={opts.edge_clearance_um}",
           f"--router.via_costs={opts.via_costs}",
           "--usage_and_diagnostic_data.disable_analytics=true",
           f"--user_data_path={work / 'fr-data'}"]
    for attempt in range(1, opts.tries + 1):
        if ses.exists():
            ses.unlink()
        t0 = time.monotonic()
        with open(log, "w") as fh:
            res = subprocess.run(cmd, stdout=fh, stderr=subprocess.STDOUT, timeout=opts.timeout_s)
        if not ses.exists():
            raise RuntimeError(f"Freerouting produced no session file; see {log}")
        unrouted = _unrouted(log, opts.stitch_net)
        print(f"freerouting: try {attempt}, exit {res.returncode} in {time.monotonic() - t0:.0f}s, "
              f"unrouted: {', '.join(unrouted) or 'none'} (excluding the pour net), log {log}")
        if not unrouted:
            break

    run_step("pcbgen.route", "import-ses", str(pcb_path), str(ses))
    fill_zones(pcb_path)
    if opts.stitch_net:
        run_step("pcbgen.route", "stitch", str(pcb_path), json.dumps(asdict(opts)))
        fill_zones(pcb_path)
    return pcb_path


def _export_dsn(pcb_path: Path, dsn: Path) -> None:
    board = pcbnew.LoadBoard(str(pcb_path))
    # drop earlier routing so re-runs start clean; locked tracks (escape stubs) stay.
    # pcbnew's Remove() hands each track to Python, which then frees it, but KiCad still
    # holds pointers to it (connectivity, DSN export): the heap gets corrupted and Python
    # segfaults later somewhere unrelated. So removed tracks are leaked on purpose (this
    # process is short-lived), and connectivity is rebuilt without them.
    for t in list(board.GetTracks()):
        if not t.IsLocked():
            board.Remove(t)
            t.thisown = False
    board.BuildConnectivity()
    pcbnew.SaveBoard(str(pcb_path), board)
    # Hide the copper pours from the router: exported as planes, their nets count as
    # connected and Freerouting routes none of their pads. Tracks then cut the pour
    # into pieces, stranding pads (DRC unconnected_items). Without planes every GND pad
    # gets a real track; pours and stitching come on top. Not saved: DSN only.
    for z in list(board.Zones()):
        if not z.GetIsRuleArea():
            board.Remove(z)
            z.thisown = False
    board.BuildConnectivity()
    if not pcbnew.ExportSpecctraDSN(board, str(dsn)):
        raise RuntimeError("DSN export failed")


def _import_ses(pcb_path: Path, ses: Path) -> None:
    board = pcbnew.LoadBoard(str(pcb_path))
    if not pcbnew.ImportSpecctraSES(board, str(ses)):
        raise RuntimeError("SES import failed")
    pcbnew.SaveBoard(str(pcb_path), board)
    n_tracks = sum(1 for t in board.GetTracks() if t.GetClass() == "PCB_TRACK")
    n_vias = sum(1 for t in board.GetTracks() if t.GetClass() == "PCB_VIA")
    print(f"routed: {n_tracks} track segments, {n_vias} vias")


def _stitch(pcb_path: Path, opts: RouteOptions) -> None:
    board = pcbnew.LoadBoard(str(pcb_path))   # zones come filled by kicad-cli
    n = stitch(board, opts)
    pcbnew.SaveBoard(str(pcb_path), board)
    print(f"stitching: {n} {opts.stitch_net} vias")


def _unrouted(log: Path, pour_net: str | None) -> list[str]:
    """Nets Freerouting left unrouted, except the pour net (its pour and stitching vias
    finish those; DRC checks). ["?"] if the log has no final score."""
    text = log.read_text()
    if "final score:" not in text:
        return ["?"]
    return [n for n in re.findall(r"Net '(.+)' \(\d+ unrouted", text) if n != pour_net]


def stitch(board: pcbnew.BOARD, opts: RouteOptions) -> int:
    """Drop vias on a grid wherever one fits entirely inside the net's filled pour on
    both outer layers, outside every courtyard and clear of other holes.

    Freerouting doesn't stitch, and a pour cut in two by tracks leaves an isolated piece
    (DRC: unconnected zone). Staying inside the *filled* copper means the fill has already
    kept each via clear of other nets; staying outside courtyards keeps vias out of pads.
    """
    mm = pcbnew.FromMM
    zones = [z for z in board.Zones() if not z.GetIsRuleArea() and z.GetNetname() == opts.stitch_net]
    net = board.FindNet(opts.stitch_net)
    dia, drill = (mm(v) for v in opts.stitch_via)
    per_layer: dict[int, list] = {}
    for z in zones:
        layer = z.GetFirstLayer()
        poly = borrowed(z.GetFilledPolysList(layer)).CloneDropTriangulation()
        poly.Deflate(dia // 2 + mm(0.05), pcbnew.CORNER_STRATEGY_ROUND_ALL_CORNERS, mm(0.005))
        per_layer.setdefault(layer, []).append(poly)
    if net is None or not {pcbnew.F_Cu, pcbnew.B_Cu} <= per_layer.keys():
        return 0
    courtyards = []
    for fp in board.GetFootprints():
        fp.BuildCourtyardCaches()
        # copies: the caches belong to the footprint
        courtyards += [pcbnew.SHAPE_POLY_SET(borrowed(fp.GetCourtyard(layer)))
                       for layer in (pcbnew.F_CrtYd, pcbnew.B_CrtYd)]
    holes = [(t.GetPosition(), t.GetDrillValue() // 2) for t in board.GetTracks() if t.GetClass() == "PCB_VIA"]
    for fp in board.GetFootprints():
        for pad in fp.Pads():
            if pad.HasHole():
                holes.append((pad.GetPosition(), max(pad.GetDrillSize().x, pad.GetDrillSize().y) // 2))
    hole_gap = mm(0.5)

    pads = [pad for fp in board.GetFootprints() for pad in fp.Pads()]
    pad_gap = dia // 2 + mm(0.2)
    # the fill alone didn't keep one via clear of a track (0.185 mm to /EN, rule 0.2):
    # check other-net copper directly, at the largest class clearance plus a margin
    others = [t for t in board.GetTracks() if t.GetNetname() != opts.stitch_net]
    track_gap = dia // 2 + mm(opts.stitch_clearance)

    def fits(p: pcbnew.VECTOR2I, in_courtyard: bool = False) -> bool:
        # in_courtyard: allowed inside a part's courtyard if clear of all its pads
        # (a tented via under a part body is fine; a via in a pad wicks solder)
        return (all(any(poly.Contains(p) for poly in polys) for polys in per_layer.values())
                and (not any(c.Collide(p, dia // 2) for c in courtyards) if not in_courtyard
                     else not any(pad.HitTest(p, pad_gap) for pad in pads))
                and not any((p - hp).EuclideanNorm() < hr + drill // 2 + hole_gap for hp, hr in holes)
                and not any(t.HitTest(p, track_gap) for t in others))

    def add(p: pcbnew.VECTOR2I) -> None:
        v = pcbnew.PCB_VIA(board)
        v.SetPosition(p)
        v.SetWidth(dia)
        v.SetDrill(drill)
        v.SetNet(net)
        board.Add(v)
        holes.append((p, drill // 2))

    def grid(x0, y0, x1, y1, step):
        for x in range(x0 + step // 2, x1, step):
            for y in range(y0 + step // 2, y1, step):
                yield pcbnew.VECTOR2I(x, y)

    bb = board.GetBoardEdgesBoundingBox()
    added = 0
    for p in grid(bb.GetLeft(), bb.GetTop(), bb.GetRight(), bb.GetBottom(), mm(opts.stitch_pitch)):
        if fits(p):
            add(p)
            added += 1

    # Pour pieces the grid missed (narrow slivers between tracks): one via each, found
    # by a fine search inside that piece, nearest its centre.
    for z in zones:
        # copies: the zone's own fill buffers must not be held across board edits
        filled = borrowed(z.GetFilledPolysList(z.GetFirstLayer())).CloneDropTriangulation()
        for i in range(filled.OutlineCount()):
            piece = pcbnew.SHAPE_LINE_CHAIN(borrowed(filled.COutline(i)))
            if any(piece.PointInside(hp) for hp, _ in holes):
                continue
            pbb = piece.BBox()
            centre = pbb.GetCenter()
            spots = sorted((p for p in grid(pbb.GetLeft(), pbb.GetTop(), pbb.GetRight(),
                                            pbb.GetBottom(), mm(0.25)) if piece.PointInside(p)),
                           key=lambda p: (p - centre).EuclideanNorm())
            spot = (next((p for p in spots if fits(p)), None)
                    or next((p for p in spots if fits(p, in_courtyard=True)), None))
            if spot is None:
                print(f"stitching: no room for a via in a {z.GetZoneName()} piece near "
                      f"({pcbnew.ToMM(centre.x) - 100:.1f}, {pcbnew.ToMM(centre.y) - 100:.1f}) mm")
                continue
            add(spot)
            added += 1
    return added


if __name__ == "__main__":
    # one pcbnew step per process; see route() and kicad.run_step
    step, pcb_arg, arg = sys.argv[1:4]
    if step == "export-dsn":
        _export_dsn(Path(pcb_arg), Path(arg))
    elif step == "import-ses":
        _import_ses(Path(pcb_arg), Path(arg))
    elif step == "stitch":
        o = json.loads(arg)
        o["stitch_via"] = tuple(o["stitch_via"])
        _stitch(Path(pcb_arg), RouteOptions(**o))
    else:
        sys.exit(f"unknown step {step}")
