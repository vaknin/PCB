"""KiCad's SWIG `pcbnew` module, patched for Python 3.14.

pcbnew's container __iter__ calls `it.next()`, which SwigPyIterator no longer
has under Python 3.14 (only `__next__`), so every `for x in board.Tracks()`
raises AttributeError. Aliasing it back fixes all of them. Import pcbnew from
here, never directly.
"""

import pcbnew

if not hasattr(pcbnew.SwigPyIterator, "next"):
    pcbnew.SwigPyIterator.next = pcbnew.SwigPyIterator.__next__

__all__ = ["pcbnew"]
