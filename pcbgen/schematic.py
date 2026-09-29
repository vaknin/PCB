"""Emit a KiCad schematic from a Circuit.

Style: every connected pin gets a short wire stub ending in a net label; unused
pins get a no-connect flag. Parts are grouped by `block` into titled columns.
The file is written in KiCad 9 format and then rewritten by
`kicad-cli sch upgrade`, so the final file is always saved by KiCad itself.
"""

from __future__ import annotations

import copy
import math
import subprocess
import uuid
from datetime import date
from pathlib import Path

from . import symlib
from .circuit import Circuit, Part
from .sexpr import L, Sym, dumps, find, find_all, yes

FORMAT_VERSION = 20250114  # KiCad 9; upgraded to the running KiCad's format afterwards
STUB = 2.54
GRID = 2.54
CHAR_W = 1.0     # approx width of one 1.27 mm label character
FONT = L("effects", L("font", L("size", 1.27, 1.27)))


def _uid(circuit: Circuit, *key: str) -> str:
    return str(uuid.uuid5(uuid.NAMESPACE_URL, "pcbgen:" + circuit.name + ":" + "/".join(key)))


def _r(v: float) -> float:
    return round(v, 4)


def _snap(v: float) -> float:
    return round(v / GRID) * GRID


def _xform(px: float, py: float, rot: int) -> tuple[float, float]:
    """Library point (Y up) -> schematic offset (Y down) for a symbol rotated `rot` deg CCW."""
    x, y = px, -py
    t = math.radians(rot)
    c, s = round(math.cos(t)), round(math.sin(t))
    return x * c + y * s, -x * s + y * c


def _dir(angle: int, rot: int) -> tuple[int, int]:
    """Unit vector (schematic space) for a library pin angle after symbol rotation."""
    a = math.radians(angle)
    dx, dy = _xform(round(math.cos(a)), round(math.sin(a)), rot)
    return int(dx), int(dy)


_LABEL_ANGLE = {(1, 0): 0, (0, -1): 90, (-1, 0): 180, (0, 1): 270}


def _extent(circuit: Circuit, part: Part) -> tuple[float, float, float, float]:
    """Bounding box (schematic offsets) including stubs and label text."""
    sym = part.symbol
    x0, y0, x1, y1 = sym.bbox
    pts = [_xform(x, y, part.rot) for x in (x0, x1) for y in (y0, y1)]
    xs = [p[0] for p in pts]
    ys = [p[1] for p in pts]
    for pin in sym.pins:
        net = circuit.net_of(part, pin)
        px, py = _xform(pin.x, pin.y, part.rot)
        ox, oy = (-d for d in _dir(pin.angle, part.rot))
        reach = STUB + (len(net.name) * CHAR_W + 1.5 if net else 1.0)
        xs.append(px + ox * reach)
        ys.append(py + oy * reach)
        if oy == 0:
            ys += [py - 1.5, py + 1.5]
    return min(xs), min(ys), max(xs), max(ys) + 5.0  # room for ref/value text


def _layout(circuit: Circuit) -> tuple[dict[str, tuple[float, float]], list, tuple[float, float]]:
    """Place blocks as columns left to right, wrapping into rows. Returns part positions,
    block-title texts, and the used width/height."""
    blocks: dict[str, list[Part]] = {}
    for p in circuit.parts.values():
        blocks.setdefault(p.block, []).append(p)
    pos: dict[str, tuple[float, float]] = {}
    titles = []
    margin, gap, max_w = 25.4, 12.7, 380.0
    x = y = margin
    row_h = 0.0
    for name, parts in blocks.items():
        ext = {p.ref: _extent(circuit, p) for p in parts}
        width = max(max(e[2] - e[0] for e in ext.values()), len(name) * 2.3)
        height = sum(e[3] - e[1] + 5.08 for e in ext.values()) + 7.62
        if x + width > max_w and x > margin:
            x, y, row_h = margin, y + row_h + gap, 0.0
        titles.append((name, x, y))
        cy = y + 7.62
        for p in parts:
            e = ext[p.ref]
            pos[p.ref] = (_snap(x - e[0] + (width - (e[2] - e[0])) / 2), _snap(cy - e[1]))
            cy += e[3] - e[1] + 5.08
        x += width + gap
        row_h = max(row_h, height)
    return pos, titles, (max_w + margin, y + row_h + margin)


def _paper(w: float, h: float) -> list:
    for name, (pw, ph) in (("A4", (297, 210)), ("A3", (420, 297)), ("A2", (594, 420))):
        if w <= pw and h <= ph:
            return L("paper", name)
    return L("paper", "User", _r(w), _r(h))


def _prop(key: str, value: str, x: float, y: float, *, hide=False, justify: str | None = None) -> list:
    eff = copy.deepcopy(FONT)
    if justify:
        eff.append(L("justify", Sym(justify)))
    if hide:
        eff.append(L("hide", Sym("yes")))
    return L("property", key, value, L("at", _r(x), _r(y), 0), eff)


def _symbol_instance(circuit: Circuit, part: Part, x: float, y: float, root: str) -> list:
    sym = part.symbol
    x0, y0, x1, y1 = sym.bbox
    pts = [_xform(a, b, part.rot) for a in (x0, x1) for b in (y0, y1)]
    top = min(p[1] for p in pts)
    bot = max(p[1] for p in pts)
    right = max(p[0] for p in pts)
    has_vertical_pins = any(_dir(p.angle, part.rot)[0] == 0 for p in sym.pins)
    if has_vertical_pins and not sym.is_power:
        # keep text clear of labels hanging off top/bottom pins
        fx, j = x + right + 1.27, "left"
        ref_y, val_y = y + top - 1.27, y + top + 1.27
    else:
        fx, j = x, None
        ref_y, val_y = y + top - 1.27, y + bot + 1.905
    node = L("symbol", L("lib_id", part.lib_id), L("at", _r(x), _r(y), part.rot),
             L("unit", 1), L("exclude_from_sim", Sym("no")), L("in_bom", yes(part.in_bom)),
             L("on_board", yes(part.on_board)), L("dnp", yes(part.dnp)),
             L("uuid", _uid(circuit, "sym", part.ref)))
    node.append(_prop("Reference", part.ref, fx, ref_y, justify=j, hide=sym.is_power))
    node.append(_prop("Value", part.value, fx, val_y, justify=j))
    node.append(_prop("Footprint", part.footprint, x, y, hide=True))
    node.append(_prop("Datasheet", sym.prop("Datasheet") or "~", x, y, hide=True))
    for k, v in part.fields.items():
        node.append(_prop(k, v, x, y, hide=True))
    for pin in sym.pins:
        node.append(L("pin", pin.number, L("uuid", _uid(circuit, "pin", part.ref, pin.number))))
    node.append(L("instances", L("project", circuit.name,
                                 L("path", "/" + root, L("reference", part.ref), L("unit", 1)))))
    return node


def _power_symbol(net_name: str) -> symlib.Symbol | None:
    """A net named like a KiCad power symbol (GND, +3V3, VBUS, ...) is drawn with that symbol."""
    try:
        sym = symlib.load(f"power:{net_name}")
    except KeyError:
        return None
    return sym if sym.is_power and net_name != "PWR_FLAG" else None


def _power_rot(sym: symlib.Symbol, outward: tuple[int, int]) -> int:
    """Rotation that makes the symbol's body point along `outward` (away from the pin)."""
    x0, y0, x1, y1 = sym.bbox
    natural = (0, 1) if (y0 + y1) < 0 else (0, -1)  # screen space: body below or above the pin
    for rot in (0, 90, 180, 270):
        if tuple(int(v) for v in _xform(natural[0], -natural[1], rot)) == outward:
            return rot
    raise AssertionError


def _power_instance(circuit: Circuit, sym: symlib.Symbol, ref: str, x: float, y: float,
                    rot: int, root: str, key: tuple[str, str]) -> list:
    tx, ty = _xform(0, 3.2 if (sym.bbox[1] + sym.bbox[3]) > 0 else -4.0, rot)
    node = L("symbol", L("lib_id", sym.lib_id), L("at", x, y, rot), L("unit", 1),
             L("exclude_from_sim", Sym("no")), L("in_bom", Sym("no")), L("on_board", Sym("yes")),
             L("dnp", Sym("no")), L("uuid", _uid(circuit, "pwr", *key)))
    node.append(_prop("Reference", ref, x, y, hide=True))
    node.append(_prop("Value", sym.prop("Value"), _r(x + tx), _r(y + ty)))
    node.append(_prop("Footprint", "", x, y, hide=True))
    node.append(_prop("Datasheet", "", x, y, hide=True))
    for pin in sym.pins:
        node.append(L("pin", pin.number, L("uuid", _uid(circuit, "pwrpin", *key))))
    node.append(L("instances", L("project", circuit.name,
                                 L("path", "/" + root, L("reference", ref), L("unit", 1)))))
    return node


def build(circuit: Circuit) -> list:
    problems = circuit.check()
    if problems:
        raise ValueError("circuit check failed:\n  " + "\n  ".join(problems))
    root = _uid(circuit, "root")

    # PWR_FLAG symbols become ordinary parts in their own block
    for i, net_name in enumerate(circuit.pwr_flags, 1):
        ref = f"#FLG{i:02d}"
        if ref not in circuit.parts:
            flag = circuit.part(ref, "power:PWR_FLAG", "PWR_FLAG", "", block="Power flags", in_bom=False)
            flag.on_board = False
            net = circuit.net(net_name)
            net += flag["1"]

    pos, titles, (w, h) = _layout(circuit)
    items: list = []
    lib_ids: list[str] = []
    n_pwr = 0
    for part in circuit.parts.values():
        if part.lib_id not in lib_ids:
            lib_ids.append(part.lib_id)
        x, y = pos[part.ref]
        items.append(_symbol_instance(circuit, part, x, y, root))
        seen: set[tuple[float, float]] = set()
        for pin in part.symbol.pins:
            dx, dy = _xform(pin.x, pin.y, part.rot)
            px, py = _r(x + dx), _r(y + dy)
            if (px, py) in seen:
                continue  # stacked pins share one connection
            seen.add((px, py))
            net = circuit.net_of(part, pin)
            key = (part.ref, pin.number)
            if net is None:
                if circuit.is_nc(part, pin):
                    items.append(L("no_connect", L("at", px, py), L("uuid", _uid(circuit, "nc", *key))))
                continue
            ox, oy = (-d for d in _dir(pin.angle, part.rot))
            ex, ey = _r(px + ox * STUB), _r(py + oy * STUB)
            items.append(L("wire", L("pts", L("xy", px, py), L("xy", ex, ey)),
                           L("stroke", L("width", 0), L("type", Sym("default"))),
                           L("uuid", _uid(circuit, "wire", *key))))
            psym = _power_symbol(net.name)
            if psym:
                if psym.lib_id not in lib_ids:
                    lib_ids.append(psym.lib_id)
                n_pwr += 1
                items.append(_power_instance(circuit, psym, f"#PWR{n_pwr:03d}", ex, ey,
                                             _power_rot(psym, (ox, oy)), root, key))
                continue
            angle = _LABEL_ANGLE[(ox, oy)]
            eff = copy.deepcopy(FONT)
            eff.append(L("justify", Sym("right" if angle in (180, 270) else "left"), Sym("bottom")))
            items.append(L("label", net.name, L("at", ex, ey, angle), L("fields_autoplaced", Sym("yes")),
                           eff, L("uuid", _uid(circuit, "label", *key))))
    for name, tx, ty in titles:
        eff = L("effects", L("font", L("size", 2.54, 2.54), L("bold", Sym("yes"))),
                L("justify", Sym("left"), Sym("bottom")))
        items.append(L("text", name, L("exclude_from_sim", Sym("no")), L("at", _r(tx), _r(ty), 0), eff,
                       L("uuid", _uid(circuit, "title", name))))

    lib_symbols = L("lib_symbols", *[copy.deepcopy(symlib.load(i).node) for i in lib_ids])
    return L("kicad_sch", L("version", FORMAT_VERSION), L("generator", "pcbgen"),
             L("generator_version", "0.1"), L("uuid", root), _paper(w, h),
             L("title_block", L("title", circuit.title), L("date", date.today().isoformat()),
               L("rev", circuit.rev), L("company", circuit.company),
               L("comment", 1, "Generated from code by pcbgen; do not edit by hand")),
             lib_symbols, *items,
             L("sheet_instances", L("path", "/", L("page", "1"))))


def write(circuit: Circuit, out_dir: Path) -> Path:
    out_dir.mkdir(parents=True, exist_ok=True)
    path = out_dir / f"{circuit.name}.kicad_sch"
    path.write_text(dumps(build(circuit)) + "\n")
    res = subprocess.run(["kicad-cli", "sch", "upgrade", str(path)], capture_output=True, text=True)
    if res.returncode != 0:
        raise RuntimeError(f"kicad-cli sch upgrade failed:\n{res.stdout}\n{res.stderr}")
    _write_lib_tables(circuit, out_dir)
    return path


def _write_lib_tables(circuit: Circuit, out_dir: Path) -> None:
    """Project-local library tables, so ERC/DRC don't depend on the user's global KiCad config."""
    sym_libs = sorted({p.lib_id.split(":")[0] for p in circuit.parts.values()})
    fp_libs = sorted({p.footprint.split(":")[0] for p in circuit.parts.values() if p.footprint})
    local = Path(__file__).resolve().parent.parent / "lib"

    def sym_uri(lib: str) -> str:
        own = local / "symbols" / f"{lib}.kicad_sym"
        return str(own) if own.exists() else f"${{KICAD10_SYMBOL_DIR}}/{lib}.kicad_sym"

    def fp_uri(lib: str) -> str:
        own = local / "footprints" / f"{lib}.pretty"
        return str(own) if own.exists() else f"${{KICAD10_FOOTPRINT_DIR}}/{lib}.pretty"

    sym = L("sym_lib_table", L("version", 7), *[
        L("lib", L("name", n), L("type", "KiCad"), L("uri", sym_uri(n)), L("options", ""), L("descr", ""))
        for n in sym_libs])
    fp = L("fp_lib_table", L("version", 7), *[
        L("lib", L("name", n), L("type", "KiCad"), L("uri", fp_uri(n)), L("options", ""), L("descr", ""))
        for n in fp_libs])
    (out_dir / "sym-lib-table").write_text(dumps(sym) + "\n")
    (out_dir / "fp-lib-table").write_text(dumps(fp) + "\n")
