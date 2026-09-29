"""Build a .kicad_pcb from the schematic's netlist, with placement given in code.

The netlist is exported from the schematic by kicad-cli, so the PCB is always
derived from exactly what ERC checked, and every footprint carries its symbol's
UUID path (DRC's schematic-parity check relies on that).
"""

from __future__ import annotations

import math
import os
from dataclasses import dataclass, field
from pathlib import Path

from .gates import export_netlist
from .kicad import borrowed, pcbnew
from .sexpr import find, find_all, parse

ORIGIN = (100.0, 100.0)   # board top-left on the KiCad page, mm


@dataclass
class Place:
    x: float                  # footprint anchor, mm from board top-left, Y down
    y: float
    rot: float = 0.0          # degrees CCW as seen from the top
    side: str = "top"         # "top" | "bottom"


@dataclass
class Keepout:
    """Rule area: no copper of any kind (antenna clearance)."""
    x0: float
    y0: float
    x1: float
    y1: float
    name: str = "keepout"


@dataclass
class CopperZone:
    """A local copper pour of one net (e.g. cooling copper under a regulator tab).

    Filled above the GND pours (higher priority). The router never sees it (route.py
    hides every pour), so tracks may cross the area and the fill flows around them;
    vias tying its layers together come from `RouteOptions.stitch_local`.
    """
    net: str
    x0: float
    y0: float
    x1: float
    y1: float
    layers: tuple[str, ...] = ("F.Cu", "B.Cu")
    priority: int = 10        # the GND pours use 0 and 1


@dataclass
class Text:
    text: str
    x: float
    y: float
    size: float = 1.2
    layer: str = "F.SilkS"
    rot: float = 0.0


@dataclass
class BoardSpec:
    width: float
    height: float
    corner_radius: float = 1.0
    places: dict[str, Place] = field(default_factory=dict)
    keepouts: list[Keepout] = field(default_factory=list)
    zones: list[CopperZone] = field(default_factory=list)
    texts: list[Text] = field(default_factory=list)
    gnd_net: str = "GND"
    gnd_layers: tuple[str, ...] = ("F.Cu", "B.Cu")
    # Reference designators go to the fab (assembly) layer, except these; owner-facing
    # labels are given as `texts` instead, so the silkscreen stays readable.
    silk_refs: tuple[str, ...] = ()


def mm(v: float) -> int:
    return pcbnew.FromMM(v)


def pt(x: float, y: float) -> pcbnew.VECTOR2I:
    return pcbnew.VECTOR2I(mm(ORIGIN[0] + x), mm(ORIGIN[1] + y))


def _fp_lib_paths(project_dir: Path) -> dict[str, Path]:
    env = {"KICAD10_FOOTPRINT_DIR": os.environ.get("KICAD10_FOOTPRINT_DIR", "/usr/share/kicad/footprints"),
           "KIPRJMOD": str(project_dir)}
    libs = {}
    for lib in find_all(parse((project_dir / "fp-lib-table").read_text()), "lib"):
        uri = find(lib, "uri")[1]
        for k, v in env.items():
            uri = uri.replace("${" + k + "}", v)
        libs[find(lib, "name")[1]] = Path(uri)
    return libs


def _outline(board: pcbnew.BOARD, w: float, h: float, r: float) -> None:
    def seg(a, b):
        s = pcbnew.PCB_SHAPE(board, pcbnew.SHAPE_T_SEGMENT)
        s.SetLayer(pcbnew.Edge_Cuts); s.SetWidth(mm(0.1))
        s.SetStart(pt(*a)); s.SetEnd(pt(*b)); board.Add(s)

    def arc(start, mid, end):
        s = pcbnew.PCB_SHAPE(board, pcbnew.SHAPE_T_ARC)
        s.SetLayer(pcbnew.Edge_Cuts); s.SetWidth(mm(0.1))
        s.SetArcGeometry(pt(*start), pt(*mid), pt(*end)); board.Add(s)

    k = r * (1 - 0.70710678)
    seg((r, 0), (w - r, 0)); seg((w, r), (w, h - r)); seg((w - r, h), (r, h)); seg((0, h - r), (0, r))
    if r > 0:
        arc((w - r, 0), (w - k, k), (w, r))
        arc((w, h - r), (w - k, h - k), (w - r, h))
        arc((r, h), (k, h - k), (0, h - r))
        arc((0, r), (k, k), (r, 0))


def _rect_poly(x0, y0, x1, y1) -> pcbnew.SHAPE_POLY_SET:
    poly = pcbnew.SHAPE_POLY_SET()
    poly.NewOutline()
    for x, y in ((x0, y0), (x1, y0), (x1, y1), (x0, y1)):
        poly.Append(pt(x, y))
    return poly


def _layer_set(*names: str) -> pcbnew.LSET:
    s = pcbnew.LSET()
    for n in names:
        s.AddLayer(getattr(pcbnew, n.replace(".", "_")))
    return s


def _escape_stubs(board: pcbnew.BOARD, fp: pcbnew.FOOTPRINT, reach: float = 0.5) -> None:
    """Short locked tracks leading connected pads out of the footprint's own keep-out.

    Some footprints (e.g. Sensirion's DFN, "no copper under the sensor") carry a keep-out
    that leaves only a pad-sized notch. Freerouting always aims for pad centres and can't
    fit a track plus clearance into the notch, so it gives up. The stub runs from the pad
    centre along the pad's long axis, away from the footprint centre, to `reach` mm past
    the pad's outer end; the router then connects to its free end. Stubs are locked so the
    route stage keeps them.
    """
    keepouts = [z for z in fp.Zones() if z.GetIsRuleArea() and z.GetDoNotAllowTracks()]
    if not keepouts:
        return
    a = math.radians(fp.GetOrientation().AsDegrees())
    cos, sin = math.cos(a), math.sin(a)
    for pad in fp.Pads():
        net = pad.GetNet()
        if not pad.IsOnLayer(pcbnew.F_Cu) or net is None or net.GetNetCode() <= 0 \
                or net.GetNetname().startswith("unconnected-"):
            continue
        if not any(borrowed(z.Outline()).Collide(pad.GetPosition(), mm(0.2)) for z in keepouts):
            continue
        # pad geometry in the footprint's own frame (pads rotated by 90 deg swap axes)
        local = pad.GetFPRelativePosition()
        size = pad.GetSize(pcbnew.F_Cu)
        sx, sy = size.x, size.y
        if round(pad.GetFPRelativeOrientation().AsDegrees()) % 180 == 90:
            sx, sy = sy, sx
        if sx >= sy:
            d, length, short = (1 if local.x > 0 else -1, 0), sx, sy
        else:
            d, length, short = (0, 1 if local.y > 0 else -1), sy, sx
        ex = local.x + d[0] * (length / 2 + mm(reach))
        ey = local.y + d[1] * (length / 2 + mm(reach))
        # footprint frame -> board: KiCad angles are CCW on screen, with Y pointing down
        end = pcbnew.VECTOR2I(int(round(ex * cos + ey * sin)), int(round(-ex * sin + ey * cos)))
        t = pcbnew.PCB_TRACK(board)
        t.SetLayer(pcbnew.F_Cu)
        t.SetWidth(max(mm(0.15), int(short * 2 / 3)))
        t.SetStart(pad.GetPosition())
        t.SetEnd(fp.GetPosition() + end)
        t.SetNet(net)
        t.SetLocked(True)
        board.Add(t)


def build(project_dir: Path, name: str, spec: BoardSpec) -> Path:
    sch = project_dir / f"{name}.kicad_sch"
    pcb_path = project_dir / f"{name}.kicad_pcb"
    net_tree = export_netlist(sch)
    libs = _fp_lib_paths(project_dir)

    board = pcbnew.NewBoard(str(pcb_path))
    board.SetCopperLayerCount(2)

    nets: dict[str, pcbnew.NETINFO_ITEM] = {}
    pad_net: dict[tuple[str, str], str] = {}
    for n in find_all(find(net_tree, "nets"), "net"):
        # "unconnected-(...)" nets are kept: schematic parity expects no-connect pads to carry them
        nname = find(n, "name")[1]
        ni = pcbnew.NETINFO_ITEM(board, nname)
        board.Add(ni)
        nets[nname] = ni
        for node in find_all(n, "node"):
            pad_net[(find(node, "ref")[1], find(node, "pin")[1])] = nname

    missing = []
    for comp in find_all(find(net_tree, "components"), "comp"):
        ref = find(comp, "ref")[1]
        fpid = find(comp, "footprint")
        if not fpid or not fpid[1]:
            raise ValueError(f"{ref} has no footprint")
        lib, fpname = fpid[1].split(":")
        fp = pcbnew.FootprintLoad(str(libs[lib]), fpname)
        if fp is None:
            raise FileNotFoundError(f"{ref}: footprint {fpid[1]} not found in {libs[lib]}")
        fp.SetFPIDAsString(fpid[1])
        fp.SetReference(ref)
        fp.SetValue(find(comp, "value")[1])
        uuid = find(comp, "tstamps")[1]
        fp.SetPath(pcbnew.KIID_PATH(f"/{uuid}"))
        fields = find(comp, "fields")
        for f in find_all(fields, "field") if fields else []:
            fname = find(f, "name")[1]
            fval = f[2] if len(f) > 2 and isinstance(f[2], str) else ""
            if fname == "Footprint" or not fval:
                continue
            fp.SetField(fname, fval)
            fp.GetField(fname).SetVisible(False)
        props = {find(p, "name")[1]: (find(p, "value") or [None, ""])[1] for p in find_all(comp, "property")}
        fp.SetSheetname(props.get("Sheetname", ""))
        fp.SetSheetfile(props.get("Sheetfile", sch.name))
        attrs = fp.GetAttributes()
        if "exclude_from_bom" in props:
            attrs |= pcbnew.FP_EXCLUDE_FROM_BOM
        if "dnp" in props:
            attrs |= pcbnew.FP_DNP
        fp.SetAttributes(attrs)
        for pad in fp.Pads():
            key = (ref, pad.GetNumber())
            if key in pad_net:
                pad.SetNet(nets[pad_net[key]])
        place = spec.places.get(ref)
        if place is None:
            missing.append(ref)
            place = Place(-20, 0)
        fp.SetPosition(pt(place.x, place.y))
        if place.side == "bottom":
            fp.Flip(fp.GetPosition(), pcbnew.FLIP_DIRECTION_LEFT_RIGHT)
        fp.SetOrientationDegrees(place.rot)
        if ref not in spec.silk_refs:
            fp.Reference().SetLayer(pcbnew.F_Fab if place.side == "top" else pcbnew.B_Fab)
        board.Add(fp)
        _escape_stubs(board, fp)
    if missing:
        raise ValueError(f"no placement given for: {', '.join(sorted(missing))}")

    _outline(board, spec.width, spec.height, spec.corner_radius)

    for ko in spec.keepouts:
        z = pcbnew.ZONE(board)
        z.SetIsRuleArea(True)
        z.SetLayerSet(_layer_set("F.Cu", "B.Cu"))
        z.SetDoNotAllowTracks(True); z.SetDoNotAllowVias(True); z.SetDoNotAllowPads(False)
        z.SetDoNotAllowZoneFills(True); z.SetDoNotAllowFootprints(False)
        z.SetZoneName(ko.name)
        borrowed(z.Outline()).Append(_rect_poly(ko.x0, ko.y0, ko.x1, ko.y1))
        board.Add(z)

    if spec.gnd_net in nets:
        for i, layer in enumerate(spec.gnd_layers):
            z = pcbnew.ZONE(board)
            z.SetLayer(getattr(pcbnew, layer.replace(".", "_")))
            z.SetNet(nets[spec.gnd_net])
            z.SetZoneName(f"GND_{layer}")
            z.SetLocalClearance(mm(0.3))
            z.SetMinThickness(mm(0.25))
            # SMD pads solid (reflowed by the fab), thermal reliefs only on through-hole pads
            z.SetPadConnection(pcbnew.ZONE_CONNECTION_THT_THERMAL)
            z.SetThermalReliefGap(mm(0.3))
            z.SetThermalReliefSpokeWidth(mm(0.4))
            z.SetAssignedPriority(i)
            borrowed(z.Outline()).Append(_rect_poly(0, 0, spec.width, spec.height))
            board.Add(z)

    for cz in spec.zones:
        if cz.net not in nets:
            raise ValueError(f"copper zone net {cz.net} is not in the netlist")
        for layer in cz.layers:
            z = pcbnew.ZONE(board)
            z.SetLayer(getattr(pcbnew, layer.replace(".", "_")))
            z.SetNet(nets[cz.net])
            z.SetZoneName(f"{cz.net}_{layer}")
            z.SetLocalClearance(mm(0.3))
            z.SetMinThickness(mm(0.25))
            z.SetPadConnection(pcbnew.ZONE_CONNECTION_THT_THERMAL)   # as the GND pours
            z.SetThermalReliefGap(mm(0.3))
            z.SetThermalReliefSpokeWidth(mm(0.4))
            z.SetAssignedPriority(cz.priority)
            borrowed(z.Outline()).Append(_rect_poly(cz.x0, cz.y0, cz.x1, cz.y1))
            board.Add(z)

    for t in spec.texts:
        tx = pcbnew.PCB_TEXT(board)
        tx.SetText(t.text)
        tx.SetLayer(getattr(pcbnew, t.layer.replace(".", "_")))
        tx.SetTextSize(pcbnew.VECTOR2I(mm(t.size), mm(t.size)))
        tx.SetTextThickness(mm(max(0.15, t.size * 0.15)))
        tx.SetPosition(pt(t.x, t.y))
        tx.SetTextAngleDegrees(t.rot)
        if t.layer.startswith("B."):
            tx.SetMirrored(True)
        board.Add(tx)

    ds = board.GetDesignSettings()
    ds.SetAuxOrigin(pt(0, spec.height))   # drill/pos files measured from board bottom-left
    ds.SetGridOrigin(pt(0, 0))
    pcbnew.SaveBoard(str(pcb_path), board)
    return pcb_path
