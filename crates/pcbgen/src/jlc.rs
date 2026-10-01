//! JLCPCB's public parts-search API: the backend jlcpcb.com/parts itself calls (no login;
//! undocumented, so a change on their side shows up here as a fetch or parse error).
//! Method and field meanings: `research/2026-09-29-parts-starter.md`.
//!
//! Requests go through curl, like kicad-cli is run (no HTTP crate). Answers are kept for a
//! day in `~/.cache/pcbgen/jlc/`, keyed by a hash of the request body: prices and stock
//! are live, the cache only saves repeat calls in one working session. `Jlc { fresh: true }`
//! skips it (the `--fresh` flags).
//!
//! Filters, checked live 2026-10-01 with "schottky 40V": `"componentLibraryType":"base"`
//! gives only Basic parts (5 of 4,679), `"preferredComponentFlag":true` only Preferred
//! Extended parts (56, every row with the flag true), `"stockFlag":true` only parts in
//! stock (2,716, lowest stock 1). `pageSize` 100 works; `currentPage` pages.
//!
//! Class: `componentLibraryType` "base" = Basic; "expand" = Extended, Preferred Extended
//! when `preferredComponentFlag` is true. Earlier research found that flag false on every
//! part it sampled, so a "preferred" row is believed but an "extended" one may in fact be
//! Preferred (the part page shows the badge).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

pub const API: &str = "https://jlcpcb.com/api/overseas-pcb-order/v1/shoppingCart/smtGood/selectSmtComponentList";
/// JLCPCB Economic PCBA loading fee per unique Extended part (HARDWARE_LESSONS, VERIFIED).
pub const EXTENDED_FEE: f64 = 3.07;
/// Rows per page asked for in a search (the API accepts 100).
pub const PAGE_SIZE: u64 = 100;
const CACHE_AGE: Duration = Duration::from_secs(24 * 3600);
const UA: &str = "User-Agent: Mozilla/5.0 (X11; Linux x86_64)";

/// One part as JLCPCB lists it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Listing {
    /// LCSC code, "C2913202".
    pub lcsc: String,
    /// "basic", "preferred" (Preferred Extended) or "extended".
    pub library: String,
    pub stock: u64,
    /// Minimum order (`leastPatchNumber`) and attrition (`lossNumber`) for assembly.
    pub minimum: u64,
    pub attrition: u64,
    /// (from qty, to qty or None for no limit, USD each).
    pub tiers: Vec<(u64, Option<u64>, f64)>,
    /// Manufacturer part number (`componentModelEn`).
    pub mpn: String,
    pub manufacturer: String,
    /// Package as JLCPCB writes it (`componentSpecificationEn`): "SOD-123", "0805", "SMD,25.5x18mm".
    pub package: String,
    pub category: String,
    pub description: String,
    /// (name, value) from the listing's parameter table.
    pub attributes: Vec<(String, String)>,
    /// Datasheet link as listed (often an lcsc.com page that serves HTML to curl).
    pub datasheet: String,
    /// JLCPCB's own copy of the datasheet, when it has one (serves the PDF).
    pub datasheet_file: Option<String>,
    /// jlcpcb.com part page.
    pub page: String,
}

/// What a search keeps.
#[derive(Clone, Copy, Debug, Default)]
pub struct Filter {
    /// Basic parts only.
    pub basic_only: bool,
    /// Basic and Preferred Extended parts (no loading fee).
    pub no_fee: bool,
    /// Parts JLCPCB has in stock.
    pub in_stock: bool,
}

impl Listing {
    /// Basic and Preferred Extended parts carry no loading fee.
    pub fn fee_free(&self) -> bool {
        self.library != "extended"
    }
    /// "Basic", "Preferred" or "Extended +$3.07".
    pub fn class(&self) -> String {
        match self.library.as_str() {
            "basic" => "Basic".into(),
            "preferred" => "Preferred".into(),
            _ => format!("Extended +${EXTENDED_FEE:.2}"),
        }
    }
    /// What JLCPCB buys for `needed` placements: needed + attrition, at least the
    /// minimum (the model in the cost estimate, INFERRED).
    pub fn order_qty(&self, needed: u64) -> u64 {
        (needed + self.attrition).max(self.minimum)
    }
    /// Unit price at a quantity: the tier that covers it, else the nearest one.
    pub fn unit(&self, qty: u64) -> f64 {
        let covers = |t: &&(u64, Option<u64>, f64)| t.0 <= qty && t.1.is_none_or(|to| qty <= to);
        self.tiers.iter().find(covers).or_else(|| self.tiers.iter().rfind(|t| t.0 <= qty)).unwrap_or(&self.tiers[0]).2
    }
}

/// The class rule: library type first, the preferred flag only within "expand".
pub fn library(kind: Option<&str>, preferred: Option<bool>) -> Option<&'static str> {
    match (kind, preferred) {
        (Some("base"), _) => Some("basic"),
        (Some("expand"), Some(true)) => Some("preferred"),
        (Some("expand"), _) => Some("extended"),
        _ => None,
    }
}

fn text(c: &Value, key: &str) -> String {
    c[key].as_str().unwrap_or("").trim().to_string()
}

/// One row of `data.componentPageInfo.list`.
pub fn parse(c: &Value) -> Result<Listing> {
    let lcsc = text(c, "componentCode");
    let library = library(c["componentLibraryType"].as_str(), c["preferredComponentFlag"].as_bool())
        .with_context(|| format!("{lcsc}: unknown library type {:?}", c["componentLibraryType"]))?;
    let mut tiers = c["componentPrices"]
        .as_array()
        .with_context(|| format!("{lcsc}: no componentPrices"))?
        .iter()
        .map(|t| {
            let to = t["endNumber"].as_i64().filter(|&n| n >= 0).map(|n| n as u64);
            Ok((t["startNumber"].as_u64().context("startNumber")?, to, t["productPrice"].as_f64().context("productPrice")?))
        })
        .collect::<Result<Vec<_>>>()?;
    if tiers.is_empty() {
        bail!("{lcsc}: no price tiers");
    }
    tiers.sort_by_key(|t| t.0);
    let attributes = c["attributes"]
        .as_array()
        .map(|a| a.iter().map(|x| (text(x, "attribute_name_en"), text(x, "attribute_value_name"))).collect())
        .unwrap_or_default();
    let suffix = text(c, "urlSuffix");
    Ok(Listing {
        library: library.into(),
        stock: c["stockCount"].as_u64().unwrap_or(0),
        minimum: c["leastPatchNumber"].as_u64().unwrap_or(0),
        attrition: c["lossNumber"].as_u64().unwrap_or(0),
        tiers,
        mpn: text(c, "componentModelEn"),
        manufacturer: text(c, "componentBrandEn"),
        package: text(c, "componentSpecificationEn"),
        category: text(c, "componentTypeEn"),
        description: text(c, "describe"),
        attributes,
        datasheet: text(c, "dataManualUrl"),
        datasheet_file: c["dataManualFileAccessId"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(|id| format!("https://jlcpcb.com/api/file/downloadByFileSystemAccessId/{id}")),
        page: if suffix.is_empty() { format!("https://jlcpcb.com/partdetail/{lcsc}") } else { format!("https://jlcpcb.com/partdetail/{suffix}") },
        lcsc,
    })
}

fn rows(answer: &Value) -> Result<&Vec<Value>> {
    if let Some(code) = answer["code"].as_i64().filter(|&c| c != 200) {
        bail!("JLCPCB parts API answered code {code}: {}", answer["message"]);
    }
    let info = &answer["data"]["componentPageInfo"];
    if info["list"].is_null() && info["total"].as_u64() == Some(0) {
        static EMPTY: Vec<Value> = vec![];
        return Ok(&EMPTY);
    }
    info["list"].as_array().context("no data.componentPageInfo.list in the answer")
}

/// Every row of an answer.
pub fn listings(answer: &Value) -> Result<Vec<Listing>> {
    rows(answer)?.iter().map(parse).collect()
}

/// The listing for one LCSC code, exact code match only.
pub fn listing(answer: &Value, lcsc: &str) -> Result<Listing> {
    let c = rows(answer)?.iter().find(|c| c["componentCode"] == lcsc).with_context(|| format!("{lcsc} not in JLCPCB's parts list"))?;
    parse(c)
}

/// Request bodies for one search page: one per class asked for (no-fee = Basic + Preferred).
pub fn bodies(keyword: &str, f: &Filter, page: u64) -> Vec<Value> {
    let base = |extra: &[(&str, Value)]| {
        let mut b = json!({"keyword": keyword, "currentPage": page, "pageSize": PAGE_SIZE});
        if f.in_stock {
            b["stockFlag"] = true.into();
        }
        for (k, v) in extra {
            b[*k] = v.clone();
        }
        b
    };
    if f.basic_only {
        vec![base(&[("componentLibraryType", "base".into())])]
    } else if f.no_fee {
        vec![base(&[("componentLibraryType", "base".into())]), base(&[("preferredComponentFlag", true.into())])]
    } else {
        vec![base(&[])]
    }
}

/// FNV-1a 64: a hash that stays the same across Rust releases, for cache file names.
fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ *b as u64).wrapping_mul(0x0100_0000_01b3))
}

/// `~/.cache/pcbgen/jlc` (or under `$XDG_CACHE_HOME`).
pub fn cache_dir() -> PathBuf {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".cache"));
    base.join("pcbgen/jlc")
}

fn young(path: &Path) -> bool {
    let age = std::fs::metadata(path).and_then(|m| m.modified()).ok().and_then(|t| SystemTime::now().duration_since(t).ok());
    age.is_some_and(|a| a < CACHE_AGE)
}

/// The API client.
#[derive(Clone, Debug, Default)]
pub struct Jlc {
    /// Ask JLCPCB even when a cached answer is less than a day old.
    pub fresh: bool,
}

impl Jlc {
    /// One API call (or its cached answer).
    pub fn post(&self, body: &Value) -> Result<Value> {
        let body = body.to_string();
        let path = cache_dir().join(format!("{:016x}.json", fnv(body.as_bytes())));
        if !self.fresh
            && young(&path)
            && let Some(v) = std::fs::read(&path).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        {
            return Ok(v);
        }
        let out = Command::new("curl")
            .args(["-sS", "-f", "-m", "30", "--retry", "2", "-X", "POST", API, "-H", "Content-Type: application/json"])
            .args(["-H", UA, "-d", &body])
            .output()
            .context("running curl")?;
        if !out.status.success() {
            bail!("JLCPCB parts API: {}", String::from_utf8_lossy(&out.stderr).trim());
        }
        let answer: Value = serde_json::from_slice(&out.stdout).context("JLCPCB's answer is not JSON")?;
        rows(&answer)?; // only good answers are kept
        if std::fs::create_dir_all(cache_dir()).is_ok() {
            let _ = std::fs::write(&path, &out.stdout);
        }
        Ok(answer)
    }

    /// One part by its LCSC code (exact match).
    pub fn get(&self, lcsc: &str) -> Result<Listing> {
        let answer = self.post(&json!({"keyword": lcsc, "currentPage": 1, "pageSize": 10})).with_context(|| lcsc.to_string())?;
        listing(&answer, lcsc)
    }

    /// Up to `pages` pages of each class asked for, duplicates dropped, in API order.
    pub fn search(&self, keyword: &str, f: &Filter, pages: u64) -> Result<Vec<Listing>> {
        let mut out: Vec<Listing> = vec![];
        for kind in 0..bodies(keyword, f, 1).len() {
            for page in 1..=pages.max(1) {
                let answer = self.post(&bodies(keyword, f, page)[kind])?;
                let got = listings(&answer)?;
                let n = got.len() as u64;
                for l in got {
                    if !out.iter().any(|o| o.lcsc == l.lcsc) {
                        out.push(l);
                    }
                }
                let total = answer["data"]["componentPageInfo"]["total"].as_u64().unwrap_or(0);
                if n < PAGE_SIZE || page * PAGE_SIZE >= total {
                    break;
                }
            }
        }
        if f.basic_only {
            out.retain(|l| l.library == "basic");
        } else if f.no_fee {
            out.retain(Listing::fee_free);
        }
        if f.in_stock {
            out.retain(|l| l.stock > 0);
        }
        Ok(out)
    }

    /// Downloads a part's datasheet PDF into the cache and runs `pdftotext -layout` beside
    /// it: (pdf, txt). JLCPCB's own copy first, then LCSC's wmsc mirror of the listed link
    /// (lcsc.com/datasheet/ links serve an HTML page to curl).
    pub fn datasheet(&self, l: &Listing) -> Result<(PathBuf, PathBuf)> {
        let dir = cache_dir().join("datasheets");
        std::fs::create_dir_all(&dir)?;
        let pdf = dir.join(format!("{}.pdf", l.lcsc));
        let txt = dir.join(format!("{}.txt", l.lcsc));
        if self.fresh || !is_pdf(&pdf) {
            let mut urls: Vec<String> = l.datasheet_file.iter().cloned().collect();
            urls.extend(wmsc(&l.datasheet));
            urls.push(l.datasheet.clone());
            let mut tried = vec![];
            for url in urls.iter().filter(|u| !u.is_empty()) {
                let ok = Command::new("curl")
                    .args(["-sS", "-f", "-L", "-m", "120", "-H", UA, "-o"])
                    .arg(&pdf)
                    .arg(url)
                    .status()
                    .is_ok_and(|s| s.success());
                if ok && is_pdf(&pdf) {
                    tried.clear();
                    break;
                }
                tried.push(url.clone());
            }
            if !tried.is_empty() || !is_pdf(&pdf) {
                let _ = std::fs::remove_file(&pdf);
                bail!("{}: no PDF from {}", l.lcsc, if tried.is_empty() { "any datasheet link".into() } else { tried.join(", ") });
            }
        }
        let out = Command::new("pdftotext").arg("-layout").arg(&pdf).arg(&txt).output().context("running pdftotext")?;
        if !out.status.success() {
            bail!("pdftotext: {}", String::from_utf8_lossy(&out.stderr).trim());
        }
        Ok((pdf, txt))
    }
}

fn is_pdf(path: &Path) -> bool {
    std::fs::read(path).is_ok_and(|b| b.starts_with(b"%PDF"))
}

/// `www.lcsc.com/datasheet/lcsc_datasheet_X.pdf` → `wmsc.lcsc.com/wmsc/upload/file/pdf/v2/lcsc/X.pdf`.
pub fn wmsc(url: &str) -> Option<String> {
    let file = url.split("/datasheet/lcsc_datasheet_").nth(1)?;
    Some(format!("https://wmsc.lcsc.com/wmsc/upload/file/pdf/v2/lcsc/{file}"))
}

/// One part by LCSC code, through the 1-day cache.
pub fn get(lcsc: &str) -> Result<Listing> {
    Jlc::default().get(lcsc)
}

/// A search through the 1-day cache.
pub fn search(keyword: &str, f: &Filter, pages: u64) -> Result<Vec<Listing>> {
    Jlc::default().search(keyword, f, pages)
}

/// An MPN reduced for comparison: upper case; spaces, dashes, dots, slashes and
/// underscores dropped; bracketed groups ("(LF)(SN)") dropped.
pub fn mpn_norm(mpn: &str) -> String {
    let mut out = String::new();
    let mut depth = 0;
    for ch in mpn.chars() {
        match ch {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = (depth - 1).max(0),
            _ if depth > 0 => {}
            c if c.is_alphanumeric() || c == '#' || c == '+' => out.extend(c.to_uppercase()),
            _ => {}
        }
    }
    out
}

/// Ordering-code tails that only name the packaging (tape and reel, lead-free, ...).
const PACKAGING: [&str; 16] = ["TR", "T", "R", "RL", "CT", "TRPBF", "PBF", "#PBF", "LF", "G", "Z", "E3", "REEL", "TAPE", "TRG", "TB"];

/// Whether two MPNs name the same part: equal once normalised, or one is the other plus a
/// packaging tail ("LDL1117S33R" vs "LDL1117S33TR" is not; "AO3401A" vs "AO3401A-TR" is).
pub fn mpn_matches(a: &str, b: &str) -> bool {
    let (a, b) = (mpn_norm(a), mpn_norm(b));
    if a.is_empty() || b.is_empty() {
        return false;
    }
    if a == b {
        return true;
    }
    let (short, long) = if a.len() < b.len() { (&a, &b) } else { (&b, &a) };
    long.strip_prefix(short.as_str()).is_some_and(|tail| PACKAGING.contains(&tail))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Value {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/jlc-search.json");
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    #[test]
    fn parses_a_real_answer() {
        let all = listings(&fixture()).unwrap();
        assert_eq!(all.len(), 3);
        let esp = &all[0];
        assert_eq!(esp.lcsc, "C2913202");
        assert_eq!(esp.library, "extended");
        assert_eq!(esp.class(), "Extended +$3.07");
        assert_eq!(esp.mpn, "ESP32-S3-WROOM-1-N16R8");
        assert_eq!(esp.manufacturer, "Espressif Systems");
        assert_eq!(esp.package, "SMD,25.5x18mm");
        assert_eq!(esp.category, "WiFi Modules");
        assert_eq!(esp.stock, 29043);
        assert_eq!(esp.tiers[0], (1, Some(9), 5.1371));
        assert_eq!(esp.tiers.last().unwrap(), &(1300, None, 3.3814));
        assert_eq!(esp.page, "https://jlcpcb.com/partdetail/3198300-ESP32_S3_WROOM_1N16R8/C2913202");
        assert!(esp.datasheet.ends_with("_C2913202.pdf"));
        assert_eq!(esp.datasheet_file.as_deref(), Some("https://jlcpcb.com/api/file/downloadByFileSystemAccessId/8555725843657703424"));
        assert!(esp.description.contains("2.4GHz"));
        assert!(esp.attributes.contains(&("Antenna Type".into(), "On-board PCB Antenna".into())));
        let b5819 = &all[1];
        assert_eq!((b5819.library.as_str(), b5819.package.as_str(), b5819.minimum, b5819.attrition), ("preferred", "SOD-123", 15, 10));
        assert!(b5819.fee_free());
        assert_eq!(all[2].library, "basic");
        assert_eq!(all[2].package, "SMA(DO-214AC)");
        assert_eq!(listing(&fixture(), "C8678").unwrap().mpn, "SS34");
        assert!(listing(&fixture(), "C867").is_err(), "only an exact code match counts");
    }

    #[test]
    fn class_rule() {
        assert_eq!(library(Some("base"), Some(true)), Some("basic"));
        assert_eq!(library(Some("base"), None), Some("basic"));
        assert_eq!(library(Some("expand"), Some(true)), Some("preferred"));
        assert_eq!(library(Some("expand"), Some(false)), Some("extended"));
        assert_eq!(library(Some("expand"), None), Some("extended"));
        assert_eq!(library(Some("other"), None), None);
        assert_eq!(library(None, Some(true)), None);
    }

    #[test]
    fn tiers() {
        let l = Listing { tiers: vec![(1, Some(9), 0.5), (10, Some(29), 0.4), (30, None, 0.3)], minimum: 20, attrition: 10, ..Default::default() };
        assert_eq!(l.order_qty(2), 20, "minimum");
        assert_eq!(l.order_qty(15), 25, "needed + attrition");
        assert_eq!(l.unit(1), 0.5);
        assert_eq!(l.unit(9), 0.5);
        assert_eq!(l.unit(10), 0.4);
        assert_eq!(l.unit(29), 0.4);
        assert_eq!(l.unit(1000), 0.3);
        let gap = Listing { tiers: vec![(5, Some(9), 0.5), (20, None, 0.3)], ..Default::default() };
        assert_eq!(gap.unit(1), 0.5, "below the first tier: the first");
        assert_eq!(gap.unit(12), 0.5, "in a gap: the nearest tier below");
    }

    #[test]
    fn search_bodies() {
        let f = Filter { no_fee: true, in_stock: true, ..Default::default() };
        let b = bodies("schottky 40V", &f, 2);
        assert_eq!(b.len(), 2);
        assert_eq!(b[0]["componentLibraryType"], "base");
        assert_eq!(b[1]["preferredComponentFlag"], true);
        assert_eq!((b[1]["currentPage"].as_u64(), b[1]["stockFlag"].as_bool()), (Some(2), Some(true)));
        let b = bodies("x", &Filter { basic_only: true, no_fee: true, ..Default::default() }, 1);
        assert_eq!(b.len(), 1, "basic only wins");
        assert!(b[0].get("stockFlag").is_none());
        assert_eq!(bodies("x", &Filter::default(), 1)[0].as_object().unwrap().len(), 3);
    }

    #[test]
    fn mpn_normaliser() {
        assert_eq!(mpn_norm("S2B-PH-SM4-TB(LF)(SN)"), "S2BPHSM4TB");
        assert_eq!(mpn_norm(" smf5.0a "), "SMF50A");
        assert!(mpn_matches("ESP32-S3-WROOM-1-N16R8", "esp32 s3 wroom 1 n16r8"));
        assert!(mpn_matches("S2B-PH-SM4-TB(LF)(SN)", "S2B-PH-SM4-TB"));
        assert!(mpn_matches("AO3401A", "AO3401A-TR"));
        assert!(mpn_matches("LM358DR", "LM358DRG"));
        assert!(!mpn_matches("ESP32-S3-WROOM-1-N16R8", "ESP32-S3-WROOM-1-N8R8"));
        assert!(!mpn_matches("SHT40-AD1B-R2", "SHT40-AD1B"), "R2 is a reel size, but also a real suffix elsewhere: kept strict");
        assert!(!mpn_matches("SS34", "SS340"));
        assert!(!mpn_matches("", ""));
    }

    #[test]
    fn datasheet_mirror() {
        assert_eq!(
            wmsc("https://www.lcsc.com/datasheet/lcsc_datasheet_2411121101_Espressif-Systems-ESP32-S3-WROOM-1-N16R8_C2913202.pdf").as_deref(),
            Some("https://wmsc.lcsc.com/wmsc/upload/file/pdf/v2/lcsc/2411121101_Espressif-Systems-ESP32-S3-WROOM-1-N16R8_C2913202.pdf")
        );
        assert_eq!(wmsc("https://example.com/x.pdf"), None);
    }

    #[test]
    fn cache_key_is_stable() {
        assert_eq!(fnv(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv(b"a"), 0xaf63_dc4c_8601_ec8c);
    }
}
