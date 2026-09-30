//! Capture clip, revision A: board shape, placement and rules.
//!
//! FIRST INTENT ONLY (Phase E.1): these positions have never been through the `pcb`, `route`
//! or DRC stages. Layout proper is Phase E.3, after the datasheet check (E.2). Nothing here
//! is verified; it exists so the crate compiles and the net classes are checked by `sch`.
//!
//! 30 x 60 mm (the spec's target). Coordinates are mm from the board's top-left corner, Y
//! down. The module sits at the top with its antenna flush with the top edge (clear of the
//! battery and the hand); USB-C is flush with the bottom edge; the battery lies under the
//! lower half of the board, in the case. The microphone is kept outside the battery's
//! footprint, its sound hole going through the board to a chimney in the case. The button
//! and the light are on the centre line, where the case's lid wants them.

use pcbgen::layout::{BoardSpec, Layout, RouteOptions, Waiver, at, at_rot, silk};
use pcbgen::project::{BoardRules, NetClass};

const W: f64 = 30.0;
const H: f64 = 60.0;

/// Module body is 18 x 25.5 mm; top edge at y = 0.
const MOD_X: f64 = W / 2.0;
const MOD_Y: f64 = 12.75;

pub fn layout() -> Layout {
    let rules = BoardRules {
        classes: vec![
            // 0.2/0.15: the microphone's pads are 0.3 mm apart; JLCPCB's floor is 0.1/0.1.
            NetClass::new("Default", 0.2, 0.15, 0.6, 0.3),
            // 0.3 mm on 1 oz copper carries ~1 A at a 10 C rise (IPC-2221); peak draw is ~0.4 A.
            // KiCad names local nets "/VSYS".
            NetClass::new("Power", 0.3, 0.2, 0.8, 0.4).patterns(&["VBUS", "+BATT", "*VSYS", "+3V3", "GND"]),
            NetClass::new("USB", 0.3, 0.15, 0.6, 0.3).patterns(&["*USB_D*"]),
        ],
        ..Default::default()
    };

    let mut spec = BoardSpec::new(W, H);
    spec.corner_radius = 2.0;
    spec.places = [
        // ESP32-S3 module (pads at x = 6.25 and 23.75)
        ("U1", at(MOD_X, MOD_Y)),
        ("C5", at_rot(3.2, 9.0, 90.0)),  // 22u at the 3V3 pad
        ("C6", at_rot(3.2, 12.6, 90.0)), // 100n
        ("R7", at_rot(3.2, 16.5, 90.0)), // EN pull-up
        ("C7", at_rot(3.2, 20.0, 90.0)), // EN delay
        // two M2 holes, below the module, at the edges
        ("H1", at(3.0, 29.0)),
        ("H2", at(W - 3.0, 29.0)),
        // main button and the colour light on the centre line
        ("SW1", at(MOD_X, 33.0)),
        ("R8", at(MOD_X, 28.0)),
        ("D4", at(MOD_X, 39.0)),
        ("R10", at(10.5, 39.0)),
        ("R11", at(19.5, 39.0)),
        ("R12", at(19.5, 41.5)),
        // microphone, right edge (outside the battery's footprint; decided in E.3)
        ("MK1", at(26.0, 36.0)),
        ("C8", at_rot(26.5, 40.0, 90.0)),
        ("R9", at_rot(24.0, 40.0, 90.0)),
        ("R18", at_rot(22.0, 36.0, 90.0)), // 0 ohm between IO6 and the mic's VDD
        // regulator, left
        ("U4", at(4.5, 36.0)),
        ("C3", at_rot(4.5, 39.5, 0.0)),
        ("C4", at_rot(8.5, 36.0, 90.0)),
        // power path and charger, lower left
        ("Q1", at(4.0, 43.5)),
        ("D2", at(9.0, 43.5)),
        ("R6", at_rot(13.0, 43.5, 90.0)),
        ("U2", at(5.0, 48.0)),
        ("R3", at_rot(9.0, 48.0, 90.0)),
        ("C1", at_rot(2.0, 52.0, 90.0)),
        ("C2", at_rot(5.0, 52.0, 90.0)),
        ("R4", at_rot(2.5, 56.5, 90.0)),
        ("D1", at_rot(5.5, 56.5, 90.0)), // charge light, next to the USB-C mouth
        ("R5", at_rot(11.5, 46.5, 90.0)),
        // battery connector on the right edge, mouth facing +x; divider next to it
        ("J2", at_rot(25.6, 47.0, 90.0)),
        ("R13", at_rot(18.0, 44.5, 90.0)),
        ("R14", at_rot(20.2, 44.5, 90.0)),
        ("R15", at_rot(18.0, 48.0, 90.0)),
        ("C9", at_rot(20.2, 48.0, 90.0)),
        // USB-C, bottom centre; receptacle body ends flush with the bottom edge
        ("J1", at(MOD_X, H - 3.7)),
        ("R1", at(11.5, 50.5)),
        ("R2", at(18.5, 50.5)),
        ("U3", at_rot(MOD_X, 47.5, 90.0)),
        ("D3", at_rot(8.5, 54.0, 90.0)),
        ("R16", at_rot(22.0, 54.0, 90.0)),
        ("R17", at_rot(24.5, 54.0, 90.0)),
        // reset, bottom right (a pinhole in the case)
        ("SW2", at(26.0, 57.5)),
        // test points: power row, then UART and the spares
        ("TP1", at(10.0, 57.0)),
        ("TP2", at(9.0, 40.0)),
        ("TP3", at(22.5, 50.5)),
        ("TP4", at(2.5, 24.0)),
        ("TP5", at(2.5, 32.5)),
        ("TP6", at(27.5, 24.0)),
        ("TP7", at(27.5, 20.5)),
        ("TP8", at(27.5, 17.0)),
        ("TP9", at(27.5, 32.5)),
    ]
    .into_iter()
    .map(|(r, p)| (r.to_string(), p))
    .collect();
    spec.texts = vec![
        // battery polarity (spec: JST PH polarity is not standard; + is marked on the silk).
        // Pad 1 = + is UNVERIFIED (circuit.rs, J2); the labels follow it.
        silk("+", 22.0, 45.8, 1.5),
        silk("-", 22.0, 48.2, 1.5),
        silk("LiPo 3.7V", 25.0, 42.5, 1.0),
        silk("RESET", 26.0, 54.8, 1.0),
    ];

    Layout { rules, spec, route: RouteOptions::default(), waivers: Vec::<Waiver>::new() }
}
