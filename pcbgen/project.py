"""Write the KiCad project file (.kicad_pro) and fab design rules (.kicad_dru).

Our own board rules are deliberately looser than the fab's minimums, so a DRC
pass against the fab rules (the .kicad_dru) has margin to spare.
"""

from __future__ import annotations

import json
import shutil
from dataclasses import dataclass, field
from pathlib import Path

RULES_DIR = Path(__file__).resolve().parent.parent / "rules"


@dataclass
class NetClass:
    name: str
    track: float          # mm
    clearance: float
    via_diameter: float
    via_drill: float
    patterns: list[str] = field(default_factory=list)


@dataclass
class BoardRules:
    fab_rules: str = "JLCPCB-2L-1oz.kicad_dru"
    min_track: float = 0.15
    min_clearance: float = 0.15
    min_via_diameter: float = 0.5
    min_via_drill: float = 0.3
    min_hole: float = 0.3
    edge_clearance: float = 0.5
    classes: list[NetClass] = field(default_factory=lambda: [
        NetClass("Default", 0.2, 0.2, 0.6, 0.3),
    ])


def _class(nc: NetClass, priority: int) -> dict:
    return {
        "name": nc.name, "priority": priority,
        "clearance": nc.clearance, "track_width": nc.track,
        "via_diameter": nc.via_diameter, "via_drill": nc.via_drill,
        "microvia_diameter": 0.3, "microvia_drill": 0.1,
        "diff_pair_width": 0.2, "diff_pair_gap": 0.25, "diff_pair_via_gap": 0.25,
        "bus_width": 12, "wire_width": 6, "line_style": 0,
        "pcb_color": "rgba(0, 0, 0, 0.000)", "schematic_color": "rgba(0, 0, 0, 0.000)",
    }


def write(out_dir: Path, name: str, rules: BoardRules) -> Path:
    out_dir.mkdir(parents=True, exist_ok=True)
    classes = rules.classes
    assert classes[0].name == "Default"
    tracks = sorted({c.track for c in classes})
    vias = sorted({(c.via_diameter, c.via_drill) for c in classes})
    pro = {
        "meta": {"filename": f"{name}.kicad_pro", "version": 3},
        "board": {
            "design_settings": {
                "rules": {
                    "min_track_width": rules.min_track,
                    "min_clearance": rules.min_clearance,
                    "min_via_diameter": rules.min_via_diameter,
                    "min_through_hole_diameter": rules.min_hole,
                    "min_copper_edge_clearance": rules.edge_clearance,
                    "min_hole_clearance": 0.25,
                    "min_hole_to_hole": 0.5,
                    "min_via_annular_width": 0.1,
                    "min_silk_clearance": 0.0,
                    "min_text_height": 0.8,
                    "min_text_thickness": 0.15,
                    "solder_mask_to_copper_clearance": 0.0,
                    "max_error": 0.005,
                    "min_resolved_spokes": 2,
                    "use_height_for_length_calcs": True,
                },
                # first entry 0 = "use net class"; KiCad expects that slot
                "track_widths": [0.0, *tracks],
                "via_dimensions": [{"diameter": 0.0, "drill": 0.0},
                                   *[{"diameter": d, "drill": h} for d, h in vias]],
                "diff_pair_dimensions": [{"gap": 0.0, "via_gap": 0.0, "width": 0.0}],
                "meta": {"version": 2},
            },
        },
        "net_settings": {
            "classes": [_class(c, 2147483647 if c.name == "Default" else i)
                        for i, c in enumerate(classes)],
            "meta": {"version": 4},
            "net_colors": None,
            "netclass_assignments": None,
            "netclass_patterns": [{"netclass": c.name, "pattern": p}
                                  for c in classes for p in c.patterns],
        },
        "schematic": {"meta": {"version": 1}},
        "erc": {"meta": {"version": 0}},
        "pcbnew": {"page_layout_descr_file": ""},
        "libraries": {"pinned_footprint_libs": [], "pinned_symbol_libs": []},
        "sheets": [],
        "text_variables": {},
    }
    path = out_dir / f"{name}.kicad_pro"
    path.write_text(json.dumps(pro, indent=2) + "\n")
    shutil.copyfile(RULES_DIR / rules.fab_rules, out_dir / f"{name}.kicad_dru")
    return path
