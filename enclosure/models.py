"""Fetch the 3D models the boards need into lib/3dmodels (D-025 Phase B.2).

    enclosure/.venv/bin/python enclosure/models.py          # fetch what is missing
    enclosure/.venv/bin/python enclosure/models.py --check  # only compare with MANIFEST.json

The models are committed; this script records where each came from and redoes the one-time
alignment of the EasyEDA ones, so the directory can be rebuilt and audited. KiCad footprints
reference `${KICAD10_3DMODEL_DIR}/<Lib>.3dshapes/<name>.step`; pcbgen's case stage sets that
variable to lib/3dmodels, so each model is stored under the name its footprint already uses.
"""

import hashlib
import json
import subprocess
import sys
import tempfile
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "lib" / "3dmodels"
MANIFEST = OUT / "MANIFEST.json"
KICAD_TAG = "10.0.6"
KICAD_RAW = "https://gitlab.com/kicad/libraries/kicad-packages3D/-/raw/" + KICAD_TAG + "/"

# Models in KiCad's own library, fetched unchanged.
KICAD = [
    "Button_Switch_SMD.3dshapes/SW_Push_1TS009xxxx-xxxx-xxxx_6x6x5mm.step",
    "Button_Switch_SMD.3dshapes/SW_SPST_TS-1088-xR020.step",
    "Capacitor_SMD.3dshapes/C_0805_2012Metric.step",
    "Connector_JST.3dshapes/JST_SH_SM04B-SRSS-TB_1x04-1MP_P1.00mm_Horizontal.step",
    "Diode_SMD.3dshapes/D_SOD-123F.step",
    "Fuse.3dshapes/Fuse_1206_3216Metric.step",
    "LED_SMD.3dshapes/LED_0805_2012Metric.step",
    "Package_TO_SOT_SMD.3dshapes/SOT-223.step",
    "Package_TO_SOT_SMD.3dshapes/SOT-23.step",
    "Resistor_SMD.3dshapes/R_0805_2012Metric.step",
    "RF_Module.3dshapes/ESP32-S3-WROOM-1.step",
    "Sensor_Audio.3dshapes/InvenSense_ICS-43434-6_3.5x2.65mm.step",
]

# Footprints whose model KiCad doesn't ship: the LCSC part's EasyEDA model, turned by `rot_z`
# degrees and then moved by `move` (mm, model axes: y up = towards the footprint's -y) so it sits
# on the KiCad footprint. Each alignment was matched against the footprint's pads and F.Fab
# outline on 2026-09-29 (HARDWARE_LESSONS, "3D models").
EASYEDA = [
    {
        "path": "Connector_USB.3dshapes/USB_C_Receptacle_HRO_TYPE-C-31-M-12.step",
        "lcsc": "C165948",
        "rot_z": 180,
        "move": [0.0, -1.05, 0.0],
        "why": "pegs land on the NPTH holes (±2.89, -2.6) and the shell legs on the SH pads "
        "(±4.32, -3.13 / 1.05); the mouth ends at the F.Fab edge y = +3.65",
    },
    {
        "path": "Button_Switch_SMD.3dshapes/SW_Push_1P1T_XKB_TS-1187A.step",
        "lcsc": "C318884",
        "rot_z": 0,
        "move": [0.0, 0.0, 0.0],
        "why": "already centred: legs at (±3.05, ±1.85) on the pads (±3, ±1.875), body = F.Fab 6.5 × 5.1",
    },
    {
        "path": "Sensor_Humidity.3dshapes/Sensirion_DFN-4_1.5x1.5mm_P0.8mm_SHT4x_NoCentralPad.step",
        "lcsc": "C2909890",
        "rot_z": 0,
        "move": [0.0, 0.0, 0.0],
        "why": "already centred: terminals at (±0.61, ±0.40) on the pads (±0.7, ±0.4), body = F.Fab 1.5 × 1.5",
    },
    {
        "path": "Connector_JST.3dshapes/JST_PH_S2B-PH-SM4-TB_1x02-1MP_P2.00mm_Horizontal.step",
        "lcsc": "C295747",
        "rot_z": 0,
        "move": [1.0, -2.75, 0.0],
        "why": "signal legs centred on pads 1-2 (x ±1, footprint y -2.85), body x ±3.95 and "
        "mouth at the F.Fab edge y = +4.40",
    },
]


def sha256(p: Path) -> str:
    return hashlib.sha256(p.read_bytes()).hexdigest()


def fetch_kicad(rel: str) -> dict:
    dst = OUT / rel
    dst.parent.mkdir(parents=True, exist_ok=True)
    with urllib.request.urlopen(KICAD_RAW + rel, timeout=60) as r:
        dst.write_bytes(r.read())
    return {"source": f"KiCad kicad-packages3D tag {KICAD_TAG}", "url": KICAD_RAW + rel}


def fetch_easyeda(e: dict) -> dict:
    import cadquery as cq

    exe = Path(sys.executable).parent / "easyeda2kicad"
    with tempfile.TemporaryDirectory() as tmp:
        subprocess.run([str(exe), "--3d", "--lcsc_id", e["lcsc"], "--output", f"{tmp}/m"],
                       check=True, capture_output=True)
        steps = list(Path(tmp).glob("m.3dshapes/*.step"))
        if len(steps) != 1:
            raise SystemExit(f"{e['lcsc']}: expected one STEP from easyeda2kicad, got {steps}")
        shape = cq.importers.importStep(str(steps[0])).val()
        name = steps[0].name
    if e["rot_z"]:
        shape = shape.rotate(cq.Vector(0, 0, 0), cq.Vector(0, 0, 1), e["rot_z"])
    shape = shape.translate(cq.Vector(*e["move"]))
    dst = OUT / e["path"]
    dst.parent.mkdir(parents=True, exist_ok=True)
    cq.exporters.export(shape, str(dst))
    bb = shape.BoundingBox()
    return {
        "source": f"EasyEDA model of LCSC {e['lcsc']} via easyeda2kicad 1.0.1 ({name})",
        "rot_z": e["rot_z"],
        "move": e["move"],
        "why": e["why"],
        "bbox": [round(v, 3) for v in (bb.xmin, bb.xmax, bb.ymin, bb.ymax, bb.zmin, bb.zmax)],
    }


def main() -> int:
    check = "--check" in sys.argv[1:]
    manifest = json.loads(MANIFEST.read_text()) if MANIFEST.exists() else {}
    wanted = KICAD + [e["path"] for e in EASYEDA]
    bad = 0
    for rel in wanted:
        dst = OUT / rel
        entry = manifest.get(rel)
        if dst.exists() and entry and sha256(dst) == entry["sha256"]:
            continue
        if check:
            print(f"MISSING or CHANGED: {rel}")
            bad += 1
            continue
        print(f"fetching {rel}")
        e = next((e for e in EASYEDA if e["path"] == rel), None)
        entry = fetch_easyeda(e) if e else fetch_kicad(rel)
        entry["sha256"] = sha256(dst)
        manifest[rel] = entry
    for rel in sorted(set(manifest) - set(wanted)):
        print(f"not in the table any more: {rel}")
        bad += check
    if not check:
        MANIFEST.write_text(json.dumps(dict(sorted(manifest.items())), indent=2) + "\n")
    print(f"{len(wanted)} models, {bad} problems")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
