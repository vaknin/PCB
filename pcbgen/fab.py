"""Fab outputs for JLCPCB: gerbers + drill (zipped), BOM and CPL (pick-and-place).

Everything is made by kicad-cli from the checked board; the BOM comes from circuit.py
(the LCSC numbers live there). Gerbers, drill and positions all use the board's
drill/place origin (bottom-left corner, set by the pcb stage), so they line up.

Rotations: JLCPCB's reel orientation differs from KiCad's footprint zero for some
packages. ROTATIONS holds those corrections, from the community table that the
kicad-jlcpcb-tools plugin downloads (matthewlai/JLCKicadTools cpl_rotations_db.csv,
fetched 2026-09-29). A footprint with no entry is listed as UNVERIFIED in fab/README:
check it in JLCPCB's placement preview before paying.
"""

from __future__ import annotations

import csv
import re
import shutil
import subprocess
import zipfile
from pathlib import Path

from .circuit import Circuit

GERBER_LAYERS = "F.Cu,B.Cu,F.Paste,B.Paste,F.Silkscreen,B.Silkscreen,F.Mask,B.Mask,Edge.Cuts"

# footprint-name regex -> degrees added to KiCad's rotation (top side)
ROTATIONS: dict[str, int] = {
    r"^SOT-223": 180,
    r"^SOT-23": -90,
    r"^TSOT-23": 180,
    r"^DFN-": 270,
    r"^USB_C_Receptacle_HRO_TYPE-C-31-M-12": 180,
    r"^ESP32-W": 270,
}
# footprints that need no correction: unpolarised two-terminal parts only (LEDs and
# diodes have a polarity, so a 180 deg error would matter: they stay UNVERIFIED)
NO_CORRECTION = re.compile(r"^(R|C|Fuse)_\d{4}_")


def _cli(*args: str) -> None:
    res = subprocess.run(["kicad-cli", *args], capture_output=True, text=True)
    if res.returncode != 0:
        raise RuntimeError(f"kicad-cli {' '.join(args[:3])} failed:\n{res.stdout}\n{res.stderr}")


def _correction(footprint: str) -> int | None:
    name = footprint.split(":")[-1]
    for pat, deg in ROTATIONS.items():
        if re.search(pat, name):
            return deg
    return 0 if NO_CORRECTION.search(name) else None


def export(project_dir: Path, name: str, circuit: Circuit, out: Path) -> Path:
    pcb = project_dir / f"{name}.kicad_pcb"
    if out.exists():
        shutil.rmtree(out)
    gerbers = out / "gerbers"
    gerbers.mkdir(parents=True)

    _cli("pcb", "export", "gerbers", "-o", str(gerbers), "-l", GERBER_LAYERS,
         "--use-drill-file-origin", "--subtract-soldermask", str(pcb))
    _cli("pcb", "export", "drill", "-o", str(gerbers) + "/", "--format", "excellon",
         "--drill-origin", "plot", "--excellon-units", "mm", str(pcb))
    zip_path = out / f"{name}-gerbers.zip"
    with zipfile.ZipFile(zip_path, "w", zipfile.ZIP_DEFLATED) as z:
        for f in sorted(gerbers.iterdir()):
            z.write(f, f.name)

    pos = out / "positions-kicad.csv"
    _cli("pcb", "export", "pos", "-o", str(pos), "--format", "csv", "--units", "mm",
         "--side", "both", "--use-drill-file-origin", "--exclude-dnp", str(pcb))

    # BOM: parts JLCPCB places = in the BOM, not DNP, with an LCSC number
    placed = {r: p for r, p in circuit.parts.items()
              if p.in_bom and not p.dnp and p.fields.get("LCSC") and not r.startswith("#")}
    skipped = sorted(r for r, p in circuit.parts.items()
                     if not r.startswith("#") and r not in placed)
    groups: dict[tuple[str, str, str], list[str]] = {}
    for r, p in placed.items():
        groups.setdefault((p.value, p.footprint.split(":")[-1], p.fields["LCSC"]), []).append(r)
    with open(out / f"{name}-bom.csv", "w", newline="") as fh:
        w = csv.writer(fh)
        w.writerow(["Comment", "Designator", "Footprint", "LCSC"])
        for (value, fp, lcsc), refs in sorted(groups.items(), key=lambda g: g[1][0]):
            w.writerow([value, ",".join(sorted(refs, key=_refkey)), fp, lcsc])

    # CPL with rotation corrections
    unverified, notes = [], []
    with open(pos, newline="") as fh, open(out / f"{name}-cpl.csv", "w", newline="") as oh:
        w = csv.writer(oh)
        w.writerow(["Designator", "Mid X", "Mid Y", "Layer", "Rotation"])
        for row in csv.DictReader(fh):
            ref = row["Ref"]
            if ref not in placed:
                continue
            fp = placed[ref].footprint
            corr = _correction(fp)
            if corr is None:
                unverified.append(f"{ref} ({fp.split(':')[-1]})")
                corr = 0
            side = row["Side"].lower()
            if side != "top" and corr:
                raise RuntimeError(f"{ref}: bottom-side rotation corrections are not handled")
            rot = (float(row["Rot"]) + corr) % 360
            if corr:
                notes.append(f"{ref}: KiCad {float(row['Rot']):g} deg + {corr} = {rot:g} deg")
            w.writerow([ref, f"{float(row['PosX']):.4f}mm", f"{float(row['PosY']):.4f}mm",
                        "Top" if side == "top" else "Bottom", f"{rot:g}"])
    missing = sorted(set(placed) - {r for r in _refs(out / f"{name}-cpl.csv")})
    if missing:
        raise RuntimeError(f"parts in the BOM but not in the placement file: {missing}")

    (out / "README.md").write_text(_readme(name, len(placed), skipped, notes, unverified))
    print(f"fab: {zip_path.name}, {name}-bom.csv ({len(groups)} lines, {len(placed)} parts), "
          f"{name}-cpl.csv; not assembled: {', '.join(skipped) or 'none'}")
    if unverified:
        print(f"fab: rotation UNVERIFIED for {', '.join(unverified)}: check JLCPCB's placement preview")
    return out


def _refkey(ref: str):
    m = re.match(r"([A-Za-z]+)(\d+)", ref)
    return (m.group(1), int(m.group(2))) if m else (ref, 0)


def _refs(cpl: Path) -> list[str]:
    with open(cpl, newline="") as fh:
        return [row["Designator"] for row in csv.DictReader(fh)]


def _readme(name, n_parts, skipped, notes, unverified) -> str:
    lines = [f"# Fab files: {name}", "",
             "Generated by `python -m pcbgen boards/<name> fab`; do not edit by hand.", "",
             f"- `{name}-gerbers.zip`: upload as the PCB (gerbers + drill).",
             f"- `{name}-bom.csv` and `{name}-cpl.csv`: upload for assembly ({n_parts} parts).",
             f"- Not assembled (no LCSC part, or test points/holes): {', '.join(skipped) or 'none'}.", "",
             "## Rotation corrections applied", ""]
    lines += [f"- {n}" for n in notes] or ["- none"]
    lines += ["", "## Rotation UNVERIFIED (no known correction)", "",
              "Check each of these in JLCPCB's placement preview (pin 1 marker) before paying.", ""]
    lines += [f"- {u}" for u in unverified] or ["- none"]
    return "\n".join(lines) + "\n"
