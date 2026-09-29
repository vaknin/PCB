//! What a board's `layout.rs` provides: net classes, the board shape and placement,
//! routing options, and DRC/ERC waivers.
//!
//! Coordinates are mm from the board's top-left corner, Y pointing down; rotations are
//! degrees CCW as seen from the top. Each `Place` is the footprint's own origin.
//!
//! A bottom-side part is the footprint flipped left-right (as pcbnew's flip of a
//! footprint at angle 0) and then turned `rot`: `rot` is the angle KiCad shows and saves
//! for it, still CCW as seen from the top.

use crate::project::BoardRules;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Top,
    Bottom,
}

#[derive(Clone, Copy, Debug)]
pub struct Place {
    pub x: f64,
    pub y: f64,
    pub rot: f64,
    pub side: Side,
}

/// A top-side placement at (x, y), unrotated.
pub fn at(x: f64, y: f64) -> Place {
    Place { x, y, rot: 0.0, side: Side::Top }
}

/// A top-side placement at (x, y), rotated `rot` degrees CCW.
pub fn at_rot(x: f64, y: f64, rot: f64) -> Place {
    Place { x, y, rot, side: Side::Top }
}

/// A bottom-side placement at (x, y), rotated `rot` degrees CCW as seen from the top (the
/// angle KiCad shows for the flipped footprint).
pub fn at_bottom(x: f64, y: f64, rot: f64) -> Place {
    Place { x, y, rot, side: Side::Bottom }
}

/// Rule area: no copper of any kind (e.g. antenna clearance).
#[derive(Clone, Debug)]
pub struct Keepout {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
    pub name: String,
}

/// A local copper pour of one net (e.g. cooling copper under a regulator tab).
///
/// Filled above the GND pours (higher priority). The router never sees it (the route
/// stage hides every pour), so tracks may cross the area and the fill flows around
/// them; vias tying its layers together come from `RouteOptions::stitch_local`.
#[derive(Clone, Debug)]
pub struct CopperZone {
    pub net: String,
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
    pub layers: Vec<String>,
    /// The GND pours use 0 and 1.
    pub priority: u32,
}

impl CopperZone {
    /// Both outer layers, priority 10.
    pub fn new(net: &str, x0: f64, y0: f64, x1: f64, y1: f64) -> Self {
        CopperZone { net: net.into(), x0, y0, x1, y1, layers: vec!["F.Cu".into(), "B.Cu".into()], priority: 10 }
    }
}

#[derive(Clone, Debug)]
pub struct Text {
    pub text: String,
    pub x: f64,
    pub y: f64,
    pub size: f64,
    pub layer: String,
    pub rot: f64,
}

/// Front silkscreen text of height `size` mm.
pub fn silk(text: &str, x: f64, y: f64, size: f64) -> Text {
    Text { text: text.into(), x, y, size, layer: "F.SilkS".into(), rot: 0.0 }
}

#[derive(Clone, Debug)]
pub struct BoardSpec {
    pub width: f64,
    pub height: f64,
    pub corner_radius: f64,
    /// Reference → placement, for every part with a footprint.
    pub places: Vec<(String, Place)>,
    pub keepouts: Vec<Keepout>,
    pub zones: Vec<CopperZone>,
    pub texts: Vec<Text>,
    pub gnd_net: String,
    pub gnd_layers: Vec<String>,
    /// Reference designators go to the fab (assembly) layer, except these; owner-facing
    /// labels are given as `texts` instead, so the silkscreen stays readable.
    pub silk_refs: Vec<String>,
}

impl BoardSpec {
    pub fn new(width: f64, height: f64) -> Self {
        BoardSpec {
            width,
            height,
            corner_radius: 1.0,
            places: vec![],
            keepouts: vec![],
            zones: vec![],
            texts: vec![],
            gnd_net: "GND".into(),
            gnd_layers: vec!["F.Cu".into(), "B.Cu".into()],
            silk_refs: vec![],
        }
    }

    pub fn place(&self, reference: &str) -> Option<Place> {
        self.places.iter().find(|(r, _)| r == reference).map(|(_, p)| *p)
    }
}

#[derive(Clone, Debug)]
pub struct RouteOptions {
    pub max_passes: u32,
    /// KiCad's DSN export omits board-edge clearance; Freerouting gets it as an option.
    pub edge_clearance_um: u32,
    pub via_costs: u32,
    pub timeout_s: u64,
    /// Footprint orders to route (Freerouting is deterministic, and the order of the
    /// footprints in the DSN steers it). Every order is routed and the best result kept:
    /// fewest unrouted, then least track thinner than its net class, then fewest vias,
    /// then shortest. Order 0 is the board's own order.
    pub tries: u32,
    /// Freerouting runs at a time. On the 8-core laptop 4 at a time take ~54 s each
    /// against ~21 s alone (memory-bound; its thread settings change nothing): ~1.5× the
    /// throughput of running them one by one.
    pub parallel: usize,
    /// Freerouting's fanout stage (escape tracks and vias from SMD pads before routing).
    /// Off: fanout narrows tracks to 3/4 or 3/5 of their net-class width (read from
    /// Freerouting 2.4.1's code) and added vias; without it the starter board routes
    /// with no track under its class width and fewer vias (D-017).
    pub fanout: bool,
    /// The best this many orders (by the router's numbers) are finished (pours, stitching)
    /// and DRC-checked; the best-ranked one with no open DRC item is kept, else the one
    /// with the fewest. 0 keeps the router's best unchecked. Each check takes ~15 s
    /// (they run at once).
    pub drc_checks: usize,
    /// Add vias tying this net's pours together after routing.
    pub stitch_net: Option<String>,
    /// mm grid for stitching vias.
    pub stitch_pitch: f64,
    /// Diameter, drill (mm).
    pub stitch_via: (f64, f64),
    /// mm from a stitching via to other-net tracks.
    pub stitch_clearance: f64,
    /// Local pours of other nets (`CopperZone`, e.g. regulator cooling copper): net → via
    /// pitch (mm). Unlike the main pour net, their unrouted connections still count.
    pub stitch_local: Vec<(String, f64)>,
}

impl Default for RouteOptions {
    fn default() -> Self {
        RouteOptions {
            max_passes: 100,
            edge_clearance_um: 500,
            via_costs: 50,
            timeout_s: 900,
            tries: 8,
            parallel: 4,
            fanout: false,
            drc_checks: 3,
            stitch_net: Some("GND".into()),
            stitch_pitch: 3.0,
            stitch_via: (0.6, 0.3),
            stitch_clearance: 0.25,
            stitch_local: vec![],
        }
    }
}

/// An accepted DRC/ERC warning: violation type, text that must appear in it, and why.
/// Errors can't be waived.
#[derive(Clone, Copy, Debug)]
pub struct Waiver {
    pub kind: &'static str,
    pub substring: &'static str,
    pub reason: &'static str,
}

pub struct Layout {
    pub rules: BoardRules,
    pub spec: BoardSpec,
    pub route: RouteOptions,
    pub waivers: Vec<Waiver>,
}
