//! Read a Freerouting session (SES) back into board tracks and vias.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result, anyhow, bail};

use crate::geom::{Pt, pt};
use crate::sexpr::{Sexp, parse};

pub struct Track {
    pub start: Pt,
    pub end: Pt,
    pub width: f64,
    pub layer: String,
    pub net: String,
}

pub struct RoutedVia {
    pub at: Pt,
    pub size: f64,
    pub drill: f64,
    pub net: String,
}

/// Tracks and vias from the session's `network_out`, in board mm (Y down). Via sizes come
/// from `vias` (padstack name → diameter, drill), as written into the DSN.
pub fn read(path: &Path, vias: &HashMap<String, (f64, f64)>) -> Result<(Vec<Track>, Vec<RoutedVia>)> {
    let text = std::fs::read_to_string(path).with_context(|| path.display().to_string())?;
    let root = parse(&text)?;
    let routes = root.find("routes").ok_or_else(|| anyhow!("{}: no routes", path.display()))?;
    // (resolution um 10): 10 units per µm
    let res = routes.find("resolution").ok_or_else(|| anyhow!("SES has no resolution"))?;
    let per_mm = match res.arg(1) {
        Some("um") => 1000.0,
        Some("mm") => 1.0,
        Some("mil") => 1.0 / 0.0254,
        Some("inch") => 1.0 / 25.4,
        u => bail!("SES resolution unit {u:?}"),
    } * res.num(2);
    let at = |x: f64, y: f64| pt(x / per_mm, -y / per_mm);
    let mut tracks = vec![];
    let mut out_vias = vec![];
    for net in routes.find("network_out").map(|n| n.find_all("net").collect::<Vec<_>>()).unwrap_or_default() {
        let name = net.arg(1).unwrap_or("").to_string();
        for w in net.find_all("wire") {
            let Some(p) = w.find("path") else { continue };
            let layer = p.arg(1).unwrap_or("").to_string();
            let width = p.num(2) / per_mm;
            let nums: Vec<f64> = p.items()[3..].iter().filter_map(Sexp::atom).filter_map(|s| s.parse().ok()).collect();
            let pts: Vec<Pt> = nums.as_chunks::<2>().0.iter().map(|&[x, y]| at(x, y)).collect();
            for s in pts.windows(2) {
                tracks.push(Track { start: s[0], end: s[1], width, layer: layer.clone(), net: name.clone() });
            }
        }
        for v in net.find_all("via") {
            let ps = v.arg(1).unwrap_or("");
            let &(size, drill) = vias.get(ps).ok_or_else(|| anyhow!("SES via padstack {ps:?} is not one the DSN defined"))?;
            out_vias.push(RoutedVia { at: at(v.num(2), v.num(3)), size, drill, net: name.clone() });
        }
    }
    Ok((tracks, out_vias))
}
