//! Load symbols from KiCad `.kicad_sym` libraries, flattening `extends`.
//!
//! Coordinates returned here are in *library* space (mm, Y up), exactly as stored.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use anyhow::{Context, Result, anyhow, bail};

use crate::sexpr::{Sexp, parse};

pub fn symbol_dirs() -> Vec<PathBuf> {
    vec![
        PathBuf::from(std::env::var("KICAD10_SYMBOL_DIR").unwrap_or("/usr/share/kicad/symbols".into())),
        crate::repo_root().join("lib").join("symbols"),
    ]
}

#[derive(Clone, Debug)]
pub struct Pin {
    pub number: String,
    pub name: String,
    /// input, output, bidirectional, power_in, power_out, passive, no_connect, ...
    pub etype: String,
    /// Connection point, library space.
    pub x: f64,
    pub y: f64,
    /// Direction from the connection point into the body.
    pub angle: i32,
    pub hidden: bool,
    pub unit: i32,
}

#[derive(Debug)]
pub struct Symbol {
    pub lib_id: String,
    /// Flattened, renamed "Lib:Name", ready to embed in lib_symbols.
    pub node: Sexp,
    pub pins: Vec<Pin>,
    /// xmin, ymin, xmax, ymax (library space, body + pins).
    pub bbox: (f64, f64, f64, f64),
    pub is_power: bool,
}

impl Symbol {
    pub fn prop(&self, key: &str) -> Option<&str> {
        self.node.find_all("property").find(|p| p.arg(1) == Some(key)).and_then(|p| p.arg(2))
    }
}

type Library = HashMap<String, Sexp>;

fn library(lib: &str) -> Result<Arc<Library>> {
    static LIBS: OnceLock<Mutex<HashMap<String, Arc<Library>>>> = OnceLock::new();
    let mut libs = LIBS.get_or_init(Default::default).lock().unwrap();
    if let Some(l) = libs.get(lib) {
        return Ok(l.clone());
    }
    for d in symbol_dirs() {
        let path = d.join(format!("{lib}.kicad_sym"));
        if path.exists() {
            let root = parse(&std::fs::read_to_string(&path)?).with_context(|| path.display().to_string())?;
            let map: Library = root
                .find_all("symbol")
                .map(|s| (s.arg(1).unwrap_or_default().to_string(), s.clone()))
                .collect();
            let map = Arc::new(map);
            libs.insert(lib.to_string(), map.clone());
            return Ok(map);
        }
    }
    bail!("symbol library {lib:?} not found in {:?}", symbol_dirs())
}

fn flatten(lib: &str, name: &str) -> Result<Sexp> {
    let libmap = library(lib)?;
    let raw = libmap.get(name).ok_or_else(|| anyhow!("symbol {lib}:{name} not found"))?;
    let Some(ext) = raw.find("extends") else {
        return Ok(raw.clone());
    };
    let parent = flatten(lib, ext.arg(1).unwrap_or_default())?;
    let pname = parent.arg(1).unwrap_or_default().to_string();
    let mut out = vec![Sexp::Sym("symbol".into()), Sexp::Str(name.into())];
    for c in &parent.items()[2..] {
        let Sexp::List(_) = c else { continue };
        if c.is("property") {
            continue;
        }
        if c.is("symbol") {
            let mut sub = c.clone();
            let subname = sub.arg(1).unwrap_or_default().to_string();
            assert!(subname.starts_with(&format!("{pname}_")), "{subname}");
            sub.items_mut()[1] = Sexp::Str(format!("{name}{}", &subname[pname.len()..]));
            out.push(sub);
        } else {
            out.push(c.clone());
        }
    }
    // child's own flags and properties win; insert them before the sub-symbols
    let child_items: Vec<Sexp> = raw.items()[2..]
        .iter()
        .filter(|c| matches!(c, Sexp::List(_)) && !c.is("extends"))
        .cloned()
        .collect();
    let child_keys: Vec<&str> = child_items.iter().filter(|c| !c.is("property")).filter_map(|c| c.head()).collect();
    out.retain(|c| !matches!(c, Sexp::List(_)) || !c.head().is_some_and(|h| child_keys.contains(&h)));
    let first_sub = out.iter().position(|c| c.is("symbol")).unwrap_or(out.len());
    let tail = out.split_off(first_sub);
    out.extend(child_items);
    out.extend(tail);
    Ok(Sexp::List(out))
}

fn unit_style(subname: &str) -> (i32, i32) {
    let mut parts = subname.rsplitn(3, '_');
    let style = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let unit = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    (unit, style)
}

fn xy(n: Option<&Sexp>) -> (f64, f64) {
    n.map(|n| (n.num(1), n.num(2))).unwrap_or_default()
}

pub fn load(lib_id: &str) -> Result<Arc<Symbol>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Arc<Symbol>>>> = OnceLock::new();
    if let Some(s) = CACHE.get_or_init(Default::default).lock().unwrap().get(lib_id) {
        return Ok(s.clone());
    }
    let (lib, name) = lib_id.split_once(':').ok_or_else(|| anyhow!("lib id {lib_id:?} has no ':'"))?;
    let mut node = flatten(lib, name)?;
    node.items_mut()[1] = Sexp::Str(lib_id.to_string());
    let mut pins = Vec::new();
    let (mut xs, mut ys) = (Vec::new(), Vec::new());
    for sub in node.find_all("symbol") {
        let (unit, style) = unit_style(sub.arg(1).unwrap_or_default());
        if style != 0 && style != 1 {
            continue; // De Morgan alternates
        }
        for g in &sub.items()[2..] {
            match g.head() {
                Some("pin") => {
                    let at = g.find("at").unwrap();
                    let (x, y, a) = (at.num(1), at.num(2), at.num(3) as i32);
                    let length = g.find("length").map_or(0.0, |l| l.num(1));
                    let hidden = g.items().iter().any(|c| matches!(c, Sexp::Sym(s) if s == "hide"))
                        || g.find("hide").is_some_and(|h| h.arg(1) == Some("yes"));
                    pins.push(Pin {
                        number: g.find("number").and_then(|n| n.arg(1)).unwrap_or_default().into(),
                        name: g.find("name").and_then(|n| n.arg(1)).unwrap_or_default().into(),
                        etype: g.arg(1).unwrap_or_default().into(),
                        x,
                        y,
                        angle: a,
                        hidden,
                        unit,
                    });
                    xs.push(x);
                    ys.push(y);
                    let (dx, dy) = match a {
                        0 => (1.0, 0.0),
                        90 => (0.0, 1.0),
                        180 => (-1.0, 0.0),
                        270 => (0.0, -1.0),
                        _ => bail!("{lib_id}: pin angle {a}"),
                    };
                    xs.push(x + dx * length);
                    ys.push(y + dy * length);
                }
                Some("rectangle") => {
                    for k in ["start", "end"] {
                        let (x, y) = xy(g.find(k));
                        xs.push(x);
                        ys.push(y);
                    }
                }
                Some("polyline" | "bezier") => {
                    for p in g.find("pts").into_iter().flat_map(|p| p.find_all("xy")) {
                        xs.push(p.num(1));
                        ys.push(p.num(2));
                    }
                }
                Some("circle") => {
                    let (cx, cy) = xy(g.find("center"));
                    let r = g.find("radius").map_or(0.0, |r| r.num(1));
                    xs.extend([cx - r, cx + r]);
                    ys.extend([cy - r, cy + r]);
                }
                Some("arc") => {
                    for k in ["start", "mid", "end"] {
                        let (x, y) = xy(g.find(k));
                        xs.push(x);
                        ys.push(y);
                    }
                }
                _ => {}
            }
        }
    }
    let bbox = if xs.is_empty() {
        (-1.27, -1.27, 1.27, 1.27)
    } else {
        let min = |v: &[f64]| v.iter().copied().fold(f64::INFINITY, f64::min);
        let max = |v: &[f64]| v.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        (min(&xs), min(&ys), max(&xs), max(&ys))
    };
    let is_power = node.find("power").is_some();
    let mut units: Vec<i32> = pins.iter().map(|p| p.unit).filter(|&u| u != 0).collect();
    units.sort();
    units.dedup();
    if units.len() > 1 {
        bail!("{lib_id}: multi-unit symbols not supported yet ({units:?})");
    }
    let sym = Arc::new(Symbol { lib_id: lib_id.to_string(), node, pins, bbox, is_power });
    CACHE.get().unwrap().lock().unwrap().insert(lib_id.to_string(), sym.clone());
    Ok(sym)
}
