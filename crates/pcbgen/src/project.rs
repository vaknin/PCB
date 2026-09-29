//! Write the KiCad project file (.kicad_pro) and fab design rules (.kicad_dru).
//!
//! Our own board rules are deliberately looser than the fab's minimums, so a DRC
//! pass against the fab rules (the .kicad_dru) has margin to spare.

use std::path::{Path, PathBuf};

use anyhow::Result;
use serde_json::{Value, json};

#[derive(Clone, Debug)]
pub struct NetClass {
    pub name: String,
    /// mm
    pub track: f64,
    pub clearance: f64,
    pub via_diameter: f64,
    pub via_drill: f64,
    pub patterns: Vec<String>,
}

impl NetClass {
    pub fn new(name: &str, track: f64, clearance: f64, via_diameter: f64, via_drill: f64) -> Self {
        NetClass { name: name.into(), track, clearance, via_diameter, via_drill, patterns: vec![] }
    }

    pub fn patterns(mut self, p: &[&str]) -> Self {
        self.patterns = p.iter().map(|s| s.to_string()).collect();
        self
    }
}

#[derive(Clone, Debug)]
pub struct BoardRules {
    pub fab_rules: String,
    pub min_track: f64,
    pub min_clearance: f64,
    pub min_via_diameter: f64,
    pub min_via_drill: f64,
    pub min_hole: f64,
    pub edge_clearance: f64,
    /// The first class must be "Default".
    pub classes: Vec<NetClass>,
}

impl Default for BoardRules {
    fn default() -> Self {
        BoardRules {
            fab_rules: "JLCPCB-2L-1oz.kicad_dru".into(),
            min_track: 0.15,
            min_clearance: 0.15,
            min_via_diameter: 0.5,
            min_via_drill: 0.3,
            min_hole: 0.3,
            edge_clearance: 0.5,
            classes: vec![NetClass::new("Default", 0.2, 0.2, 0.6, 0.3)],
        }
    }
}

impl BoardRules {
    /// The class KiCad gives `net`: the first whose pattern matches, else Default.
    pub fn class_of(&self, net: &str) -> &NetClass {
        self.classes
            .iter()
            .find(|nc| nc.patterns.iter().any(|p| glob(p, net)))
            .unwrap_or_else(|| self.classes.iter().find(|nc| nc.name == "Default").unwrap())
    }
}

/// fnmatch-style match (`*`, `?`, `[...]`), case-sensitive, as KiCad's net-class patterns.
pub fn glob(pat: &str, s: &str) -> bool {
    fn go(p: &[char], s: &[char]) -> bool {
        match p.first() {
            None => s.is_empty(),
            Some('*') => (0..=s.len()).any(|i| go(&p[1..], &s[i..])),
            Some('?') => !s.is_empty() && go(&p[1..], &s[1..]),
            Some('[') => {
                let Some(end) = p.iter().skip(2).position(|&c| c == ']').map(|i| i + 2) else {
                    return s.first() == Some(&'[') && go(&p[1..], &s[1..]);
                };
                let Some(&c) = s.first() else { return false };
                let set = &p[1..end];
                let (neg, set) = if set.first() == Some(&'!') { (true, &set[1..]) } else { (false, set) };
                let mut hit = false;
                let mut i = 0;
                while i < set.len() {
                    if i + 2 < set.len() && set[i + 1] == '-' {
                        hit |= set[i] <= c && c <= set[i + 2];
                        i += 3;
                    } else {
                        hit |= set[i] == c;
                        i += 1;
                    }
                }
                hit != neg && go(&p[end + 1..], &s[1..])
            }
            Some(&c) => s.first() == Some(&c) && go(&p[1..], &s[1..]),
        }
    }
    let p: Vec<char> = pat.chars().collect();
    let s: Vec<char> = s.chars().collect();
    go(&p, &s)
}

fn class_json(nc: &NetClass, priority: i64) -> Value {
    json!({
        "name": nc.name, "priority": priority,
        "clearance": nc.clearance, "track_width": nc.track,
        "via_diameter": nc.via_diameter, "via_drill": nc.via_drill,
        "microvia_diameter": 0.3, "microvia_drill": 0.1,
        "diff_pair_width": 0.2, "diff_pair_gap": 0.25, "diff_pair_via_gap": 0.25,
        "bus_width": 12, "wire_width": 6, "line_style": 0,
        "pcb_color": "rgba(0, 0, 0, 0.000)", "schematic_color": "rgba(0, 0, 0, 0.000)",
    })
}

fn sorted_dedup(mut v: Vec<f64>) -> Vec<f64> {
    v.sort_by(f64::total_cmp);
    v.dedup();
    v
}

pub fn write(out_dir: &Path, name: &str, rules: &BoardRules) -> Result<PathBuf> {
    std::fs::create_dir_all(out_dir)?;
    let classes = &rules.classes;
    assert_eq!(classes[0].name, "Default");
    let tracks = sorted_dedup(classes.iter().map(|c| c.track).collect());
    let mut vias: Vec<(f64, f64)> = classes.iter().map(|c| (c.via_diameter, c.via_drill)).collect();
    vias.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    vias.dedup();
    let mut track_widths = vec![json!(0.0)];
    track_widths.extend(tracks.iter().map(|t| json!(t)));
    let mut via_dims = vec![json!({"diameter": 0.0, "drill": 0.0})];
    via_dims.extend(vias.iter().map(|(d, h)| json!({"diameter": d, "drill": h})));
    let pro = json!({
        "meta": {"filename": format!("{name}.kicad_pro"), "version": 3},
        "board": {
            "design_settings": {
                "rules": {
                    "min_track_width": rules.min_track,
                    "min_clearance": rules.min_clearance,
                    "min_via_diameter": rules.min_via_diameter,
                    "min_through_hole_diameter": rules.min_hole,
                    "min_copper_edge_clearance": rules.edge_clearance,
                    "min_hole_clearance": 0.25,
                    "min_hole_to_hole": 0.5,
                    "min_via_annular_width": 0.1,
                    "min_silk_clearance": 0.0,
                    "min_text_height": 0.8,
                    "min_text_thickness": 0.15,
                    "solder_mask_to_copper_clearance": 0.0,
                    "max_error": 0.005,
                    "min_resolved_spokes": 2,
                    "use_height_for_length_calcs": true,
                },
                // first entry 0 = "use net class"; KiCad expects that slot
                "track_widths": track_widths,
                "via_dimensions": via_dims,
                "diff_pair_dimensions": [{"gap": 0.0, "via_gap": 0.0, "width": 0.0}],
                "meta": {"version": 2},
            },
        },
        "net_settings": {
            "classes": classes.iter().enumerate()
                .map(|(i, c)| class_json(c, if c.name == "Default" { 2147483647 } else { i as i64 }))
                .collect::<Vec<_>>(),
            "meta": {"version": 4},
            "net_colors": null,
            "netclass_assignments": null,
            "netclass_patterns": classes.iter()
                .flat_map(|c| c.patterns.iter().map(move |p| json!({"netclass": c.name, "pattern": p})))
                .collect::<Vec<_>>(),
        },
        "schematic": {"meta": {"version": 1}},
        "erc": {"meta": {"version": 0}},
        "pcbnew": {"page_layout_descr_file": ""},
        "libraries": {"pinned_footprint_libs": [], "pinned_symbol_libs": []},
        "sheets": [],
        "text_variables": {},
    });
    let path = out_dir.join(format!("{name}.kicad_pro"));
    std::fs::write(&path, serde_json::to_string_pretty(&pro)? + "\n")?;
    std::fs::copy(crate::repo_root().join("rules").join(&rules.fab_rules), out_dir.join(format!("{name}.kicad_dru")))?;
    Ok(path)
}

/// Net classes as KiCad will apply them: read back from the project file next to the board
/// (kicad-cli rewrites it, so this is what DRC sees, not what we meant to write).
pub fn read_classes(pro: &Path) -> Result<BoardRules> {
    let v: Value = serde_json::from_str(&std::fs::read_to_string(pro)?)?;
    let ns = &v["net_settings"];
    let num = |c: &Value, k: &str| c[k].as_f64().unwrap_or(0.0);
    let mut classes: Vec<NetClass> = ns["classes"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|c| NetClass::new(c["name"].as_str().unwrap_or(""), num(c, "track_width"), num(c, "clearance"), num(c, "via_diameter"), num(c, "via_drill")))
        .collect();
    for p in ns["netclass_patterns"].as_array().into_iter().flatten() {
        let (Some(cls), Some(pat)) = (p["netclass"].as_str(), p["pattern"].as_str()) else { continue };
        if let Some(c) = classes.iter_mut().find(|c| c.name == cls) {
            c.patterns.push(pat.to_string());
        }
    }
    // KiCad's own order: by priority, Default last
    let prio = |name: &str| {
        ns["classes"].as_array().into_iter().flatten().find(|c| c["name"] == name).and_then(|c| c["priority"].as_i64()).unwrap_or(i64::MAX)
    };
    classes.sort_by_key(|c| prio(&c.name));
    Ok(BoardRules { classes, ..Default::default() })
}

#[cfg(test)]
mod tests {
    use super::glob;

    #[test]
    fn patterns() {
        assert!(glob("*USB_D*", "/USB_D+"));
        assert!(!glob("USB_D+", "/USB_D+"));
        assert!(glob("+3V3", "+3V3"));
        assert!(glob("I2C_S[CD][AL]", "I2C_SDA"));
    }
}
