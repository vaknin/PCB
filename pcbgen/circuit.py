"""Circuit-as-code model: parts, pins, nets.

Every pin of every part must end up either on a net or explicitly marked
no-connect; `Circuit.check()` enforces that before any file is written.
"""

from __future__ import annotations

from dataclasses import dataclass, field

from . import symlib


@dataclass(frozen=True)
class PinRef:
    part: "Part"
    pin: symlib.Pin

    def __repr__(self) -> str:
        return f"{self.part.ref}.{self.pin.number}({self.pin.name})"


class Net:
    def __init__(self, circuit: "Circuit", name: str):
        self.circuit = circuit
        self.name = name
        self.pins: list[PinRef] = []

    def __iadd__(self, other):
        for pr in _flatten(other):
            self.circuit._attach(self, pr)
        return self

    def __repr__(self) -> str:
        return f"Net({self.name}, {len(self.pins)} pins)"


def _flatten(x) -> list[PinRef]:
    if isinstance(x, PinRef):
        return [x]
    out: list[PinRef] = []
    for i in x:
        out += _flatten(i)
    return out


@dataclass
class Part:
    circuit: "Circuit"
    ref: str
    lib_id: str
    value: str
    footprint: str
    fields: dict[str, str] = field(default_factory=dict)
    block: str = "Misc"
    rot: int = 0                    # schematic rotation, degrees CCW
    in_bom: bool = True
    on_board: bool = True
    dnp: bool = False

    @property
    def symbol(self) -> symlib.Symbol:
        return symlib.load(self.lib_id)

    def __getitem__(self, key) -> list[PinRef]:
        """Pins by number ("3"), by name ("EN"), or several: part["A", "K"]."""
        if isinstance(key, tuple):
            return [pr for k in key for pr in self[k]]
        key = str(key)
        pins = [p for p in self.symbol.pins if p.number == key]
        if not pins:
            pins = [p for p in self.symbol.pins if p.name == key]
        if not pins:
            raise KeyError(f"{self.ref} ({self.lib_id}) has no pin {key!r}; "
                           f"pins: {sorted({p.name for p in self.symbol.pins})}")
        return [PinRef(self, p) for p in pins]

    def nc(self, *keys) -> None:
        for k in keys:
            for pr in self[k]:
                self.circuit._mark_nc(pr)


class Circuit:
    def __init__(self, name: str, title: str, rev: str = "0", company: str = ""):
        self.name = name
        self.title = title
        self.rev = rev
        self.company = company
        self.parts: dict[str, Part] = {}
        self.nets: dict[str, Net] = {}
        self._pin_net: dict[tuple[str, str], Net] = {}
        self._nc: set[tuple[str, str]] = set()
        self.pwr_flags: list[str] = []

    def net(self, name: str) -> Net:
        if name not in self.nets:
            self.nets[name] = Net(self, name)
        return self.nets[name]

    def part(self, ref: str, lib_id: str, value: str, footprint: str, *,
             lcsc: str | None = None, mpn: str | None = None, block: str = "Misc",
             rot: int = 0, in_bom: bool = True, dnp: bool = False, **extra: str) -> Part:
        if ref in self.parts:
            raise ValueError(f"duplicate reference {ref}")
        fields = {}
        if lcsc:
            fields["LCSC"] = lcsc
        if mpn:
            fields["MPN"] = mpn
        fields.update(extra)
        p = Part(self, ref, lib_id, value, footprint, fields, block, rot, in_bom, True, dnp)
        p.symbol  # load now so bad lib ids fail at the line that wrote them
        self.parts[ref] = p
        return p

    def pwr_flag(self, *nets: Net) -> None:
        """Declare that a net is driven from off-sheet (e.g. VBUS from the USB cable)."""
        self.pwr_flags += [n.name for n in nets]

    # -- internals ------------------------------------------------------
    def _colocated(self, pr: PinRef) -> list[PinRef]:
        """Pins stacked at one location are one electrical node in KiCad."""
        return [PinRef(pr.part, p) for p in pr.part.symbol.pins
                if (p.x, p.y) == (pr.pin.x, pr.pin.y)]

    def _attach(self, net: Net, pr: PinRef) -> None:
        for q in self._colocated(pr):
            k = (q.part.ref, q.pin.number)
            if k in self._nc:
                raise ValueError(f"{q} is marked no-connect but is being connected to {net.name}")
            prev = self._pin_net.get(k)
            if prev is net:
                continue
            if prev is not None:
                raise ValueError(f"{q} is already on net {prev.name}; cannot also join {net.name}")
            self._pin_net[k] = net
            net.pins.append(q)

    def _mark_nc(self, pr: PinRef) -> None:
        for q in self._colocated(pr):
            k = (q.part.ref, q.pin.number)
            if k in self._pin_net:
                raise ValueError(f"{q} is on net {self._pin_net[k].name}; cannot mark no-connect")
            self._nc.add(k)

    def net_of(self, part: Part, pin: symlib.Pin) -> Net | None:
        return self._pin_net.get((part.ref, pin.number))

    def is_nc(self, part: Part, pin: symlib.Pin) -> bool:
        return (part.ref, pin.number) in self._nc

    def check(self) -> list[str]:
        """Structural checks the circuit author must satisfy. Returns problems."""
        problems = []
        for part in self.parts.values():
            for p in part.symbol.pins:
                if p.etype == "no_connect":
                    continue
                if not self.net_of(part, p) and not self.is_nc(part, p):
                    problems.append(f"{part.ref} pin {p.number} ({p.name}) is neither connected nor marked nc")
        for net in self.nets.values():
            refs = {pr.part.ref for pr in net.pins}
            if len(net.pins) < 2 and net.name not in self.pwr_flags:
                problems.append(f"net {net.name} has only {len(net.pins)} pin(s)")
            elif len(refs) < 2 and net.name not in self.pwr_flags:
                problems.append(f"net {net.name} only touches {refs}")
        return problems
