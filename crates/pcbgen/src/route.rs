//! Autorouting with Freerouting via Specctra DSN/SES, written and read here.
//!
//! Freerouting 2.4.1's Linux bundle carries its own Java 25 runtime, so nothing is
//! installed system-wide. Fetch it with scripts/fetch-tools.sh.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use regex::Regex;

use crate::board::Board;
use crate::gates::fill_zones;
use crate::layout::RouteOptions;
use crate::sexpr::{Sexp, dumps, parse};
use crate::{dsn, node, project, ses, stitch, uid};

pub fn freerouting() -> PathBuf {
    crate::repo_root().join("tools/freerouting-2.4.1-linux-x64/bin/freerouting")
}

pub fn load(pcb: &Path) -> Result<Sexp> {
    parse(&std::fs::read_to_string(pcb).with_context(|| pcb.display().to_string())?)
}

pub fn save(pcb: &Path, root: &Sexp) -> Result<()> {
    Ok(std::fs::write(pcb, dumps(root) + "\n")?)
}

/// The board's name (its file stem), for deterministic UUIDs.
fn board_name(pcb: &Path) -> String {
    pcb.file_stem().unwrap_or_default().to_string_lossy().into_owned()
}

/// Minimum hole clearance from the project's design rules (DSN keep-outs around NPTH).
fn hole_clearance(pro: &Path) -> f64 {
    std::fs::read_to_string(pro)
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .and_then(|v| v["board"]["design_settings"]["rules"]["min_hole_clearance"].as_f64())
        .unwrap_or(0.25)
}

/// Export DSN, run Freerouting, import SES, fill, stitch, fill.
pub fn route(pcb: &Path, opts: &RouteOptions) -> Result<()> {
    let fr = freerouting();
    if !fr.exists() {
        bail!("{} missing; run scripts/fetch-tools.sh", fr.display());
    }
    let work = pcb.parent().unwrap().join("route");
    std::fs::create_dir_all(&work)?;
    let name = board_name(pcb);

    // drop earlier routing so re-runs start clean; locked tracks (escape stubs) stay
    let mut root = load(pcb)?;
    root.items_mut().retain(|c| !((c.is("segment") || c.is("arc") || c.is("via")) && !c.flag("locked")));
    save(pcb, &root)?;

    let rules = project::read_classes(&pcb.with_extension("kicad_pro"))?;
    let board = Board::from_sexp(&root)?;
    let hole_clearance = hole_clearance(&pcb.with_extension("kicad_pro"));

    let jobs: Vec<u64> = (0..opts.tries.max(1) as u64).collect();
    let parallel = opts.parallel.clamp(1, jobs.len());
    println!("freerouting: {} footprint orders, {parallel} at a time", jobs.len());
    let t0 = Instant::now();
    let next = AtomicUsize::new(0);
    let results: Vec<(u64, Result<Try>)> = std::thread::scope(|s| {
        let workers: Vec<_> = (0..parallel)
            .map(|_| {
                s.spawn(|| {
                    let mut done = vec![];
                    while let Some(&seed) = jobs.get(next.fetch_add(1, Ordering::Relaxed)) {
                        let r = run_try(&fr, &board, &rules, hole_clearance, opts, seed, &work.join(format!("try-{seed}")));
                        match &r {
                            Ok(t) => println!("freerouting: {}", t.summary()),
                            Err(e) => println!("freerouting: order {seed} failed: {e:#}"),
                        }
                        done.push((seed, r));
                    }
                    done
                })
            })
            .collect();
        workers.into_iter().flat_map(|w| w.join().expect("routing thread panicked")).collect()
    });
    let mut tries: Vec<Try> = vec![];
    for (seed, r) in results {
        match r {
            Ok(t) => tries.push(t),
            Err(e) => println!("freerouting: order {seed} dropped: {e:#}"),
        }
    }
    tries.sort_by_key(|t| t.seed);
    // ties go to the lowest order (min_by keeps the first)
    let Some(best) = tries.iter().min_by(|a, b| a.key().cmp(&b.key())) else {
        bail!("every Freerouting run failed; see {}/try-*/freerouting.log", work.display());
    };
    println!("freerouting: kept order {} of {} ({:.0} s in all): {}", best.seed, tries.len(), t0.elapsed().as_secs_f64(), best.summary());
    // the winner's files where a reader expects them
    for f in ["board.dsn", "board.ses", "freerouting.log"] {
        std::fs::copy(best.dir.join(f), work.join(f))?;
    }
    let summary = serde_json::json!({
        "kept_order": best.seed,
        "score": "fewest unrouted (other nets, then the pour net), then fewest mm thinner than the net class, then fewest vias, then shortest",
        "orders": tries.iter().map(Try::json).collect::<Vec<_>>(),
    });
    std::fs::write(work.join("tries.json"), serde_json::to_string_pretty(&summary)? + "\n")?;
    let (tracks, routed_vias) = (&best.tracks, &best.vias);

    println!("routed: {} track segments, {} vias", tracks.len(), routed_vias.len());
    for (i, t) in tracks.iter().enumerate() {
        root.push(node!(
            "segment",
            node!("start", t.start.x, t.start.y),
            node!("end", t.end.x, t.end.y),
            node!("width", t.width),
            node!("layer", &t.layer),
            node!("net", &t.net),
            node!("uuid", uid(&name, &["track", &i.to_string()]))
        ));
    }
    for (i, v) in routed_vias.iter().enumerate() {
        root.push(via_node(&name, &format!("route/{i}"), v.at, v.size, v.drill, &v.net));
    }
    save(pcb, &root)?;
    fill_zones(pcb)?;

    let mut nets: Vec<(String, f64)> = opts.stitch_net.iter().map(|n| (n.clone(), opts.stitch_pitch)).collect();
    nets.extend(opts.stitch_local.iter().cloned());
    if !nets.is_empty() {
        let mut root = load(pcb)?; // zones come filled by kicad-cli
        let mut board = Board::from_sexp(&root)?;
        for (net, pitch) in nets {
            let added = stitch::stitch(&board, opts, &net, pitch);
            println!("stitching: {} {net} vias", added.len());
            for (i, p) in added.into_iter().enumerate() {
                let (d, h) = opts.stitch_via;
                root.push(via_node(&name, &format!("stitch/{net}/{i}"), p, d, h, &net));
                board.vias.push(crate::board::Via { at: p, size: d, drill: h, net: net.clone(), locked: false });
            }
        }
        save(pcb, &root)?;
        fill_zones(pcb)?;
    }
    Ok(())
}

pub fn via_node(board: &str, key: &str, at: crate::geom::Pt, size: f64, drill: f64, net: &str) -> Sexp {
    node!(
        "via",
        node!("at", at.x, at.y),
        node!("size", size),
        node!("drill", drill),
        node!("layers", "F.Cu", "B.Cu"),
        node!("net", net),
        node!("uuid", uid(board, &["via", key]))
    )
}

/// One Freerouting run on one footprint order, read back but not yet on the board.
pub struct Try {
    pub seed: u64,
    pub dir: PathBuf,
    pub tracks: Vec<ses::Track>,
    pub vias: Vec<ses::RoutedVia>,
    /// Unrouted connections per net, from Freerouting's log, pour net excluded.
    pub unrouted: Vec<(String, usize)>,
    /// Unrouted connections of the pour net (its pour and stitching vias may finish them).
    pub pour_unrouted: usize,
    /// Track length thinner than its net class (the router necks down at small pads), mm.
    pub thin_mm: f64,
    pub length_mm: f64,
    /// Freerouting's own clearance-violation count (KiCad's DRC is the gate).
    pub fr_violations: Option<usize>,
    pub secs: f64,
}

impl Try {
    /// Lower is better: unrouted other nets, unrouted pour net, necked-down length (0.1 mm
    /// steps), vias, length (0.1 mm steps).
    pub fn key(&self) -> (usize, usize, i64, usize, i64) {
        let n: usize = self.unrouted.iter().map(|u| u.1).sum();
        (n, self.pour_unrouted, (self.thin_mm * 10.0).round() as i64, self.vias.len(), (self.length_mm * 10.0).round() as i64)
    }

    pub fn summary(&self) -> String {
        let unrouted = if self.unrouted.is_empty() {
            "none".to_string()
        } else {
            self.unrouted.iter().map(|(n, k)| format!("{n} ({k})")).collect::<Vec<_>>().join(", ")
        };
        format!(
            "order {}: unrouted {unrouted}, pour net {}, {:.1} mm thinner than class, {} vias, {:.1} mm of track ({:.0} s)",
            self.seed,
            self.pour_unrouted,
            self.thin_mm,
            self.vias.len(),
            self.length_mm,
            self.secs
        )
    }

    fn json(&self) -> serde_json::Value {
        let r1 = |v: f64| (v * 10.0).round() / 10.0;
        serde_json::json!({
            "order": self.seed,
            "unrouted": self.unrouted.iter().map(|(n, k)| (n.clone(), serde_json::json!(k))).collect::<serde_json::Map<_, _>>(),
            "pour_net_unrouted": self.pour_unrouted,
            "thinner_than_class_mm": r1(self.thin_mm),
            "vias": self.vias.len(),
            "track_length_mm": r1(self.length_mm),
            "freerouting_clearance_violations": self.fr_violations,
        })
    }
}

/// Write the DSN for footprint order `seed` into `dir`, run Freerouting there and read the
/// session back.
fn run_try(fr: &Path, board: &Board, rules: &project::BoardRules, hole_clearance: f64, opts: &RouteOptions, seed: u64, dir: &Path) -> Result<Try> {
    let _ = std::fs::remove_dir_all(dir);
    std::fs::create_dir_all(dir)?;
    let (dsn_path, ses_path, log, result) =
        (dir.join("board.dsn"), dir.join("board.ses"), dir.join("freerouting.log"), dir.join("result.json"));
    let via_table = dsn::write(board, rules, hole_clearance, seed, &dsn_path)?;
    let args: Vec<String> = vec![
        "-de".into(),
        dsn_path.to_string_lossy().into(),
        "-do".into(),
        ses_path.to_string_lossy().into(),
        "-mp".into(),
        opts.max_passes.to_string(),
        "--gui.enabled=false".into(),
        format!("--router.copperToEdgeClearanceUm={}", opts.edge_clearance_um),
        format!("--router.scoring.viaCosts={}", opts.via_costs),
        format!("--router.fanout.enabled={}", opts.fanout),
        format!("--router.resultJsonPath={}", result.display()),
        "--usage_and_diagnostic_data.disable_analytics=true".into(),
        format!("--user_data_path={}", dir.join("fr-data").display()),
    ];
    let t0 = Instant::now();
    let fh = std::fs::File::create(&log)?;
    let mut child = Command::new(fr).args(&args).stdout(fh.try_clone()?).stderr(fh).stdin(Stdio::null()).spawn()?;
    loop {
        if child.try_wait()?.is_some() {
            break;
        }
        if t0.elapsed() > Duration::from_secs(opts.timeout_s) {
            child.kill()?;
            let _ = child.wait();
            bail!("took over {} s; see {}", opts.timeout_s, log.display());
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    if !ses_path.exists() {
        bail!("no session file; see {}", log.display());
    }
    let text = std::fs::read_to_string(&log)?;
    let Some(unrouted) = unrouted(&text) else {
        bail!("no final score in {}", log.display());
    };
    let (other, pour): (Vec<_>, Vec<_>) = unrouted.into_iter().partition(|(n, _)| Some(n.as_str()) != opts.stitch_net.as_deref());
    let (tracks, vias) = ses::read(&ses_path, &via_table)?;
    let fr_violations = std::fs::read_to_string(&result)
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .and_then(|v| v["board_statistics"]["clearance_violations"]["total_count"].as_u64())
        .map(|n| n as usize);
    Ok(Try {
        seed,
        dir: dir.to_path_buf(),
        thin_mm: thin_mm(tracks.iter().map(|t| (t.net.as_str(), t.width, (t.end - t.start).norm())), rules),
        length_mm: tracks.iter().map(|t| (t.end - t.start).norm()).sum(),
        tracks,
        vias,
        unrouted: other,
        pour_unrouted: pour.iter().map(|p| p.1).sum(),
        fr_violations,
        secs: t0.elapsed().as_secs_f64(),
    })
}

/// Length of track narrower than its net class asks for, from (net, width, length) mm.
pub fn thin_mm<'a>(tracks: impl Iterator<Item = (&'a str, f64, f64)>, rules: &project::BoardRules) -> f64 {
    // (an empty f64 sum is -0.0)
    0.0 + tracks.filter(|(net, w, _)| *w < rules.class_of(net).track - 1e-6).map(|t| t.2).sum::<f64>()
}

/// Unrouted connections per net from a Freerouting log; None if the log has no final score
/// (the run didn't finish).
pub fn unrouted(log: &str) -> Option<Vec<(String, usize)>> {
    if !log.contains("final score:") {
        return None;
    }
    let re = Regex::new(r"Net '(.+)' \((\d+) unrouted").unwrap();
    let mut out: Vec<(String, usize)> = vec![];
    for c in re.captures_iter(log) {
        let n: usize = c[2].parse().unwrap_or(1);
        // the log lists the result after each stage; keep the last count per net
        match out.iter_mut().find(|(name, _)| name == &c[1]) {
            Some(e) => e.1 = n,
            None => out.push((c[1].to_string(), n)),
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{BoardRules, NetClass};

    const LOG: &str = "\
... Auto-routing pass #3 ... score 946.23 (2 unrouted and 20 violations)
The following connections could not be routed -- please review your design:
  Net 'GND' (1 unrouted connection):
    - U4-4  ->  C6-2
  Net '/I2C_SCL' (2 unrouted connections):
    - U1-9  ->  R5-2
... Auto-routing stage completed: started with 89 unrouted nets, completed in 14.70 seconds, final score: 946.23 (3 unrouted and 20 violations)
";

    #[test]
    fn reads_unrouted_from_log() {
        assert_eq!(unrouted(LOG).unwrap(), [("GND".to_string(), 1), ("/I2C_SCL".to_string(), 2)]);
        assert_eq!(unrouted("no score yet"), None);
        assert_eq!(unrouted("final score: 1 (0 unrouted and 0 violations)").unwrap(), []);
    }

    #[test]
    fn thin_track_length() {
        let rules = BoardRules {
            classes: vec![NetClass::new("Default", 0.2, 0.15, 0.6, 0.3), NetClass::new("Power", 0.3, 0.2, 0.8, 0.4).patterns(&["GND"])],
            ..Default::default()
        };
        let tracks = [("GND", 0.225, 2.0), ("GND", 0.3, 5.0), ("/SDA", 0.15, 1.0), ("/SDA", 0.2, 9.0)];
        assert_eq!(thin_mm(tracks.into_iter(), &rules), 3.0);
        assert!(thin_mm(std::iter::empty(), &rules).is_sign_positive());
    }

    fn t(seed: u64, unrouted: usize, pour: usize, thin: f64, vias: usize, len: f64) -> Try {
        let via = || ses::RoutedVia { at: crate::geom::pt(0.0, 0.0), size: 0.6, drill: 0.3, net: "GND".into() };
        Try {
            seed,
            dir: PathBuf::new(),
            tracks: vec![],
            vias: (0..vias).map(|_| via()).collect(),
            unrouted: if unrouted > 0 { vec![("+3V3".into(), unrouted)] } else { vec![] },
            pour_unrouted: pour,
            thin_mm: thin,
            length_mm: len,
            fr_violations: None,
            secs: 0.0,
        }
    }

    /// Unrouted first, then the pour net, then necked-down track, vias, length.
    #[test]
    fn score_order() {
        let mut v = [
            t(0, 1, 0, 0.0, 10, 100.0),
            t(1, 0, 1, 0.0, 10, 100.0),
            t(2, 0, 0, 5.0, 10, 100.0),
            t(3, 0, 0, 0.0, 30, 100.0),
            t(4, 0, 0, 0.0, 20, 900.0),
            t(5, 0, 0, 0.0, 20, 100.0),
            t(6, 0, 0, 0.04, 20, 100.0), // under 0.05 mm rounds to 0: a tie, the lower order wins
        ];
        v.sort_by_key(|t| (t.key(), t.seed));
        assert_eq!(v.iter().map(|t| t.seed).collect::<Vec<_>>(), [5, 6, 4, 3, 2, 1, 0]);
    }
}
