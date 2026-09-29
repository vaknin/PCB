//! ESP32-S3 starter board, Rev 0: board shape, placement and rules.
//!
//! 50 x 50 mm (>= NextPCB Rev 0 minimum, D-002). Coordinates are mm from the board's
//! top-left corner, Y pointing down; each place is the footprint's own origin.
//!
//! The module sits top-centre with its antenna flush with the top edge. Its footprint
//! carries Espressif's antenna keep-out (no copper or parts in a 48 mm wide band level
//! with the antenna, and 15 mm beyond it, which is off-board here). Power enters
//! bottom-centre (USB-C) and goes left through the fuse and TVS to the regulator (lower
//! left), then up to the module's 3V3 pad. The sensor sits bottom-right, the spot
//! furthest from the regulator and the module (both run warm); the Qwiic port is on the
//! right edge above it.

use pcbgen::layout::{BoardSpec, CopperZone, Layout, RouteOptions, Text, Waiver, at, at_rot, silk};
use pcbgen::project::{BoardRules, NetClass};

const W: f64 = 50.0;
const H: f64 = 50.0;

/// Module body is 25.5 mm tall; top edge at y = 0.
const MOD_X: f64 = W / 2.0;
const MOD_Y: f64 = 12.75;

pub fn layout() -> Layout {
    let rules = BoardRules {
        classes: vec![
            // 0.2/0.15 lets signals escape the SHT40's 0.3 mm pads (0.8 mm pitch); JLCPCB's floor is 0.1/0.1.
            NetClass::new("Default", 0.2, 0.15, 0.6, 0.3),
            // 0.3 mm on 1 oz outer copper carries ~1 A at a 10 C rise (IPC-2221); peak draw is ~0.5 A.
            // 0.4 mm left the SHT40's 3V3 pad unroutable (its 0.8 mm-pitch escape stubs are too tight).
            NetClass::new("Power", 0.3, 0.2, 0.8, 0.4).patterns(&["VBUS", "+5V", "+3V3", "GND"]),
            // KiCad names local nets "/USB_D+"
            NetClass::new("USB", 0.3, 0.15, 0.6, 0.3).patterns(&["*USB_D*"]),
        ],
        ..Default::default()
    };

    let mut spec = BoardSpec::new(W, H);
    spec.corner_radius = 2.0;
    spec.places = [
        // ESP32-S3 module
        ("U1", at(MOD_X, MOD_Y)),
        ("C3", at_rot(13.2, 9.0, 90.0)),  // 22u at the 3V3 pad (D-014) (pad 2 at x=16.25, y=8.76)
        ("C4", at_rot(13.2, 12.6, 90.0)), // 100n
        ("R4", at(12.0, 16.5)),           // EN pull-up (EN = pad 3 at y=10.03)
        ("C5", at(12.0, 19.0)),           // EN delay cap
        // USB-C input, bottom centre; receptacle body ends flush with the bottom edge
        ("J1", at(MOD_X, H - 3.7)),
        ("R1", at(21.5, 39.0)),             // CC1 pull-down
        ("R2", at(28.5, 39.0)),             // CC2 pull-down
        ("U3", at_rot(MOD_X, 36.0, 90.0)), // D+/D- ESD, on the way from connector to module
        // VBUS goes left: TVS -> fuse -> regulator
        ("D3", at_rot(17.0, 40.0, 90.0)),
        ("F1", at_rot(17.0, 35.0, 90.0)),
        // regulator, lower left
        ("U2", at_rot(7.5, 26.0, 90.0)),
        ("C1", at_rot(7.5, 31.8, 180.0)), // LDO input cap under U2: +5V pad at pin 3, GND pad at pin 1
        ("C2", at_rot(11.0, 33.5, 90.0)),
        ("R3", at(4.5, 36.5)), // power LED, fed from +5V
        ("D1", at(4.5, 39.0)),
        // buttons, bottom corners
        ("SW1", at(11.5, 44.5)),
        ("SW2", at(38.5, 44.5)),
        ("R5", at(38.5, 39.5)),
        // status LED
        ("R6", at(37.5, 30.0)),
        ("D2", at(37.5, 33.0)),
        // Qwiic on the right edge (mouth faces +x); I2C pull-ups where the bus leaves the module
        ("J2", at_rot(46.3, 24.0, 90.0)),
        ("R7", at_rot(40.0, 15.5, 90.0)),
        ("R8", at_rot(42.2, 15.5, 90.0)),
        // sensor bottom-right: furthest from the module and the regulator (both warm)
        // rot 270: SDA/SCL pads face up to the bus, VDD/GND pads face down to C6
        ("U4", at_rot(46.5, 33.0, 270.0)),
        ("C6", at(46.5, 36.0)),
        // test points: power row under the module, UART and I2C on the right
        ("TP1", at(14.0, 30.5)),
        ("TP2", at(17.5, 30.5)),
        ("TP3", at(21.0, 30.5)),
        ("TP4", at(37.5, 21.0)),
        ("TP5", at(37.5, 24.5)),
        ("TP6", at(41.5, 29.0)),
        ("TP7", at(44.5, 29.0)),
        // mounting holes, clear of the antenna band (y < 6)
        ("H1", at(4.0, 10.0)),
        ("H2", at(W - 4.0, 10.0)),
        ("H3", at(4.0, H - 4.0)),
        ("H4", at(W - 4.0, H - 4.0)),
    ]
    .into_iter()
    .map(|(r, p)| (r.to_string(), p))
    .collect();
    // Cooling copper for the regulator: its tab (pad 2, +3V3, at (7.5, 22.85)) spreads heat
    // into a +3V3 pour on both layers, tied together by the vias from `route.stitch_local`.
    // ~0.4 W at 0.25 A (inferred). Above the GND pours in priority; parts inside it
    // (R4, C5, U2's other pads) keep their clearance.
    spec.zones = vec![CopperZone::new("+3V3", 1.0, 14.0, 13.0, 26.0)];
    // Owner-facing labels (reference designators live on the fab layer)
    spec.texts = vec![
        silk("5V", 14.0, 28.8, 1.0),
        silk("3V3", 17.5, 28.8, 1.0),
        silk("GND", 21.0, 28.8, 1.0),
        silk("TX", 37.5, 19.2, 1.0),
        silk("RX", 37.5, 26.3, 1.0),
        silk("SDA", 41.5, 30.8, 1.0),
        silk("SCL", 44.5, 30.8, 1.0),
        silk("PWR", 4.5, 40.8, 1.0),
        silk("LED", 40.6, 33.0, 1.0),
        silk("RESET", 11.5, 48.4, 1.0),
        silk("BOOT", 38.5, 48.4, 1.0),
        silk("Qwiic", 46.3, 19.3, 1.0),
        Text { layer: "B.SilkS".into(), ..silk("ESP32-S3 starter  Rev 0", W / 2.0, 30.0, 1.2) },
    ];

    let route = RouteOptions { stitch_local: vec![("+3V3".into(), 2.0)], ..Default::default() };

    let waivers = vec![
        Waiver {
            kind: "silk_over_copper",
            substring: "Circle of TP",
            reason: "KiCad's test-point footprints all draw their silk ring 0.14 mm from the pad, 0.01 mm \
                     under the JLCPCB guideline; the fab trims silk off exposed copper, so at worst the \
                     ring loses a sliver. Kept unmodified so the footprint still matches the library.",
        },
        Waiver {
            kind: "silk_edge_clearance",
            substring: "Segment of U1 on F.Silkscreen",
            reason: "The module's antenna end is flush with the top edge by design (Espressif layout \
                     guidance); its outline silk there is trimmed by the fab. No copper is involved.",
        },
        Waiver {
            kind: "silk_edge_clearance",
            substring: "Segment of J1 on F.Silkscreen",
            reason: "The USB-C receptacle's mouth is flush with the bottom edge by design; its outline \
                     silk there is trimmed by the fab. No copper is involved.",
        },
    ];

    Layout { rules, spec, route, waivers }
}
