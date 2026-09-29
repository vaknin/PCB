"""Build the printed case from pcbgen's case/board.json and fit-check it against case/board.step.

    enclosure/.venv/bin/python enclosure/case.py <board>/case [--no-render]

pcbgen's `case` stage (D-025 Phase B) writes board.json and board.step and then runs this. The
stage passes only if the fit.json written here says "ok": true.

Coordinates. board.json is in KiCad board mm (x right, y down); board.step comes from
`kicad-cli pcb export step --user-origin 0x0mm`, so STEP x = x, STEP y = -y. Everything here is
built in STEP coordinates (y up, z up, the PCB's bottom face at z = 0 and its top face at
z = thickness). `P(x, y)` converts a board.json point.

The case (rules: research/2026-09-29-enclosure-tooling.md §3-4, limits from board.json's
case.limits, which pcbgen fills from the material):
- tray: floor + walls up to the split plane = the PCB's top face; a standoff under each mounting
  hole touching the PCB's underside, with a screw clearance hole and a counterbore from below.
- lid: walls from the split plane up; its plate's underside is top_gap above the tallest part;
  a boss comes down onto the PCB's top face at each mounting hole with a blind pilot hole, so a
  self-tapping screw from below clamps the PCB between standoff and boss.
- openings, from board.json footprint data: usb_c / connector (wall cutout around the mouth,
  straddling the split so the lid drops on; usb_c adds an outer recess for the plug's overmold),
  button (lid hole + a separate printed cap with a flange and a stem), pinhole (hole + guide
  tube), led (window pocket or hole), vent (slots).

The fit gate (fit.json; the STEP solids are the independent check of the footprint data):
- interference: each case part (tray, lid, each cap) ∩ board (every part solid and the PCB)
  <= 0.001 mm³;
- clearance: each case part to every part solid >= min_clearance (exact B-rep distance); the only
  intended contacts are the standoffs and bosses on the PCB itself (not a part), and a pressed
  cap on its own switch's actuator;
- edge_gap: the PCB's edge to the tray's walls;
- each opening against the part's solids: a connector's projection onto its wall sits inside
  the cutout with >= 0.2 mm to spare; a button, pinhole, LED or vent is centred within 0.2 mm of
  its part; a cap pressed by its switch's travel touches only the actuator;
- printability from the parameters: walls, floor, boss walls, holes, thinnest skin and part size
  against the material's limits; the screw: a standard length that engages 2 × d in the boss.

fit.json (schema 1):
    {"schema": 1, "date": "YYYY-MM-DD", "board": str, "material": str, "ok": bool,
     "checks": [{"name": str, "value": float, "limit": float, "cmp": ">=" | "<=", "ok": bool,
                 "detail": str}],
     "clearance": [{"part": str, "min_mm": float, "nearest": ref}],   # at rest, every case part
     "openings": [{"ref": str, "kind": str, "ok": bool, "detail": str}],
     "parts": [{"name": str, "step": file, "stl": file, "volume_mm3": float, "size_mm": [x, y, z]}],
     "screw": {"size": str, "length_mm": float | null, "count": int, "engagement_mm": float | null},
     "renders": [png files], "files": [every file written], "problems": [str]}
"""

import datetime
import json
import math
import os
import sys
from pathlib import Path

import cadquery as cq
from OCP.BRepClass3d import BRepClass3d_SolidClassifier
from OCP.gp import gp_Pnt
from OCP.IFSelect import IFSelect_RetDone
from OCP.STEPCAFControl import STEPCAFControl_Reader
from OCP.TCollection import TCollection_ExtendedString
from OCP.TDataStd import TDataStd_Name
from OCP.TDF import TDF_Label, TDF_LabelSequence
from OCP.TDocStd import TDocStd_Document
from OCP.TopAbs import TopAbs_IN
from OCP.TopLoc import TopLoc_Location
from OCP.XCAFDoc import XCAFDoc_DocumentTool

VOLUME_TOL = 0.001  # mm³: more overlap than this is an interference
ALIGN_TOL = 0.2  # mm: an opening's centre to its part's centre
MOUTH_MARGIN = 0.2  # mm: a connector's outline inside its cutout
EPS = 1e-4  # float slack on "at least" comparisons
USB_PLUG_OVERMOLD = (12.4, 6.6)  # mm, width × height around the plug; UNVERIFIED (research §4)
USB_MOUTH_SETBACK_MAX = 2.0  # mm, receptacle mouth behind the outer face; INFERRED (a known failure at 3.4)
CAP_STEM = (1.5, 1.0)  # button cap stem: diameter, length (mm)
CAP_ABOVE_LID = 1.0  # mm the cap stands above the lid at rest
CAP_FLANGE_EXTRA = 1.0  # mm per side wider than its hole, so it can't fall out
PIN_TUBE_GAP = 0.5  # mm from the pinhole's guide tube to the switch top
SCREW_LENGTHS = [3, 4, 5, 6, 8, 10, 12, 14, 16, 20]  # standard self-tapping screw lengths, mm
SCREW_ENGAGE_D = 2.0  # thread engagement in the boss, × d (INFERRED: common guidance for plastics)


def P(x, y):
    """board.json (KiCad, y down) -> STEP (y up)."""
    return (x, -y)


# ---------------------------------------------------------------------------- board.step


def _label_name(label):
    attr = TDataStd_Name()
    if label.FindAttribute(TDataStd_Name.GetID_s(), attr):
        return attr.Get().ToExtString()
    return ""


class Board:
    """board.step read through XCAF: the PCB substrate and each part's solids by reference.

    KiCad names every part instance with its reference (cq.Assembly.importStep refuses the
    file: its part names repeat), so the component labels are read directly."""

    def __init__(self, substrate, parts):
        self.substrate = substrate  # list of cq.Solid
        self.parts = parts  # ref -> list of cq.Solid
        self._compound = {}
        self._bbox = {}

    @classmethod
    def load(cls, path):
        doc = TDocStd_Document(TCollection_ExtendedString("board"))
        reader = STEPCAFControl_Reader()
        reader.SetNameMode(True)
        if reader.ReadFile(str(path)) != IFSelect_RetDone:
            raise RuntimeError(f"can't read {path}")
        reader.Transfer(doc)
        tool = XCAFDoc_DocumentTool.ShapeTool_s(doc.Main())
        roots = TDF_LabelSequence()
        tool.GetFreeShapes(roots)
        substrate, parts = [], {}
        for i in range(1, roots.Length() + 1):
            comps = TDF_LabelSequence()
            tool.GetComponents_s(roots.Value(i), comps)
            for j in range(1, comps.Length() + 1):
                comp = comps.Value(j)
                referred = TDF_Label()
                tool.GetReferredShape_s(comp, referred)
                loc = TopLoc_Location().Multiplied(tool.GetLocation_s(comp))
                shape = cq.Shape.cast(tool.GetShape_s(referred).Moved(loc))
                solids = [s for s in shape.Solids() if s.Volume() > 1e-6]
                if _label_name(referred).endswith("_PCB"):
                    substrate += solids
                else:
                    parts.setdefault(_label_name(comp), []).extend(solids)
        if not substrate:
            raise RuntimeError("board.step has no PCB body (a component named <board>_PCB)")
        return cls(substrate, parts)

    def compound(self, ref):
        if ref not in self._compound:
            self._compound[ref] = cq.Compound.makeCompound(self.parts[ref])
        return self._compound[ref]

    def bbox(self, ref):
        if ref not in self._bbox:
            self._bbox[ref] = self.compound(ref).BoundingBox()
        return self._bbox[ref]

    def extent_above(self, ref, zmin):
        """(xmin, xmax, ymin, ymax, zmin, zmax) of what of a part stands above z = zmin, from its
        triangulation clipped at that plane. Not a boolean: OCCT's common with a box silently
        returns nothing for the EasyEDA USB-C model (verified 2026-09-29)."""
        pts = []
        for s in self.parts[ref]:
            verts, tris = s.tessellate(0.01, 0.1)
            v = [(p.x, p.y, p.z) for p in verts]
            for tri in tris:
                corners = [v[i] for i in tri]
                pts += [p for p in corners if p[2] >= zmin]
                for a, b in ((0, 1), (1, 2), (2, 0)):
                    pa, pb = corners[a], corners[b]
                    if (pa[2] - zmin) * (pb[2] - zmin) < 0:
                        t = (zmin - pa[2]) / (pb[2] - pa[2])
                        pts.append(tuple(pa[k] + t * (pb[k] - pa[k]) for k in range(3)))
        if not pts:
            raise RuntimeError(f"{ref} has nothing above z = {zmin}")
        xs, ys, zs = zip(*pts)
        return min(xs), max(xs), min(ys), max(ys), min(zs), max(zs)


# ---------------------------------------------------------------------------- geometry


def slab(outline, offset, z0, z1):
    """The board outline grown by `offset`, as a prism from z0 to z1."""
    wp = cq.Workplane("XY", origin=(0, 0, z0)).polyline(outline).close()
    if offset:
        wp = wp.offset2D(offset, "arc")
    return wp.extrude(z1 - z0).val()


def cyl(d, z0, z1, x, y):
    return cq.Solid.makeCylinder(d / 2, z1 - z0, cq.Vector(x, y, z0))


def box(x0, x1, y0, y1, z0, z1):
    return cq.Solid.makeBox(x1 - x0, y1 - y0, z1 - z0, cq.Vector(x0, y0, z0))


def fuse(a, *more):
    more = [m for m in more if m is not None]
    return a.fuse(*more).clean() if more else a


def cut(a, *more):
    more = [m for m in more if m is not None]
    return a.cut(*more).clean() if more else a


class Checks:
    def __init__(self):
        self.items = []

    def add(self, name, value, limit, cmp, detail=""):
        ok = value >= limit - EPS if cmp == ">=" else value <= limit + EPS
        self.items.append({"name": name, "value": round(value, 4), "limit": round(limit, 4), "cmp": cmp, "ok": bool(ok), "detail": detail})
        return ok

    def fail(self, name, detail):
        self.items.append({"name": name, "value": 0.0, "limit": 0.0, "cmp": ">=", "ok": False, "detail": detail})


# ---------------------------------------------------------------------------- the case


def wall_side(fp, bj):
    """The board edge a connector faces: the one nearest its body (F.Fab box)."""
    box_ = fp["fab"] or fp["courtyard"]
    (bx0, by0), (bx1, by1) = bj["bbox"]["min"], bj["bbox"]["max"]
    (fx0, fy0), (fx1, fy1) = box_["min"], box_["max"]
    gaps = {"left": fx0 - bx0, "right": bx1 - fx1, "top": fy0 - by0, "bottom": by1 - fy1}
    return min(gaps, key=gaps.get), box_


def build(bj, board, problems):
    """Builds the case parts; returns a dict with the solids and what the checks need."""
    c = bj["case"]
    lim = c["limits"]
    T = bj["thickness"]
    outline = [P(x, y) for x, y in bj["outline"]]
    (bx0, by0), (bx1, by1) = bj["bbox"]["min"], bj["bbox"]["max"]
    gap, wall, floor = c["edge_gap"], c["wall"], c["floor"]
    fps = {f["ref"]: f for f in bj["footprints"]}

    part_top = max((board.bbox(r).zmax for r in board.parts), default=T)
    zb = -(c["bottom_gap"] + floor)  # outside bottom
    zf = -c["bottom_gap"]  # floor top
    z_lu = part_top + c["top_gap"]  # lid plate underside
    z_lt = z_lu + floor  # lid top
    info = {"T": T, "zb": zb, "zf": zf, "z_lu": z_lu, "z_lt": z_lt, "part_top": part_top, "holes": [], "skins": [], "hole_sizes": [], "boss_walls": [], "tube_walls": []}

    # --- shells -------------------------------------------------------------------------
    tray = cut(slab(outline, gap + wall, zb, T), slab(outline, gap, zf, T + 1))
    info["walls"] = cut(slab(outline, gap + wall, zf, T), slab(outline, gap, zf - 1, T + 1))
    lid = cut(slab(outline, gap + wall, T, z_lt), slab(outline, gap, T - 1, z_lu))
    tray_add, tray_cut, lid_add, lid_cut = [], [], [], []

    # --- screws: standoff + counterbore below, boss + pilot above ------------------------
    s = c["screw"]
    boss_od = max(s["pilot"], s["clearance"]) + 2 * lim["boss_wall_min"]
    cb_d, cb_h = s["head_d"] + 0.6, s["head_h"] + 0.3
    usable = (z_lt - lim["skin_min"]) - T - 0.3  # deepest screw tip above the PCB's top face
    need = SCREW_ENGAGE_D * s["d"]
    screw_len, z_cb = None, zb + cb_h
    for length in SCREW_LENGTHS:
        z = max(zb + cb_h, T + need - length)  # the head's seat
        if z <= -1.0 and z + length <= T + usable:  # >= 1 mm of standoff above the head
            screw_len, z_cb = length, z
            break
    info["screw"] = {"size": s["size"], "length_mm": screw_len, "count": len(bj["mounting_holes"]),
                     "engagement_mm": round(z_cb + screw_len - T, 3) if screw_len else None,
                     "need_mm": need, "usable_mm": round(usable, 3)}
    tip = z_cb + (screw_len or 0)
    for h in bj["mounting_holes"]:
        x, y = P(h["x"], h["y"])
        tray_add.append(cyl(boss_od, zf - 0.5, 0.0, x, y))
        if z_cb > zf:  # the counterbore reaches above the floor: a wider foot around it
            tray_add.append(cyl(cb_d + 2 * lim["boss_wall_min"], zf - 0.5, z_cb, x, y))
        tray_cut += [cyl(s["clearance"], zb - 1, 0.5, x, y), cyl(cb_d, zb - 1, z_cb, x, y)]
        lid_add.append(cyl(boss_od, T, z_lu + 0.5, x, y))
        lid_cut.append(cyl(s["pilot"], T - 1, max(tip + 0.3, z_lu), x, y))
        info["holes"].append({"ref": h["ref"], "boss_r": boss_od / 2, "courtyard_r": h["courtyard_r"], "drill": h["drill"]})
    info["hole_sizes"] += [("screw pilot", s["pilot"]), ("screw clearance", s["clearance"])]
    info["boss_walls"] += [("lid boss", (boss_od - s["pilot"]) / 2), ("standoff", (boss_od - s["clearance"]) / 2)]
    if z_cb > zf:
        info["boss_walls"].append(("standoff foot around the screw head", lim["boss_wall_min"]))
    info["boss_od"] = boss_od

    # --- openings -----------------------------------------------------------------------
    caps, cap_info, openings = {}, {}, []
    for o in c["openings"]:
        ref, kind = o["ref"], o["kind"]
        fp = fps.get(ref)
        if fp is None:
            problems.append(f"opening {ref}: no footprint {ref} on the board")
            continue
        cx, cy = P(fp["x"], fp["y"])
        top = board.bbox(ref).zmax if ref in board.parts else None
        rec = {"ref": ref, "kind": kind, "centre": (cx, cy)}
        if kind in ("usb_c", "connector"):
            side, fab = wall_side(fp, bj)
            m, hgt = o["margin"], o["height"]
            (fx0, fy0), (fx1, fy1) = fab["min"], fab["max"]
            z0, z1 = T - m, T + hgt + m
            if side in ("left", "right"):
                a0, a1 = -fy1 - m, -fy0 + m  # along the wall: STEP y
                edge = bx0 if side == "left" else bx1
                sgn = -1 if side == "left" else 1
                xs = sorted([edge - sgn * 0.5, edge + sgn * (gap + wall + 1)])
                cutter = box(xs[0], xs[1], a0, a1, z0, z1)
            else:
                a0, a1 = fx0 - m, fx1 + m  # along the wall: x
                edge = -by0 if side == "top" else -by1  # STEP y of that edge
                sgn = 1 if side == "top" else -1
                ys = sorted([edge - sgn * 0.5, edge + sgn * (gap + wall + 1)])
                cutter = box(a0, a1, ys[0], ys[1], z0, z1)
            tray_cut.append(cutter)
            lid_cut.append(cutter)
            rec.update(side=side, along=(a0, a1), z=(z0, z1), edge=edge, sgn=sgn)
            if kind == "usb_c":
                depth = wall - lim["skin_min"]
                w, hh = USB_PLUG_OVERMOLD
                ac, zc = (a0 + a1) / 2, T + hgt / 2
                outer = edge + sgn * (gap + wall)
                acr = sorted([outer - sgn * depth, outer + sgn * 1.0])
                if side in ("left", "right"):
                    recess = box(acr[0], acr[1], ac - w / 2, ac + w / 2, zc - hh / 2, zc + hh / 2)
                else:
                    recess = box(ac - w / 2, ac + w / 2, acr[0], acr[1], zc - hh / 2, zc + hh / 2)
                if depth > 0:
                    tray_cut.append(recess)
                    lid_cut.append(recess)
                rec["face"] = outer - sgn * max(depth, 0)  # where the plug's overmold stops
                info["skins"].append((f"{ref} wall left at the plug recess", wall - max(depth, 0)))
        elif kind == "button":
            if top is None:
                problems.append(f"opening {ref}: no 3D solid for {ref} in board.step")
                continue
            d = o["diameter"]
            hole = d + 2 * lim["moving_fit"]
            flange_d, flange_t = hole + 2 * CAP_FLANGE_EXTRA, max(lim["wall_min"], 1.2)
            rest = max(0.2, c["min_clearance"])
            z_bot = top + rest
            stem_d, stem_len = min(CAP_STEM[0], d), CAP_STEM[1]
            z_body = z_bot + stem_len
            z_fl0 = z_lu - flange_t
            if z_fl0 < z_body + 0.2:
                problems.append(f"opening {ref}: no room for the button cap between the switch top (z {top:.2f}) and the lid (z {z_lu:.2f}); raise top_gap")
                continue
            z_top = z_lt + CAP_ABOVE_LID
            cap = fuse(cyl(stem_d, z_bot, z_body + 0.01, cx, cy), cyl(d, z_body, z_top, cx, cy), cyl(flange_d, z_fl0, z_lu, cx, cy))
            caps[ref] = cap
            lid_cut.append(cyl(hole, z_lu - 1, z_lt + 1, cx, cy))
            cap_info[ref] = {"travel": o["travel"], "rest": rest, "z_top": z_top, "switch_top": top}
            info["hole_sizes"].append((f"{ref} cap hole", hole))
        elif kind == "pinhole":
            d = o["diameter"]
            lid_cut.append(cyl(d, (top if top else T) - 1, z_lt + 1, cx, cy))
            if top is not None and top + PIN_TUBE_GAP < z_lu:
                lid_add.append(cyl(d + 2 * lim["wall_min"], top + PIN_TUBE_GAP, z_lu + 0.5, cx, cy))
                info["tube_walls"].append((f"{ref} guide tube", lim["wall_min"]))
            info["hole_sizes"].append((f"{ref} pinhole", d))
        elif kind == "led":
            d = o["diameter"]
            if o["style"] == "hole":
                lid_cut.append(cyl(d, z_lu - 1, z_lt + 1, cx, cy))
                info["hole_sizes"].append((f"{ref} LED hole", d))
            else:
                skin = lim["skin_min"]
                lid_cut.append(cyl(d, z_lu - 1, z_lt - skin, cx, cy))
                info["skins"].append((f"{ref} LED window", skin))
        elif kind == "vent":
            n, w, length = o["slots"], o["slot_width"], o["slot_length"]
            pitch = w + lim["wall_min"]
            for i in range(n):
                sx = cx + (i - (n - 1) / 2) * pitch
                lid_cut.append(box(sx - w / 2, sx + w / 2, cy - length / 2, cy + length / 2, z_lu - 1, z_lt + 1))
            info["hole_sizes"].append((f"{ref} vent slots", w))
        openings.append(rec)

    tray = cut(fuse(tray, *tray_add), *tray_cut)
    lid = cut(fuse(lid, *lid_add), *lid_cut)
    for name, shape in [("tray", tray), ("lid", lid)] + [(f"cap-{r}", s) for r, s in caps.items()]:
        n = len(shape.Solids())
        if n != 1 or not shape.isValid():
            problems.append(f"the {name} came out as {n} solids{'' if shape.isValid() else ' (invalid B-rep)'}; expected one valid solid")
    tray, lid = tray.Solids()[0], lid.Solids()[0]
    caps = {r: s.Solids()[0] for r, s in caps.items()}
    return {"tray": tray, "lid": lid, "caps": caps, "cap_info": cap_info, "openings": openings, "info": info}


# ---------------------------------------------------------------------------- the fit gate


def _inside(solid, points):
    """Is any of the points strictly inside the solid (not on its boundary)?"""
    bb = solid.BoundingBox()
    near = [p for p in points if bb.xmin <= p.X <= bb.xmax and bb.ymin <= p.Y <= bb.ymax and bb.zmin <= p.Z <= bb.zmax]
    if not near:
        return False
    cls = BRepClass3d_SolidClassifier(solid.wrapped)
    for p in near:
        cls.Perform(gp_Pnt(p.X, p.Y, p.Z), 1e-6)
        if cls.State() == TopAbs_IN:
            return True
    return False


def overlap(a, b, dist=None):
    """Volume of a ∩ b (mm³), robust to failed booleans.

    OCCT's boolean common silently returns an empty shape for some part models (the EasyEDA
    USB-C, verified 2026-09-29), so a boolean is used only where the exact B-rep distance says
    the two touch, and a vertex of either strictly inside the other also counts as an overlap.
    Apart (distance > 0) and neither inside the other: 0."""
    if dist is None:
        dist = a.distance(b)
    embedded = _inside(a, b.Vertices()) or _inside(b, a.Vertices())
    if dist > 1e-6 and not embedded:
        return 0.0
    vol = a.intersect(b).Volume()
    if embedded and vol <= VOLUME_TOL:
        # the boolean missed it: count the smaller solid as the overlap
        vol = min(a.Volume(), b.Volume())
    return vol


def box_of(bb):
    return cq.Solid.makeBox(bb.xlen, bb.ylen, bb.zlen, cq.Vector(bb.xmin, bb.ymin, bb.zmin))


def near_parts(shape, groups):
    """Exact clearance from `shape` to groups of solids {name: [solids]}.

    Returns (min distance, nearest name, {name: overlap mm³ > VOLUME_TOL}). Exact B-rep distances
    are slow on detailed models (0.3-4 s each), so each group's bounding box is measured first:
    its distance is a lower bound, and a group whose bound is already beyond the best found
    can't be nearer. Overlap volumes are computed only for groups at distance 0, or with a point
    inside `shape` (a part wholly inside a case wall has boundaries apart)."""
    bounds = []
    for name, solids in groups.items():
        comp = cq.Compound.makeCompound(solids)
        bb = comp.BoundingBox()
        bounds.append((shape.distance(box_of(bb)), name, comp))
    bounds.sort(key=lambda x: x[0])
    best, nearest, hits = math.inf, None, {}
    for lb, name, comp in bounds:
        embedded = _inside(shape, [comp.Vertices()[0]])
        if lb > 1e-6 and lb >= best and not embedded:
            continue
        d = shape.distance(comp)
        if d < best:
            best, nearest = d, name
        if d <= 1e-6 or embedded:
            vol = sum(overlap(shape, sol) for sol in groups[name])
            if vol > VOLUME_TOL:
                hits[name] = vol
    return best, nearest, hits


def check(bj, board, built, problems):
    c = bj["case"]
    lim = c["limits"]
    info = built["info"]
    T = info["T"]
    ck = Checks()

    # board.step must be the board board.json describes
    sub = cq.Compound.makeCompound(board.substrate).BoundingBox()
    (bx0, by0), (bx1, by1) = bj["bbox"]["min"], bj["bbox"]["max"]
    off = max(abs(sub.xmin - bx0), abs(sub.xmax - bx1), abs(sub.ymin + by1), abs(sub.ymax + by0), abs(sub.zmin))
    ck.add("step_matches_board", off, 0.05, "<=", f"board.step's PCB body vs board.json's outline (x = x, y = -y, bottom at z = 0); body top z {sub.zmax:.2f}, thickness {T}")
    if sub.zmax > T + 0.01:
        problems.append(f"board.step's PCB body is {sub.zmax:.2f} mm thick, more than the board's {T}")

    parts = {"tray": built["tray"], "lid": built["lid"]}
    parts.update({f"cap-{r}": s for r, s in built["caps"].items()})

    # interference and clearance, at rest (the PCB itself is touched on purpose by the standoffs
    # and bosses: it counts for interference only)
    clearance = []
    for name, shape in parts.items():
        best, nearest, hits = near_parts(shape, board.parts)
        pcb = sum(overlap(shape, sol) for sol in board.substrate)
        if pcb > VOLUME_TOL:
            hits["PCB"] = pcb
        vol = sum(hits.values())
        ck.add(f"interference.{name}", vol, VOLUME_TOL, "<=", "overlaps " + ", ".join(f"{r} ({v:.2f} mm³)" for r, v in sorted(hits.items())) if hits else "touches nothing it shouldn't")
        clearance.append({"part": name, "min_mm": round(best, 4), "nearest": nearest})
        ck.add(f"clearance.{name}", best, c["min_clearance"], ">=", f"nearest part {nearest}")

    # PCB edge to the tray's walls
    edge = min(info["walls"].distance(s) for s in board.substrate)
    ck.add("edge_gap", edge, c["edge_gap"] - 0.01, ">=", "the PCB's edge to the tray's inner wall")

    # openings against the part solids
    op_results = []
    for rec in built["openings"]:
        ref, kind = rec["ref"], rec["kind"]
        if ref not in board.parts:
            detail = f"no 3D solid for {ref} in board.step: the opening can't be checked"
            ck.fail(f"opening.{ref}", detail)
            op_results.append({"ref": ref, "kind": kind, "ok": False, "detail": detail})
            continue
        if kind in ("usb_c", "connector"):
            x0, x1, y0, y1, pz0, pz1 = board.extent_above(ref, T - 0.05)  # what stands above the PCB
            a0, a1 = rec["along"]
            z0, z1 = rec["z"]
            if rec["side"] in ("left", "right"):
                p0, p1 = y0, y1
                mouth = x0 if rec["side"] == "left" else x1
            else:
                p0, p1 = x0, x1
                mouth = y1 if rec["side"] == "top" else y0
            margin = min(p0 - a0, a1 - p1, pz0 - z0, z1 - pz1)
            detail = f"{rec['side']} wall; the part's outline {p1 - p0:.2f} × {pz1 - pz0:.2f} mm inside a {a1 - a0:.2f} × {z1 - z0:.2f} mm cutout, {margin:.2f} mm to spare"
            ok = ck.add(f"opening.{ref}", margin, MOUTH_MARGIN, ">=", detail)
            if kind == "usb_c":
                setback = abs(rec["face"] - mouth)
                ok &= ck.add(f"usb_setback.{ref}", setback, USB_MOUTH_SETBACK_MAX, "<=", "receptacle mouth behind the case's outer face at the plug recess (limit INFERRED)")
            op_results.append({"ref": ref, "kind": kind, "ok": bool(ok), "detail": detail})
        else:
            bb = board.bbox(ref)
            px, py = (bb.xmin + bb.xmax) / 2, (bb.ymin + bb.ymax) / 2
            cx, cy = rec["centre"]
            off = math.hypot(px - cx, py - cy)
            detail = f"opening centre {off:.3f} mm from {ref}'s 3D body centre"
            ok = ck.add(f"opening.{ref}", off, ALIGN_TOL, "<=", detail)
            op_results.append({"ref": ref, "kind": kind, "ok": bool(ok), "detail": detail})

    # button caps, pressed by the switch's travel
    for ref, cap in built["caps"].items():
        ci = built["cap_info"][ref]
        down = ci["rest"] + ci["travel"]
        pressed = cap.translate(cq.Vector(0, 0, -down))
        solids = board.parts[ref]
        actuator = [s for s in solids if s.BoundingBox().zmax >= ci["switch_top"] - 0.05]
        groups = {r: ss for r, ss in board.parts.items() if r != ref}
        rest_of_switch = [x for x in solids if not any(x is a for a in actuator)]
        if rest_of_switch:
            groups[f"{ref} (body)"] = rest_of_switch
        reach = sum(overlap(pressed, a) for a in actuator)
        near, _, hits = near_parts(pressed, groups)
        wrong = sum(hits.values()) + sum(overlap(pressed, sol) for sol in board.substrate)
        lid_hit = overlap(pressed, built["lid"]) + overlap(cap, built["lid"])
        above = ci["z_top"] - down - info["z_lt"]
        ok = reach > VOLUME_TOL and wrong <= VOLUME_TOL and lid_hit <= VOLUME_TOL and above > 0
        detail = (f"pressed {down:.2f} mm (gap {ci['rest']:.2f} + travel {ci['travel']:.2f}): presses the actuator "
                  f"({'yes' if reach > VOLUME_TOL else 'NO'}), overlaps nothing else ({wrong:.3f} mm³), "
                  f"stands {above:.2f} mm above the lid")
        if not ok:
            ck.fail(f"cap_travel.{ref}", detail)
        else:
            ck.add(f"cap_travel.{ref}", near, c["min_clearance"], ">=", detail + "; value: pressed cap to anything else")
        for r in op_results:
            if r["ref"] == ref:
                r["ok"] = r["ok"] and ck.items[-1]["ok"]
                r["detail"] += "; " + detail

    # printability from the parameters
    ck.add("print.wall", c["wall"], lim["wall_min"], ">=", f"side wall ({c['material']})")
    ck.add("print.floor", c["floor"], lim["wall_min"], ">=", "tray floor and lid plate")
    if info["boss_walls"]:
        name, v = min(info["boss_walls"], key=lambda x: x[1])
        ck.add("print.boss_wall", v, lim["boss_wall_min"], ">=", "thinnest: " + name)
    if info["tube_walls"]:
        name, v = min(info["tube_walls"], key=lambda x: x[1])
        ck.add("print.tube_wall", v, lim["wall_min"], ">=", name)
    if info["hole_sizes"]:
        name, v = min(info["hole_sizes"], key=lambda x: x[1])
        ck.add("print.holes", v, lim["hole_min"], ">=", "smallest: " + name)
    if info["skins"]:
        name, v = min(info["skins"], key=lambda x: x[1])
        ck.add("print.skin", v, lim["skin_min"], ">=", "thinnest: " + name)
    sizes = {n: s.BoundingBox() for n, s in parts.items()}
    small = min(sizes.items(), key=lambda kv: min(kv[1].xlen, kv[1].ylen, kv[1].zlen))
    ck.add("print.part_size", min(small[1].xlen, small[1].ylen, small[1].zlen), lim["part_min"], ">=", f"smallest part: {small[0]}")

    # screws
    if not bj["mounting_holes"]:
        ck.fail("screws", "the board has no mounting holes: nothing holds the case shut")
    for h in info["holes"]:
        room = h["courtyard_r"]
        ck.add(f"boss_in_courtyard.{h['ref']}", (room if room is not None else 0) - h["boss_r"], 0.0, ">=",
               f"boss Ø{2 * h['boss_r']:.2f} inside {h['ref']}'s courtyard (radius {room})")
    sc = info["screw"]
    if bj["mounting_holes"]:
        if sc["length_mm"] is None:
            ck.fail("screw_length", f"no standard {sc['size']} length engages {sc['need_mm']:.1f} mm in the lid, which has {sc['usable_mm']:.2f} mm; use a smaller screw or raise the lid")
        else:
            ck.add("screw_length", sc["engagement_mm"], sc["need_mm"], ">=", f"{sc['count']} × {sc['size']} × {sc['length_mm']} mm self-tapping screws for plastic; thread in the lid boss (mm)")
        drill = min(h["drill"] for h in info["holes"])
        s = c["screw"]
        ck.add("screw_through_pcb", drill - s["d"], 0.2, ">=", f"{s['size']} through the Ø{drill} mounting holes")
    return ck, clearance, op_results


# ---------------------------------------------------------------------------- outputs


COL_CASE = cq.Color(0.93, 0.93, 0.90, 1.0)
COL_LID_SEE = cq.Color(0.93, 0.93, 0.90, 0.35)
COL_CAP = cq.Color(0.85, 0.45, 0.15, 1.0)
COL_PCB = cq.Color(0.10, 0.45, 0.25, 1.0)
COL_PART = cq.Color(0.25, 0.25, 0.28, 1.0)


def board_assy(board, dz=0.0):
    a = cq.Assembly()
    loc = cq.Location(cq.Vector(0, 0, dz))
    a.add(cq.Compound.makeCompound(board.substrate), name="pcb", color=COL_PCB, loc=loc)
    a.add(cq.Compound.makeCompound([s for ss in board.parts.values() for s in ss]), name="parts", color=COL_PART, loc=loc)
    return a


def render(path, assy, **view):
    from cadquery.vis import show

    show(assy, screenshot=str(path), interact=False, width=1200, height=900, trihedron=False, tolerance=0.02, **view)


def write_outputs(out, built, board, render_pngs):
    files, parts, renders = [], [], []
    named = [("case-bottom", built["tray"]), ("case-lid", built["lid"])] + [(f"cap-{r}", s) for r, s in built["caps"].items()]
    for name, shape in named:
        cq.exporters.export(cq.Workplane().add(shape), str(out / f"{name}.step"))
        cq.exporters.export(cq.Workplane().add(shape), str(out / f"{name}.stl"), tolerance=0.01, angularTolerance=0.1)
        bb = shape.BoundingBox()
        parts.append({"name": name, "step": f"{name}.step", "stl": f"{name}.stl", "volume_mm3": round(shape.Volume(), 1),
                      "size_mm": [round(bb.xlen, 2), round(bb.ylen, 2), round(bb.zlen, 2)]})
        files += [f"{name}.step", f"{name}.stl"]
    if render_pngs:
        os.environ.pop("DISPLAY", None)  # render off-screen even from a desktop session
        os.environ.pop("WAYLAND_DISPLAY", None)
        caps = list(built["caps"].values())
        a = board_assy(board)
        a.add(built["tray"], name="tray", color=COL_CASE)
        a.add(built["lid"], name="lid", color=COL_LID_SEE)
        for i, cap in enumerate(caps):
            a.add(cap, name=f"cap{i}", color=COL_CAP)
        render(out / "case-iso.png", a, zoom=1.1)
        h = built["info"]["z_lt"] - built["info"]["zb"]
        e = cq.Assembly()
        e.add(built["tray"], name="tray", color=COL_CASE)
        e.add(board_assy(board, dz=h * 0.9), name="board")
        lift = cq.Location(cq.Vector(0, 0, h * 2.2))
        e.add(built["lid"], name="lid", color=COL_CASE, loc=lift)
        for i, cap in enumerate(caps):
            e.add(cap, name=f"cap{i}", color=COL_CAP, loc=cq.Location(cq.Vector(0, 0, h * 3.2)))
        render(out / "case-exploded.png", e, zoom=1.0)
        t = board_assy(board)
        t.add(built["lid"], name="lid", color=COL_CASE)
        for i, cap in enumerate(caps):
            t.add(cap, name=f"cap{i}", color=COL_CAP)
        render(out / "case-top.png", t, roll=0, elevation=0, azimuth=0, orthographic=True, zoom=1.1)
        renders = ["case-iso.png", "case-exploded.png", "case-top.png"]
        files += renders
    return files, parts, renders


def run(bj, board, out=None, render_pngs=True):
    """Builds and checks the case; writes the outputs into `out` if given. Returns fit.json."""
    problems = []
    built = build(bj, board, problems)
    ck, clearance, op_results = check(bj, board, built, problems)
    files, parts, renders = ([], [], [])
    if out is not None:
        files, parts, renders = write_outputs(out, built, board, render_pngs)
    ok = all(i["ok"] for i in ck.items) and not problems
    s = built["info"]["screw"]
    fit = {
        "schema": 1,
        "date": datetime.date.today().isoformat(),
        "board": bj["board"],
        "material": bj["case"]["material"],
        "ok": ok,
        "checks": ck.items,
        "clearance": clearance,
        "openings": op_results,
        "parts": parts,
        "screw": {k: s[k] for k in ("size", "length_mm", "count", "engagement_mm")},
        "renders": renders,
        "files": files + ["fit.json"],
        "problems": problems,
    }
    if out is not None:
        (out / "fit.json").write_text(json.dumps(fit, indent=2, ensure_ascii=False) + "\n")
    return fit


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    if len(args) != 1:
        sys.exit(__doc__.split("\n\n")[1])
    out = Path(args[0])
    bj = json.loads((out / "board.json").read_text())
    board = Board.load(out / "board.step")
    fit = run(bj, board, out, render_pngs="--no-render" not in sys.argv)
    print(f"case.py: {len(fit['checks'])} checks, {sum(not c['ok'] for c in fit['checks'])} failing, {len(fit['problems'])} problems; {out / 'fit.json'}")
    sys.exit(0 if fit["ok"] else 1)


if __name__ == "__main__":
    main()
