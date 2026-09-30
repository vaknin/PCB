//! Capture clip, revision A: board shape, placement and rules (Phase E.3).
//!
//! 30 x 60 mm, top side only. Coordinates are mm from the board's top-left corner, Y down.
//! - Top end: the module, antenna flush with the top edge; nothing but the module above y = 6.
//! - Beside the module: decoupling and the EN parts on the left, the microphone and the UART
//!   test points on the right. The microphone's sound hole goes through the board (its
//!   footprint's Ø0.5 hole) and opens on the back, outside the battery's footprint; the case
//!   has a chimney under it.
//! - Centre line below the module: the colour light (y 28), then the main button (y 34): the
//!   thumb comes from the USB end, so the light stays visible above it.
//! - Right column: the battery connector, mouth facing the antenna end, with a clear strip in
//!   front of it for the plug and a notch in the right edge where the lead comes up from the
//!   battery under the board.
//! - Left column: regulator, power path. Bottom edge: USB-C in the centre; the reset button and
//!   the charge light left of it; the charger and the second screw right of it (the charger
//!   warms up, so it sits beside the battery pocket, not over it).
//! - The battery (36 x 17 mm, board.toml [case.battery]) lies under the board at x 6.5..23.5,
//!   y 18..54: clear of the microphone's chimney (x 27.3), of both screw standoffs, of the USB-C
//!   shell legs (y > 53) and of the charger (y > 54). The two M2 holes are on a diagonal
//!   (top-left, bottom-right).
//! - Charger heat (up to ~0.67 W at 333 mA into a 3.0 V cell; the TP4057 throttles itself at
//!   120 C): U2 sits in the GND pours of both layers at the USB end, past the cell's end.

use pcbgen::layout::{BoardSpec, Edge, Layout, Notch, RouteOptions, Waiver, at, at_rot, silk};
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
    // the battery lead's way from under the board up to J2's plug
    spec.notches = vec![Notch { edge: Edge::Right, at: 33.0, width: 4.0, depth: 2.5 }];
    spec.places = [
        // ESP32-S3 module: body x 6..24, y 0..25.5; courtyard x 5.25..24.75
        ("U1", at(MOD_X, MOD_Y)),
        // left strip: decoupling at the 3V3 pad (pad 2, y 8.76), a screw, the EN parts
        ("C5", at_rot(3.0, 8.6, 90.0)),  // 22u
        ("C6", at_rot(3.0, 12.2, 90.0)), // 100n
        ("H1", at(2.5, 16.7)),
        ("R7", at(2.2, 22.2)), // EN pull-up
        ("C7", at(2.2, 24.4)), // EN delay
        // right strip: UART test points, the microphone and its parts
        ("TP6", at(27.4, 8.0)),  // TX
        ("TP7", at(27.4, 11.0)), // RX
        ("R18", at(27.4, 13.6)), // 0 ohm, IO6 to the mic's VDD
        ("C8", at(27.4, 15.8)),
        ("MK1", at(27.3, 19.6)), // sound hole at (27.3, 20.31)
        ("R9", at(27.4, 23.0)),
        // row under the module: light resistors, the colour light, button pull-up, battery divider
        ("R10", at_rot(8.0, 28.3, 90.0)),
        ("R11", at_rot(10.2, 28.3, 90.0)),
        ("R12", at_rot(12.4, 28.3, 90.0)),
        ("D4", at(MOD_X, 28.2)),
        ("R8", at_rot(18.0, 28.3, 90.0)),
        ("TP8", at(21.0, 28.0)), // spare IO21
        ("R15", at_rot(23.6, 28.3, 90.0)),
        ("C9", at_rot(25.8, 28.3, 90.0)),
        ("R14", at_rot(28.0, 28.3, 90.0)),
        // main button, centre
        ("SW1", at_rot(MOD_X, 33.8, 180.0)),
        // battery connector, right column; mouth faces up (y 38.1), the plug lies at y 30..38
        ("J2", at_rot(24.9, 42.5, 180.0)),
        // between button and USB: power test points, the divider's top resistor, USB ESD
        ("TP1", at(12.5, 40.0)),
        ("TP2", at(15.5, 40.0)),
        ("TP3", at(18.5, 40.0)),
        ("TP4", at(12.5, 43.0)),
        ("TP5", at(15.5, 43.0)),
        ("R13", at_rot(19.2, 45.6, 90.0)),
        ("U3", at_rot(MOD_X, 46.2, 90.0)),
        ("R1", at(11.5, 49.1)),
        ("R2", at(18.5, 49.1)),
        // left column: regulator
        ("U4", at(3.5, 32.5)),
        ("C4", at_rot(8.0, 32.0, 90.0)),
        ("C3", at(3.5, 35.6)),
        // power path
        ("Q1", at(3.0, 39.2)),
        ("D2", at(7.9, 38.0)),
        ("R6", at(7.9, 40.4)),
        // USB side parts, left column. x 5..10, y 42.5..47 is kept free for one more 0805 in
        // series with VBUS (the datasheet check's open item on the regulator's 6.5 V limit)
        ("R5", at(3.0, 42.0)),
        ("R16", at(2.2, 46.0)),
        ("R17", at(2.2, 48.2)),
        ("R4", at_rot(5.2, 50.5, 90.0)),
        ("D3", at_rot(7.6, 49.5, 90.0)),
        // bottom-left: reset (a pinhole in the case) and the charge light beside the USB mouth
        ("SW2", at(3.3, 57.0)),
        ("D1", at_rot(7.6, 56.5, 90.0)),
        // charger, bottom-right: beside the battery pocket, not over it (up to ~0.6 W in fast
        // charge), in the GND pours of both layers; its capacitors and PROG resistors above it
        ("C2", at(25.8, 49.75)),  // +BATT, next to J2's pads
        ("C1", at(22.2, 49.75)),  // VBUS
        ("R3", at(22.2, 51.85)),
        ("R19", at(25.8, 51.85)),
        ("U2", at_rot(22.4, 56.0, 90.0)),
        ("H2", at(27.0, 56.5)),
        // USB-C, bottom centre; receptacle body ends flush with the bottom edge
        ("J1", at(MOD_X, H - 3.7)),
    ]
    .into_iter()
    .map(|(r, p)| (r.to_string(), p))
    .collect();
    spec.texts = vec![
        // battery polarity: JST PH polarity is not standard. 
        // + is J2's pad 1 (verified against JST's and Adafruit's drawings by the datasheet check)
        silk("+", 25.9, 48.15, 1.0),
        silk("-", 23.9, 48.15, 1.0),
        silk("RST", 3.3, 54.6, 1.0),
    ];

    let waivers = vec![
        Waiver {
            kind: "silk_over_copper",
            substring: "Circle of TP",
            reason: "KiCad's test-point footprints all draw their silk ring 0.14 mm from the pad, 0.01 mm \
                     under the JLCPCB guideline; the fab trims silk off exposed copper. Kept unmodified so \
                     the footprint still matches the library (as on the starter).",
        },
        Waiver {
            kind: "silk_over_copper",
            substring: "Segment of D4 on F.Silkscreen",
            reason: "KiCad's LTST-C19HE1WT footprint draws its outline 0.14 mm from its own pads, 0.01 mm \
                     under the JLCPCB guideline; the fab trims silk off exposed copper. Kept unmodified so \
                     the footprint still matches the library.",
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

    Layout { rules, spec, route: RouteOptions::default(), waivers }
}
