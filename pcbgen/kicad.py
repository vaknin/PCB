"""KiCad's SWIG `pcbnew` module, patched for Python 3.14, plus the helpers that keep it safe.

pcbnew's container __iter__ calls `it.next()`, which SwigPyIterator no longer
has under Python 3.14 (only `__next__`), so every `for x in board.Tracks()`
raises AttributeError. Aliasing it back fixes all of them. Import pcbnew from
here, never directly.

SWIG pcbnew is removed in KiCad 11; keep its use to the few steps nothing else can do,
each run in its own process (`run_step`).
"""

import signal
import subprocess
import sys
import tempfile
from pathlib import Path

import pcbnew

if not hasattr(pcbnew.SwigPyIterator, "next"):
    pcbnew.SwigPyIterator.next = pcbnew.SwigPyIterator.__next__

__all__ = ["pcbnew", "borrowed", "fill_zones", "run_step"]


def borrowed(obj):
    """Mark a SWIG proxy for an object KiCad owns (a zone's Outline(), a footprint's
    GetCourtyard(), ...) as not Python's.

    pcbnew's SWIG wrappers hand these back with thisown=True, so Python deletes KiCad's
    own polygon when the proxy is garbage-collected. The heap corruption then segfaults
    later, somewhere unrelated (seen: inside an import). Use the result at once, or copy
    it (`pcbnew.SHAPE_POLY_SET(borrowed(...))`) if it must outlive a board edit.
    """
    obj.thisown = False
    return obj


def run_step(module: str, *args: str) -> None:
    """Run `python -m <module> <args>` in a fresh process: one pcbnew step per process.

    SWIG pcbnew can corrupt the heap and crash later somewhere unrelated; in its own
    process a crash stays inside that step and is reported by name.
    """
    res = subprocess.run([sys.executable, "-X", "faulthandler", "-u", "-m", module, *args],
                         cwd=Path(__file__).resolve().parent.parent)
    if res.returncode < 0:
        raise RuntimeError(f"{module} {args[0]} crashed ({signal.Signals(-res.returncode).name})")
    if res.returncode:
        raise RuntimeError(f"{module} {args[0]} failed (exit {res.returncode})")


def fill_zones(pcb_path: Path) -> None:
    """Refill every zone with kicad-cli (its DRC refills and saves), not SWIG's ZONE_FILLER."""
    with tempfile.TemporaryDirectory() as tmp:
        report = Path(tmp) / "fill.json"
        res = subprocess.run(["kicad-cli", "pcb", "drc", "--refill-zones", "--save-board",
                              "--format", "json", "-o", str(report), str(pcb_path)],
                             capture_output=True, text=True)
        if not report.exists():
            raise RuntimeError(f"zone fill failed:\n{res.stdout}\n{res.stderr}")
