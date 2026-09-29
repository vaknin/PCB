"""Autorouting with Freerouting via Specctra DSN/SES (SWIG pcbnew; kicad-cli can't do DSN).

Freerouting 2.4.1's Linux bundle carries its own Java 25 runtime, so nothing is
installed system-wide. Fetch it with scripts/fetch-tools.sh.
"""

from __future__ import annotations

import subprocess
import time
from dataclasses import dataclass
from pathlib import Path

from .kicad import pcbnew

ROOT = Path(__file__).resolve().parent.parent
FREEROUTING = ROOT / "tools" / "freerouting-2.4.1-linux-x64" / "bin" / "freerouting"


@dataclass
class RouteOptions:
    max_passes: int = 100
    edge_clearance_um: int = 500     # KiCad's DSN export omits board-edge clearance
    via_costs: int = 50
    timeout_s: int = 900
    stitch_net: str | None = "GND"   # add vias tying this net's pours together after routing
    stitch_pitch: float = 3.0         # mm grid for stitching vias
    stitch_via: tuple[float, float] = (0.6, 0.3)   # diameter, drill (mm)


def route(pcb_path: Path, opts: RouteOptions) -> Path:
    if not FREEROUTING.exists():
        raise FileNotFoundError(f"{FREEROUTING} missing; run scripts/fetch-tools.sh")
    work = pcb_path.parent / "route"
    work.mkdir(exist_ok=True)
    dsn, ses, log = work / "board.dsn", work / "board.ses", work / "freerouting.log"

    board = pcbnew.LoadBoard(str(pcb_path))
    # drop earlier routing so re-runs start clean; locked tracks (escape stubs) stay
    for t in list(board.GetTracks()):
        if not t.IsLocked():
            board.Remove(t)
    if not pcbnew.ExportSpecctraDSN(board, str(dsn)):
        raise RuntimeError("DSN export failed")
    if ses.exists():
        ses.unlink()

    cmd = [str(FREEROUTING), "-de", str(dsn), "-do", str(ses), "-mp", str(opts.max_passes),
           "--gui.enabled=false",
           f"--router.copperToEdgeClearanceUm={opts.edge_clearance_um}",
           f"--router.via_costs={opts.via_costs}",
           "--usage_and_diagnostic_data.disable_analytics=true",
           f"--user_data_path={work / 'fr-data'}"]
    t0 = time.monotonic()
    with open(log, "w") as fh:
        res = subprocess.run(cmd, stdout=fh, stderr=subprocess.STDOUT, timeout=opts.timeout_s)
    print(f"freerouting: exit {res.returncode} in {time.monotonic() - t0:.0f}s, log {log}")
    if not ses.exists():
        raise RuntimeError(f"Freerouting produced no session file; see {log}")

    if not pcbnew.ImportSpecctraSES(board, str(ses)):
        raise RuntimeError("SES import failed")
    pcbnew.ZONE_FILLER(board).Fill(board.Zones())
    if opts.stitch_net:
        n = stitch(board, opts)
        pcbnew.ZONE_FILLER(board).Fill(board.Zones())
        print(f"stitching: {n} {opts.stitch_net} vias")
    pcbnew.SaveBoard(str(pcb_path), board)
    n_tracks = sum(1 for t in board.GetTracks() if t.GetClass() == "PCB_TRACK")
    n_vias = sum(1 for t in board.GetTracks() if t.GetClass() == "PCB_VIA")
    print(f"routed: {n_tracks} track segments, {n_vias} vias")
    return pcb_path


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
        poly = z.GetFilledPolysList(layer).CloneDropTriangulation()
        poly.Deflate(dia // 2 + mm(0.05), pcbnew.CORNER_STRATEGY_ROUND_ALL_CORNERS, mm(0.005))
        per_layer.setdefault(layer, []).append(poly)
    if net is None or not {pcbnew.F_Cu, pcbnew.B_Cu} <= per_layer.keys():
        return 0
    courtyards = []
    for fp in board.GetFootprints():
        fp.BuildCourtyardCaches()
        courtyards += [fp.GetCourtyard(pcbnew.F_CrtYd), fp.GetCourtyard(pcbnew.B_CrtYd)]
    holes = [(t.GetPosition(), t.GetDrillValue() // 2) for t in board.GetTracks() if t.GetClass() == "PCB_VIA"]
    for fp in board.GetFootprints():
        for pad in fp.Pads():
            if pad.HasHole():
                holes.append((pad.GetPosition(), max(pad.GetDrillSize().x, pad.GetDrillSize().y) // 2))
    hole_gap = mm(0.5)

    def fits(p: pcbnew.VECTOR2I) -> bool:
        return (all(any(poly.Contains(p) for poly in polys) for polys in per_layer.values())
                and not any(c.Collide(p, dia // 2) for c in courtyards)
                and not any((p - hp).EuclideanNorm() < hr + drill // 2 + hole_gap for hp, hr in holes))

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
        filled = z.GetFilledPolysList(z.GetFirstLayer()).CloneDropTriangulation()
        for i in range(filled.OutlineCount()):
            piece = pcbnew.SHAPE_LINE_CHAIN(filled.COutline(i))
            if any(piece.PointInside(hp) for hp, _ in holes):
                continue
            pbb = piece.BBox()
            centre = pbb.GetCenter()
            spots = sorted((p for p in grid(pbb.GetLeft(), pbb.GetTop(), pbb.GetRight(),
                                            pbb.GetBottom(), mm(0.25)) if piece.PointInside(p)),
                           key=lambda p: (p - centre).EuclideanNorm())
            spot = next((p for p in spots if fits(p)), None)
            if spot is None:
                print(f"stitching: no room for a via in a {z.GetZoneName()} piece near "
                      f"({pcbnew.ToMM(centre.x) - 100:.1f}, {pcbnew.ToMM(centre.y) - 100:.1f}) mm")
                continue
            add(spot)
            added += 1
    return added
