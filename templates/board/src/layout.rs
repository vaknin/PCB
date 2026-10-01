//! __NAME__, Rev 0: board shape, placement and rules. Made by `pcb new` from templates/board.
//!
//! 50 x 50 mm. Coordinates are mm from the board's top-left corner, Y pointing down; each
//! place is the footprint's own origin. Placement follows the starter board's: the module
//! top-centre with its antenna flush with the top edge (its footprint carries Espressif's
//! antenna keep-out), USB-C bottom-centre, power going left through the fuse and TVS to the
//! regulator (lower left), then up to the module's 3V3 pad. The right half is free.

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
            NetClass::new("Default", 0.2, 0.15, 0.6, 0.3),
            // 0.3 mm on 1 oz outer copper carries ~1 A at a 10 C rise (IPC-2221)
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
        ("C3", at_rot(13.2, 9.0, 90.0)),  // 22u at the 3V3 pad
        ("C4", at_rot(13.2, 12.6, 90.0)), // 100n
        ("R4", at(12.0, 16.5)),           // EN pull-up
        ("C5", at(12.0, 19.0)),           // EN delay cap
        // USB-C input, bottom centre; receptacle body ends flush with the bottom edge
        ("J1", at(MOD_X, H - 3.7)),
        ("R1", at(21.5, 39.0)),            // CC1 pull-down
        ("R2", at(28.5, 39.0)),            // CC2 pull-down
        ("U3", at_rot(MOD_X, 36.0, 90.0)), // D+/D- ESD
        // VBUS goes left: TVS -> fuse -> regulator
        ("D3", at_rot(17.0, 40.0, 90.0)),
        ("F1", at_rot(17.0, 35.0, 90.0)),
        // regulator, lower left
        ("U2", at_rot(7.5, 26.0, 90.0)),
        ("C1", at_rot(7.5, 31.8, 180.0)),
        ("C2", at_rot(11.0, 33.5, 90.0)),
        ("R3", at(4.5, 36.5)), // power light, fed from +5V
        ("D1", at(4.5, 39.0)),
        // buttons, bottom corners
        ("SW1", at(11.5, 44.5)),
        ("SW2", at(38.5, 44.5)),
        ("R5", at(38.5, 39.5)),
        // status light
        ("R6", at(37.5, 30.0)),
        ("D2", at(37.5, 33.0)),
        // mounting holes, clear of the antenna band (y < 6)
        ("H1", at(4.0, 10.0)),
        ("H2", at(W - 4.0, 10.0)),
        ("H3", at(4.0, H - 4.0)),
        ("H4", at(W - 4.0, H - 4.0)),
    ]
    .into_iter()
    .map(|(r, p)| (r.to_string(), p))
    .collect();
    // Cooling copper for the regulator: its tab (+3V3) spreads heat into a +3V3 pour on both
    // layers, tied together by the vias from `route.stitch_local`.
    spec.zones = vec![CopperZone::new("+3V3", 1.0, 14.0, 13.0, 26.0)];
    spec.texts = vec![
        silk("PWR", 4.5, 40.8, 1.0),
        silk("LED", 40.6, 33.0, 1.0),
        silk("RESET", 11.5, 48.4, 1.0),
        silk("BOOT", 38.5, 48.4, 1.0),
        Text { layer: "B.SilkS".into(), ..silk("__NAME__  Rev 0", W / 2.0, 30.0, 1.2) },
    ];

    let route = RouteOptions { stitch_local: vec![("+3V3".into(), 2.0)], ..Default::default() };

    let waivers = vec![
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
