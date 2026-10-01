//! `pcb parts search "<what>" [--basic|--no-fee] [--qty N] [--pages N] [--limit N] [--any-stock] [--fresh]`
//! `pcb parts show <LCSC> [--datasheet] [--fresh]`
//!
//! Live JLCPCB listings (through the 1-day cache in `~/.cache/pcbgen/jlc/`), so LCSC codes
//! are copied from a listing instead of typed. The KiCad footprint column is a name match
//! on the package or MPN: a hint where to look, never a verified land pattern.

use std::path::PathBuf;

use anyhow::{Result, bail};
use pcbgen::jlc::{Filter, Jlc, Listing};

const HELP: &str = "usage:
  pcb parts search \"<what>\" [--basic|--no-fee] [--qty N] [--pages N] [--limit N] [--any-stock] [--fresh]
      --basic      Basic parts only            --no-fee   Basic + Preferred Extended (no $3.07 fee)
      --qty N      price at N parts (10)        --pages N  pages of 100 per class asked (1)
      --limit N    rows printed (40)            --any-stock  include parts JLCPCB has none of
  pcb parts show <LCSC> [--datasheet] [--fresh]
      --datasheet  download the PDF into the cache and run pdftotext -layout beside it
  --fresh skips the 1-day answer cache";

pub fn main(args: &[String]) -> Result<bool> {
    match args.first().map(String::as_str) {
        Some("search") => search(&args[1..]),
        Some("show") => show(&args[1..]),
        Some("-h" | "--help") => {
            println!("{HELP}");
            Ok(true)
        }
        Some(s) => bail!("unknown parts command {s:?}\n{HELP}"),
        None => bail!("{HELP}"),
    }
}

fn number(it: &mut std::slice::Iter<String>, flag: &str) -> Result<u64> {
    match it.next().and_then(|n| n.parse().ok()).filter(|&n| n > 0) {
        Some(n) => Ok(n),
        None => bail!("{flag} needs a number of at least 1"),
    }
}

fn search(args: &[String]) -> Result<bool> {
    let (mut f, mut jlc) = (Filter { in_stock: true, ..Default::default() }, Jlc::default());
    let (mut qty, mut pages, mut limit) = (10, 1, 40);
    let mut words: Vec<&str> = vec![];
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--basic" => f.basic_only = true,
            "--no-fee" => f.no_fee = true,
            "--any-stock" => f.in_stock = false,
            "--fresh" => jlc.fresh = true,
            "--qty" => qty = number(&mut it, "--qty")?,
            "--pages" => pages = number(&mut it, "--pages")?,
            "--limit" => limit = number(&mut it, "--limit")? as usize,
            "-h" | "--help" => {
                println!("{HELP}");
                return Ok(true);
            }
            s if s.starts_with("--") => bail!("unknown option {s}\n{HELP}"),
            s => words.push(s),
        }
    }
    if words.is_empty() {
        bail!("what to search for?\n{HELP}");
    }
    let keyword = words.join(" ");
    let mut found = jlc.search(&keyword, &f, pages)?;
    sort(&mut found);
    let fps = Footprints::load();
    let kind = if f.basic_only {
        "Basic"
    } else if f.no_fee {
        "Basic + Preferred"
    } else {
        "all classes"
    };
    println!(
        "JLCPCB parts for \"{keyword}\" ({kind}{}): {} found, fee-free first, then by stock; price at qty {qty}",
        if f.in_stock { ", in stock" } else { "" },
        found.len()
    );
    let head = ["LCSC", "Class", "Stock", &format!("$@{qty}"), "Package", "MPN", "KiCad footprint (hint)"].map(String::from);
    let rows: Vec<[String; 7]> = found
        .iter()
        .take(limit)
        .map(|l| {
            [
                l.lcsc.clone(),
                l.class(),
                l.stock.to_string(),
                format!("{:.4}", l.unit(qty)),
                cut(&l.package, 22),
                cut(&l.mpn, 26),
                fps.hint(l).unwrap_or_else(|| "-".into()),
            ]
        })
        .collect();
    print_table(&head, &rows);
    if found.len() > limit {
        println!("({} more: --limit {})", found.len() - limit, found.len());
    }
    println!("Footprint column: a name match in the KiCad libraries, not checked against the datasheet.");
    Ok(true)
}

/// Fee-free (Basic, Preferred) first, then the most stock.
fn sort(v: &mut [Listing]) {
    v.sort_by(|a, b| b.fee_free().cmp(&a.fee_free()).then(b.stock.cmp(&a.stock)).then(a.lcsc.cmp(&b.lcsc)));
}

fn cut(s: &str, n: usize) -> String {
    if s.chars().count() <= n { s.into() } else { s.chars().take(n - 1).collect::<String>() + "…" }
}

fn print_table(head: &[String; 7], rows: &[[String; 7]]) {
    let mut w = head.each_ref().map(|h| h.chars().count());
    for r in rows {
        for (i, c) in r.iter().enumerate() {
            w[i] = w[i].max(c.chars().count());
        }
    }
    let line = |r: &[String; 7]| {
        let cells: Vec<String> = r
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let pad = w[i] - c.chars().count();
                // numbers right-aligned
                if i == 2 || i == 3 { format!("{}{c}", " ".repeat(pad)) } else { format!("{c}{}", " ".repeat(pad)) }
            })
            .collect();
        println!("{}", cells.join("  ").trim_end());
    };
    line(head);
    line(&w.map(|n| "-".repeat(n)));
    for r in rows {
        line(r);
    }
}

fn show(args: &[String]) -> Result<bool> {
    let (mut jlc, mut datasheet, mut code) = (Jlc::default(), false, None);
    for a in args {
        match a.as_str() {
            "--datasheet" => datasheet = true,
            "--fresh" => jlc.fresh = true,
            "-h" | "--help" => {
                println!("{HELP}");
                return Ok(true);
            }
            s if s.starts_with("--") => bail!("unknown option {s}\n{HELP}"),
            s => code = Some(s.trim().to_uppercase()),
        }
    }
    let Some(code) = code else { bail!("which LCSC code?\n{HELP}") };
    if !code.starts_with('C') || !code[1..].chars().all(|c| c.is_ascii_digit()) || code.len() < 2 {
        bail!("{code:?} is not an LCSC code (C followed by digits)");
    }
    let l = jlc.get(&code)?;
    let fee = if l.fee_free() { "no loading fee" } else { "loading fee per unique part" };
    let rows: Vec<(&str, String)> = vec![
        ("LCSC", l.lcsc.clone()),
        ("MPN", l.mpn.clone()),
        ("Manufacturer", l.manufacturer.clone()),
        ("Class", format!("{} ({fee})", l.class())),
        ("Category", l.category.clone()),
        ("Package", l.package.clone()),
        ("Description", l.description.clone()),
        ("Stock", format!("{} (JLCPCB assembly stock)", l.stock)),
        ("Minimum", format!("{} parts per order; attrition {} (extra parts JLCPCB buys)", l.minimum, l.attrition)),
        ("Datasheet", l.datasheet.clone()),
        ("JLCPCB copy", l.datasheet_file.clone().unwrap_or_else(|| "-".into())),
        ("Part page", l.page.clone()),
        ("KiCad (hint)", Footprints::load().hint(&l).unwrap_or_else(|| "no name match".into())),
    ];
    for (k, v) in rows {
        println!("{k:<13} {v}");
    }
    println!("Prices (USD each):");
    for (from, to, usd) in &l.tiers {
        let range = match to {
            Some(t) => format!("{from}-{t}"),
            None => format!("{from}+"),
        };
        println!("  {range:>12}  {usd:.4}");
    }
    if !l.attributes.is_empty() {
        println!("Parameters:");
        for (k, v) in &l.attributes {
            println!("  {k}: {v}");
        }
    }
    if datasheet {
        let (pdf, txt) = jlc.datasheet(&l)?;
        println!("Datasheet PDF  {}", pdf.display());
        println!("Datasheet text {}", txt.display());
    }
    Ok(true)
}

/// Footprint names in KiCad's libraries and the repo's own (`lib/footprints`).
struct Footprints {
    /// (library, name, '_'-separated name tokens upper-cased, alphanumerics of the name).
    all: Vec<(String, String, Vec<String>, String)>,
}

fn alnum(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_uppercase()
}

impl Footprints {
    fn load() -> Self {
        let dirs = [PathBuf::from(pcbgen::footprint::kicad_dir()), pcbgen::repo_root().join("lib/footprints")];
        let mut names = vec![];
        for dir in dirs {
            for lib in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
                let p = lib.path();
                let Some(libname) = p.file_name().and_then(|n| n.to_str()).and_then(|n| n.strip_suffix(".pretty")) else { continue };
                for fp in std::fs::read_dir(&p).into_iter().flatten().flatten() {
                    if let Some(n) = fp.file_name().to_str().and_then(|n| n.strip_suffix(".kicad_mod")) {
                        names.push((libname.to_string(), n.to_string()));
                    }
                }
            }
        }
        Self::new(names)
    }

    fn new(mut names: Vec<(String, String)>) -> Self {
        names.sort();
        let all = names
            .into_iter()
            .map(|(lib, n)| {
                let tokens = n.split('_').map(|t| t.to_uppercase()).collect();
                let a = alnum(&n);
                (lib, n, tokens, a)
            })
            .collect();
        Footprints { all }
    }

    /// "Lib:Name" of the best name match, with how many others matched too.
    fn hint(&self, l: &Listing) -> Option<String> {
        let found = self.by_mpn(&l.mpn);
        let mut found = if found.is_empty() { self.by_package(&l.package) } else { found };
        // libraries that fit the part's kind first ("SMA" is a diode package and a coax
        // connector); hand-soldering variants and longer names after
        let what = format!("{} {}", l.category, l.description).to_lowercase();
        let score = |name: &str| relevance(name.split(':').next().unwrap_or(""), &what);
        let best = found.iter().map(|n| score(n)).max().unwrap_or(0);
        found.retain(|n| score(n) == best);
        found.sort_by_key(|n| (n.contains("Handsoldering"), n.len(), n.clone()));
        let first = found.first()?;
        Some(match found.len() {
            1 => first.clone(),
            n => format!("{first} (+{})", n - 1),
        })
    }

    /// Footprints named after the part: its MPN, or the MPN with ordering-code tails cut
    /// ("ESP32-S3-WROOM-1-N16R8" → "ESP32-S3-WROOM-1"), at least 6 characters.
    fn by_mpn(&self, mpn: &str) -> Vec<String> {
        let mut stem = mpn.split('(').next().unwrap_or("").trim().to_string();
        loop {
            let key = alnum(&stem);
            if key.len() < 6 {
                return vec![];
            }
            let hits: Vec<String> = self.all.iter().filter(|f| f.3.contains(&key)).map(|f| format!("{}:{}", f.0, f.1)).collect();
            if !hits.is_empty() {
                return hits;
            }
            match stem.rfind(['-', '/']) {
                Some(i) => stem.truncate(i),
                None => return vec![],
            }
        }
    }

    /// Footprints with a name token equal to the package ("SOD-123" in "D_SOD-123"), or the
    /// package plus a pin count ("SOT-223" in "SOT-223-3"). A bare size code ("0805")
    /// matches the chip footprints of that size.
    fn by_package(&self, package: &str) -> Vec<String> {
        let exact = |cand: &str, t: &str| t == cand;
        let pins = |cand: &str, t: &str| t.strip_prefix(cand).and_then(|r| r.strip_prefix('-')).is_some_and(|r| !r.is_empty() && r.chars().all(|c| c.is_ascii_digit()));
        let names = package_names(package);
        // exact names first: a SOT-23 is not a SOT-23-6
        for rule in [&exact as &dyn Fn(&str, &str) -> bool, &pins] {
            for cand in &names {
                let found: Vec<String> = self.all.iter().filter(|f| f.2.iter().any(|t| rule(cand, t))).map(|f| format!("{}:{}", f.0, f.1)).collect();
                if !found.is_empty() {
                    return found;
                }
            }
        }
        vec![]
    }
}

/// How well a KiCad library fits a part, from words in the part's category and
/// description: 2 = the library's kind is named, 1 = generic IC packages, 0 = neither.
fn relevance(lib: &str, what: &str) -> u8 {
    let kind = lib.split('_').next().unwrap_or(lib);
    let words: &[&str] = match kind {
        "Diode" => &["diode", "rectifier", "tvs", "zener", "schottky", "esd"],
        "LED" => &["led", "light emitting"],
        "Resistor" => &["resistor"],
        "Capacitor" => &["capacitor"],
        "Inductor" => &["inductor", "choke"],
        "Fuse" => &["fuse"],
        "Connector" => &["connector", "header", "socket", "receptacle", "usb", "plug"],
        "Button" => &["switch", "button"],
        "Crystal" | "Oscillator" => &["crystal", "oscillator", "resonator"],
        "RF" => &["module", "wifi", "bluetooth", "rf "],
        "Sensor" => &["sensor"],
        "Package" => return 1,
        _ => &[],
    };
    if words.iter().any(|w| what.contains(w)) { 2 } else { 0 }
}

/// The names a JLCPCB package may go by in KiCad, most specific first:
/// "SMA(DO-214AC)" → SMA, DO-214AC; "SOD-123FL" → SOD-123FL, SOD-123F; "DFN1006-2L" →
/// DFN1006-2L, DFN1006-2. Generic words ("SMD", "plugin", sizes in mm) are dropped.
fn package_names(package: &str) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    let mut push = |s: String| {
        if !s.is_empty() && !out.contains(&s) {
            out.push(s);
        }
    };
    for part in package.split(['(', ')', ',', '_']) {
        let p = part.trim().to_uppercase().replace([' ', '_'], "-");
        if p.is_empty() || p == "SMD" || p == "PLUGIN" || p == "SMD-" || p.ends_with("MM") {
            continue;
        }
        push(p.clone());
        if let Some(s) = p.strip_suffix("FL") {
            push(format!("{s}F"));
        }
        if let Some(s) = p.strip_suffix('L').filter(|s| s.ends_with(|c: char| c.is_ascii_digit())) {
            push(s.to_string());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lib() -> Footprints {
        let n = |l: &str, f: &str| (l.to_string(), f.to_string());
        Footprints::new(vec![
            n("Diode_SMD", "D_SOD-123"),
            n("Diode_SMD", "D_SOD-123F"),
            n("Diode_SMD", "D_SMA"),
            n("Diode_SMD", "D_SMA_Handsoldering"),
            n("Connector_Coaxial", "SMA_Amphenol_132134-10_Vertical"),
            n("Package_TO_SOT_SMD", "SOT-223-3_TabPin2"),
            n("Package_TO_SOT_SMD", "SOT-23"),
            n("Package_TO_SOT_SMD", "SOT-23-6"),
            n("Resistor_SMD", "R_0805_2012Metric"),
            n("RF_Module", "ESP32-S3-WROOM-1"),
            n("Connector_USB", "USB_C_Receptacle_HRO_TYPE-C-31-M-12"),
        ])
    }

    fn part(mpn: &str, package: &str) -> Listing {
        Listing { mpn: mpn.into(), package: package.into(), ..Default::default() }
    }

    #[test]
    fn package_aliases() {
        assert_eq!(package_names("SMA(DO-214AC)"), ["SMA", "DO-214AC"]);
        assert_eq!(package_names("SOD-123FL"), ["SOD-123FL", "SOD-123F"]);
        assert_eq!(package_names("DFN1006-2L"), ["DFN1006-2L", "DFN1006-2"]);
        assert!(package_names("SMD,25.5x18mm").is_empty());
    }

    #[test]
    fn footprint_hints() {
        let f = lib();
        let mut ss34 = part("SS34", "SMA(DO-214AC)");
        ss34.category = "Schottky Diodes".into();
        assert_eq!(f.hint(&ss34).as_deref(), Some("Diode_SMD:D_SMA (+1)"), "not the SMA coax connector");
        ss34.package = "SMA_DO-214AC".into();
        assert_eq!(f.hint(&ss34).as_deref(), Some("Diode_SMD:D_SMA (+1)"));
        assert_eq!(f.hint(&part("B5819W", "SOD-123")).as_deref(), Some("Diode_SMD:D_SOD-123"));
        assert_eq!(f.hint(&part("DSK34", "SOD-123FL")).as_deref(), Some("Diode_SMD:D_SOD-123F"));
        assert_eq!(f.hint(&part("LDL1117S33R", "SOT-223")).as_deref(), Some("Package_TO_SOT_SMD:SOT-223-3_TabPin2"));
        assert_eq!(f.hint(&part("BAS40", "SOT-23")).as_deref(), Some("Package_TO_SOT_SMD:SOT-23"), "SOT-23-6 is not a SOT-23");
        assert_eq!(f.hint(&part("0805W8F1002T5E", "0805")).as_deref(), Some("Resistor_SMD:R_0805_2012Metric"));
        assert_eq!(f.hint(&part("ESP32-S3-WROOM-1-N16R8", "SMD,25.5x18mm")).as_deref(), Some("RF_Module:ESP32-S3-WROOM-1"));
        assert_eq!(f.hint(&part("TYPE-C-31-M-12", "SMD")).as_deref(), Some("Connector_USB:USB_C_Receptacle_HRO_TYPE-C-31-M-12"));
        assert_eq!(f.hint(&part("XYZ123", "QFN-99")), None);
    }

    #[test]
    fn fee_free_first_then_stock() {
        let l = |c: &str, lib: &str, stock| Listing { lcsc: c.into(), library: lib.into(), stock, ..Default::default() };
        let mut v = vec![l("C1", "extended", 900), l("C2", "preferred", 10), l("C3", "basic", 50), l("C4", "extended", 1000)];
        sort(&mut v);
        let order: Vec<&str> = v.iter().map(|x| x.lcsc.as_str()).collect();
        assert_eq!(order, ["C3", "C2", "C4", "C1"]);
    }
}
