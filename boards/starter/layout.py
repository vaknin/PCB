"""ESP32-S3 starter board, Rev 0: board shape, placement and rules.

50 x 50 mm (>= NextPCB Rev 0 minimum, D-002). Coordinates are mm from the board's
top-left corner, Y pointing down; each Place is the footprint's own origin.

The module sits top-centre with its antenna flush with the top edge. Its
footprint carries Espressif's antenna keep-out (no copper or parts in a 48 mm
wide band level with the antenna, and 15 mm beyond it, which is off-board here).
Power enters bottom-centre (USB-C) and goes left through the fuse and TVS to the
regulator (lower left), then up to the module's 3V3 pad. The sensor sits
bottom-right, the spot furthest from the regulator and the module (both run
warm); the Qwiic port is on the right edge above it.
"""

from pcbgen.pcb import BoardSpec, Place, Text
from pcbgen.project import BoardRules, NetClass
from pcbgen.route import RouteOptions

W, H = 50.0, 50.0

MOD_X, MOD_Y = W / 2, 12.75          # module body is 25.5 mm tall; top edge at y = 0

RULES = BoardRules(classes=[
    # 0.2/0.15 lets signals escape the SHT40's 0.3 mm pads (0.8 mm pitch); JLCPCB's floor is 0.1/0.1.
    NetClass("Default", track=0.2, clearance=0.15, via_diameter=0.6, via_drill=0.3),
    # 0.3 mm on 1 oz outer copper carries ~1 A at a 10 C rise (IPC-2221); peak draw is ~0.5 A.
    # 0.4 mm left the SHT40's 3V3 pad unroutable (its 0.8 mm-pitch escape stubs are too tight).
    NetClass("Power", track=0.3, clearance=0.2, via_diameter=0.8, via_drill=0.4,
             patterns=["VBUS", "+5V", "+3V3", "GND"]),
    NetClass("USB", track=0.3, clearance=0.15, via_diameter=0.6, via_drill=0.3,
             patterns=["*USB_D*"]),   # KiCad names local nets "/USB_D+"
])

SPEC = BoardSpec(
    width=W, height=H, corner_radius=2.0,
    places={
        # ESP32-S3 module
        "U1": Place(MOD_X, MOD_Y),
        "C3": Place(13.2, 9.0, 90),        # 10u at the 3V3 pad (pad 2 at x=16.25, y=8.76)
        "C4": Place(13.2, 12.6, 90),       # 100n
        "R4": Place(12.0, 16.5, 0),        # EN pull-up (EN = pad 3 at y=10.03)
        "C5": Place(12.0, 19.0, 0),        # EN delay cap
        # USB-C input, bottom centre; receptacle body ends flush with the bottom edge
        "J1": Place(MOD_X, H - 3.7),
        "R1": Place(21.5, 39.0, 0),        # CC1 pull-down
        "R2": Place(28.5, 39.0, 0),        # CC2 pull-down
        "U3": Place(MOD_X, 36.0, 90),      # D+/D- ESD, on the way from connector to module
        # VBUS goes left: TVS -> fuse -> regulator
        "D3": Place(17.0, 40.0, 90),
        "F1": Place(17.0, 35.0, 90),
        # regulator, lower left
        "U2": Place(7.5, 26.0, 90),
        "C1": Place(7.5, 31.8, 180),       # LDO input cap under U2: +5V pad at pin 3, GND pad at pin 1
        "C2": Place(11.0, 33.5, 90),
        "R3": Place(4.5, 36.5, 0),         # power LED, fed from +5V
        "D1": Place(4.5, 39.0, 0),
        # buttons, bottom corners
        "SW1": Place(11.5, 44.5),
        "SW2": Place(38.5, 44.5),
        "R5": Place(38.5, 39.5, 0),
        # status LED
        "R6": Place(37.5, 30.0, 0),
        "D2": Place(37.5, 33.0, 0),
        # Qwiic on the right edge (mouth faces +x); I2C pull-ups where the bus leaves the module
        "J2": Place(46.3, 24.0, 90),
        "R7": Place(40.0, 15.5, 90),
        "R8": Place(42.2, 15.5, 90),
        # sensor bottom-right: furthest from the module and the regulator (both warm)
        # rot 270: SDA/SCL pads face up to the bus, VDD/GND pads face down to C6
        "U4": Place(46.5, 33.0, 270),
        "C6": Place(46.5, 36.0, 0),
        # test points: power row under the module, UART and I2C on the right
        "TP1": Place(14.0, 30.5), "TP2": Place(17.5, 30.5), "TP3": Place(21.0, 30.5),
        "TP4": Place(37.5, 21.0), "TP5": Place(37.5, 24.5),
        "TP6": Place(41.5, 29.0), "TP7": Place(44.5, 29.0),
        # mounting holes, clear of the antenna band (y < 6)
        "H1": Place(4.0, 10.0), "H2": Place(W - 4.0, 10.0),
        "H3": Place(4.0, H - 4.0), "H4": Place(W - 4.0, H - 4.0),
    },
    # Owner-facing labels (reference designators live on the fab layer)
    texts=[
        Text("5V", 14.0, 28.8, 1.0), Text("3V3", 17.5, 28.8, 1.0), Text("GND", 21.0, 28.8, 1.0),
        Text("TX", 37.5, 19.2, 1.0), Text("RX", 37.5, 26.3, 1.0),
        Text("SDA", 41.5, 30.8, 1.0), Text("SCL", 44.5, 30.8, 1.0),
        Text("PWR", 4.5, 40.8, 1.0), Text("LED", 40.6, 33.0, 1.0),
        Text("RESET", 11.5, 48.4, 1.0), Text("BOOT", 38.5, 48.4, 1.0),
        Text("Qwiic", 46.3, 19.3, 1.0),
        Text("ESP32-S3 starter  Rev 0", W / 2, 30.0, 1.2, layer="B.SilkS"),
    ],
)

ROUTE = RouteOptions()

# (DRC/ERC violation type, text that must appear in the violation, reason)
WAIVERS: list[tuple[str, str, str]] = [
    ("silk_over_copper", "Circle of TP",
     "KiCad's test-point footprints all draw their silk ring 0.14 mm from the pad, 0.01 mm "
     "under the JLCPCB guideline; the fab trims silk off exposed copper, so at worst the "
     "ring loses a sliver. Kept unmodified so the footprint still matches the library."),
    ("silk_edge_clearance", "Segment of U1 on F.Silkscreen",
     "The module's antenna end is flush with the top edge by design (Espressif layout "
     "guidance); its outline silk there is trimmed by the fab. No copper is involved."),
    ("silk_edge_clearance", "Segment of J1 on F.Silkscreen",
     "The USB-C receptacle's mouth is flush with the bottom edge by design; its outline "
     "silk there is trimmed by the fab. No copper is involved."),
]
