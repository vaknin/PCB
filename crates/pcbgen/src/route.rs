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
use crate::layout::{RouteOptions, Waiver};
use crate::sexpr::{Sexp, dumps, parse};
use crate::{dsn, failure, node, project, ses, stitch, uid};

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

/// What every Freerouting run of one board shares.
struct Router<'a> {
    fr: PathBuf,
    board: Board,
    rules: project::BoardRules,
    hole_clearance: f64,
    opts: &'a RouteOptions,
    work: PathBuf,
}

impl Router<'_> {
    /// Route these footprint orders, `parallel` at a time; the ones that finished, by order.
    fn run(&self, seeds: &[u64]) -> Vec<Try> {
        let parallel = self.opts.parallel.clamp(1, seeds.len().max(1));
        println!("freerouting: {} footprint orders, {parallel} at a time", seeds.len());
        pool(seeds, parallel, |&seed| {
            let r = run_try(&self.fr, &self.board, &self.rules, self.hole_clearance, self.opts, seed, &self.work.join(format!("try-{seed}")));
            match &r {
                Ok(t) => println!("freerouting: {}", t.summary()),
                Err(e) => println!("freerouting: order {seed} failed: {e:#}"),
            }
            r
        })
        .into_iter()
        .filter_map(Result::ok) // failed orders were reported as they finished
        .collect()
    }
}

/// Run `f` on every job, `n` at a time; the results in job order.
fn pool<J: Sync, R: Send>(jobs: &[J], n: usize, f: impl Fn(&J) -> R + Sync) -> Vec<R> {
    let next = AtomicUsize::new(0);
    let (f, next) = (&f, &next);
    let mut done: Vec<(usize, R)> = std::thread::scope(|s| {
        let workers: Vec<_> = (0..n.clamp(1, jobs.len().max(1)))
            .map(|_| {
                s.spawn(move || {
                    let mut done = vec![];
                    loop {
                        let k = next.fetch_add(1, Ordering::Relaxed);
                        let Some(j) = jobs.get(k) else { break };
                        done.push((k, f(j)));
                    }
                    done
                })
            })
            .collect();
        workers.into_iter().flat_map(|w| w.join().expect("worker thread panicked")).collect()
    });
    done.sort_by_key(|d| d.0);
    done.into_iter().map(|d| d.1).collect()
}

/// The next step of the route stage when checking has started (D-020), cheapest first.
#[derive(Debug, PartialEq)]
enum Step {
    /// A checked order is clean, or there is nothing left to try.
    Stop,
    /// DRC-check every order already routed but not yet checked (~15 s each).
    CheckRest,
    /// Route `extra_tries` new orders (~55 s each) and check them.
    RouteMore,
}

/// `best_open`: the open DRC items of the best checked order (None if no check finished);
/// `unchecked`: routed orders not checked yet; `rounds`: extra rounds routed so far.
fn next_step(best_open: Option<usize>, unchecked: usize, rounds: u32, opts: &RouteOptions) -> Step {
    if best_open == Some(0) || opts.drc_checks == 0 {
        Step::Stop
    } else if unchecked > 0 {
        Step::CheckRest
    } else if rounds < opts.extra_rounds && opts.extra_tries > 0 {
        Step::RouteMore
    } else {
        Step::Stop
    }
}

/// Orders best first by the router's own numbers; ties go to the lower order.
fn rank(tries: &[Try]) -> Vec<usize> {
    let mut ranked: Vec<usize> = (0..tries.len()).collect();
    ranked.sort_by_key(|&i| (tries[i].key(), tries[i].seed));
    ranked
}

/// Of the checked orders, the one to keep (see `pick`).
fn best(tries: &[Try]) -> Option<usize> {
    let checked: Vec<usize> = rank(tries).into_iter().filter(|&i| tries[i].drc.is_some()).collect();
    pick(&checked.iter().map(|&i| tries[i].drc.as_ref().map_or(0, Vec::len)).collect::<Vec<_>>()).map(|k| checked[k])
}

/// The router's keep-outs for `RouteOptions::pad_rings`: each pad's rectangle grown by the
/// margin, on its copper layers, no tracks or vias. Only pads on a poured net, which the
/// pour connects without a track.
fn pad_rings(board: &Board, opts: &RouteOptions) -> Result<Vec<crate::board::RuleArea>> {
    let poured: Vec<&str> = opts.stitch_net.iter().map(String::as_str).chain(opts.stitch_local.iter().map(|l| l.0.as_str())).collect();
    let mut out = vec![];
    for ring in &opts.pad_rings {
        let Some(fp) = board.footprints.iter().find(|f| f.reference == ring.reference) else {
            bail!("pad ring: no part {}", ring.reference);
        };
        let pads: Vec<_> = fp.pads.iter().filter(|p| p.number == ring.pad).collect();
        if pads.is_empty() {
            bail!("pad ring: {} has no pad {}", ring.reference, ring.pad);
        }
        for (k, pad) in pads.into_iter().enumerate() {
            let net = pad.net.as_deref().unwrap_or("");
            if !poured.contains(&net) {
                bail!("pad ring: {} pad {} is on {net:?}, not a poured net ({poured:?}); a ring would leave it unroutable", ring.reference, ring.pad);
            }
            let (w, h) = (pad.size.0 / 2.0 + ring.margin, pad.size.1 / 2.0 + ring.margin);
            let poly = [(-w, -h), (w, -h), (w, h), (-w, h)]
                .iter()
                .map(|&(x, y)| pad.pos + crate::geom::rotate(pad.offset + crate::geom::pt(x, y), pad.angle))
                .collect();
            let layers = pad.layers.iter().filter(|l| l.ends_with(".Cu")).cloned().collect();
            out.push(crate::board::RuleArea {
                name: format!("pad ring {} {}{}", ring.reference, ring.pad, if k == 0 { String::new() } else { format!(" {k}") }),
                layers,
                poly,
                no_tracks: true,
                no_vias: true,
            });
        }
    }
    Ok(out)
}

/// Export DSN, run Freerouting on several footprint orders, then finish (SES tracks,
/// fill, stitch, fill) and DRC-check the best few and keep the best that passes. If none
/// passes, escalate (`next_step`); if still none, write the failure report (D-020).
pub fn route(pcb: &Path, opts: &RouteOptions, waivers: &[Waiver]) -> Result<()> {
    let fr = freerouting();
    if !fr.exists() {
        bail!("{} missing; run scripts/fetch-tools.sh", fr.display());
    }
    let work = pcb.parent().unwrap().join("route");
    std::fs::create_dir_all(&work)?;
    // earlier runs' orders and report, so route/ only holds this run's
    for e in std::fs::read_dir(&work)?.flatten() {
        let n = e.file_name().to_string_lossy().into_owned();
        if n.starts_with("try-") {
            std::fs::remove_dir_all(e.path())?;
        } else if n == "failure.json" {
            std::fs::remove_file(e.path())?;
        }
    }

    // drop earlier routing so re-runs start clean; locked tracks (escape stubs) stay
    let mut root = load(pcb)?;
    root.items_mut().retain(|c| !((c.is("segment") || c.is("arc") || c.is("via")) && !c.flag("locked")));
    save(pcb, &root)?;

    let mut board = Board::from_sexp(&root)?;
    let origin = board.edge_bbox().0;
    let rings = pad_rings(&board, opts)?;
    if !rings.is_empty() {
        println!("freerouting: {} pad ring(s) keep tracks and vias off: {}", rings.len(), rings.iter().map(|r| r.name.as_str()).collect::<Vec<_>>().join(", "));
    }
    board.keepouts.extend(rings);
    let router = Router {
        fr,
        board,
        rules: project::read_classes(&pcb.with_extension("kicad_pro"))?,
        hole_clearance: hole_clearance(&pcb.with_extension("kicad_pro")),
        opts,
        work: work.clone(),
    };

    let t0 = Instant::now();
    let first: Vec<u64> = (0..opts.tries.max(1) as u64).collect();
    let mut next_seed = first.len() as u64;
    let mut tries = router.run(&first);
    if tries.is_empty() {
        bail!("every Freerouting run failed; see {}/try-*/freerouting.log", work.display());
    }

    // Finish the best few (tracks, pours, stitching) each in its own folder and run KiCad's
    // DRC on them: the router's numbers can't see e.g. a track starving a pour's thermal
    // spokes. Keep the best-ranked one DRC passes, else the one with the fewest open items.
    let mut steps: Vec<String> = vec![];
    let mut logs: Vec<(u64, Vec<String>)> = vec![];
    let n = opts.drc_checks.min(tries.len());
    let kept = if n == 0 {
        let k = rank(&tries)[0];
        finish(pcb, &root, &tries[k], opts)?.iter().for_each(|l| println!("{l}"));
        k
    } else {
        println!("drc: finishing the best {n} orders and checking each");
        let ranked = rank(&tries);
        check(pcb, &root, &mut tries, &ranked[..n], opts, waivers, &mut logs);
        steps.push(format!("checked the best {n} of {} routed orders", tries.len()));
        let mut rounds = 0;
        loop {
            let unchecked: Vec<usize> = rank(&tries).into_iter().filter(|&i| !tries[i].checked).collect();
            let best_open = best(&tries).and_then(|i| tries[i].drc.as_ref().map(Vec::len));
            match next_step(best_open, unchecked.len(), rounds, opts) {
                Step::Stop => break,
                Step::CheckRest => {
                    println!("drc: no checked order is clean; checking the other {} routed orders", unchecked.len());
                    check(pcb, &root, &mut tries, &unchecked, opts, waivers, &mut logs);
                    steps.push(format!("checked the other {} routed orders", unchecked.len()));
                }
                Step::RouteMore => {
                    rounds += 1;
                    let seeds: Vec<u64> = (next_seed..next_seed + opts.extra_tries as u64).collect();
                    next_seed += seeds.len() as u64;
                    println!("freerouting: still no clean order; extra round {rounds} of {}: orders {}-{}", opts.extra_rounds, seeds[0], seeds[seeds.len() - 1]);
                    let before = tries.len();
                    tries.extend(router.run(&seeds));
                    steps.push(format!("routed {} more orders ({}-{}), {} finished", seeds.len(), seeds[0], seeds[seeds.len() - 1], tries.len() - before));
                }
            }
        }
        let Some(k) = best(&tries) else { bail!("no checked order could be finished") };
        let log = logs.iter().position(|l| l.0 == tries[k].seed).map(|p| logs.swap_remove(p).1).unwrap_or_default();
        for l in log {
            println!("{l}");
        }
        let open = tries[k].drc.as_ref().map_or(0, Vec::len);
        if open > 0 {
            report_failure(pcb, origin, &tries, k, &steps)?;
            println!(
                "drc: WARNING: every checked order has open DRC items; the check stage will fail. See {} and the fixes above",
                work.join("failure.json").display()
            );
        }
        std::fs::copy(tries[k].dir.join("check").join(pcb.file_name().unwrap()), pcb)?;
        k
    };
    let best = &tries[kept];
    println!("freerouting: kept order {} of {} ({:.0} s in all): {}", best.seed, tries.len(), t0.elapsed().as_secs_f64(), best.summary());
    if !best.unrouted.is_empty() {
        println!(
            "freerouting: WARNING: the kept order left connections unrouted; the check stage will fail. Route more orders \
             (--tries), or give the router room (placement, net-class widths)"
        );
    }
    // the kept order's files where a reader expects them
    for f in ["board.dsn", "board.ses", "freerouting.log"] {
        std::fs::copy(best.dir.join(f), work.join(f))?;
    }
    let summary = serde_json::json!({
        "kept_order": best.seed,
        "score": "fewest unrouted (other nets, then the pour net), then fewest mm thinner than the net class, then fewest vias, then shortest; of the best `drc_checks` orders, finished and checked, the first with no open DRC item, else the fewest; if none is clean, the other routed orders are checked, then `extra_rounds` of `extra_tries` new orders (D-020)",
        "steps": steps,
        "orders": tries.iter().map(Try::json).collect::<Vec<_>>(),
    });
    std::fs::write(work.join("tries.json"), serde_json::to_string_pretty(&summary)? + "\n")?;
    Ok(())
}

/// Finish and DRC-check these orders (all at once, up to `max(parallel, drc_checks)`),
/// recording their open items; the finishing logs go to `logs`.
fn check(pcb: &Path, root: &Sexp, tries: &mut [Try], which: &[usize], opts: &RouteOptions, waivers: &[Waiver], logs: &mut Vec<(u64, Vec<String>)>) {
    let results = {
        let tries = &*tries;
        pool(which, opts.parallel.max(opts.drc_checks), |&i| check_try(pcb, root, &tries[i], opts, waivers))
    };
    for (&i, r) in which.iter().zip(results) {
        let t = &mut tries[i];
        t.checked = true;
        match r {
            Ok((open, log)) => {
                println!("drc: order {}: {} open item(s){}", t.seed, open.len(), open.first().map_or(String::new(), |o| format!(", e.g. {}", crate::gates::describe(o))));
                t.drc = Some(open);
                logs.push((t.seed, log));
            }
            Err(e) => println!("drc: order {}: check failed: {e:#}", t.seed),
        }
    }
}

/// No checked order is clean: write `route/failure.json`, print it, and log the failed
/// orders in `<board dir>/route-failures.jsonl`.
fn report_failure(pcb: &Path, origin: crate::geom::Pt, tries: &[Try], kept: usize, steps: &[String]) -> Result<()> {
    let name = board_name(pcb);
    let date = crate::schematic::today();
    let checked: Vec<(u64, Vec<serde_json::Value>)> =
        rank(tries).into_iter().filter_map(|i| tries[i].drc.clone().map(|d| (tries[i].seed, d))).collect();
    let report = failure::report(&name, &date, origin, &checked, tries[kept].seed, steps);
    let work = pcb.parent().unwrap().join("route");
    std::fs::write(work.join("failure.json"), serde_json::to_string_pretty(&report)? + "\n")?;
    for l in failure::summary(&report) {
        println!("{l}");
    }
    // the placement and rules, as the router saw them (the DSN of order 0, without its path)
    let dsn = std::fs::read_to_string(work.join("try-0/board.dsn")).unwrap_or_default();
    let layout = crate::dsn::fnv(dsn.split_once('\n').map_or("", |d| d.1));
    let log = pcb.parent().unwrap().parent().unwrap().join("route-failures.jsonl");
    let added = failure::append_log(&log, &failure::log_lines(&name, &date, &layout, &checked))?;
    println!("route failure: {added} failed order(s) added to {}", log.display());
    for l in failure::log_summary(&log) {
        println!("{l}");
    }
    Ok(())
}

/// Of checked orders in rank order, given their open DRC item counts: the first with
/// none, else the first with the fewest.
fn pick(open: &[usize]) -> Option<usize> {
    (0..open.len()).min_by_key(|&k| (open[k], k))
}

/// A checked order: (open DRC items, the finishing log).
type Checked = (Vec<serde_json::Value>, Vec<String>);

/// Finish one order's routing in `<try>/check/` (a copy of the project) and run the DRC
/// gate there: (open items, the finishing log).
fn check_try(pcb: &Path, root: &Sexp, t: &Try, opts: &RouteOptions, waivers: &[Waiver]) -> Result<Checked> {
    let dir = t.dir.join("check");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir)?;
    let src = pcb.parent().unwrap();
    let name = board_name(pcb);
    // what DRC reads besides the board: rules, net classes, the schematic (parity) and the
    // library tables (their paths are absolute)
    for f in [format!("{name}.kicad_sch"), format!("{name}.kicad_pro"), format!("{name}.kicad_dru"), "fp-lib-table".into(), "sym-lib-table".into()] {
        if src.join(&f).exists() {
            std::fs::copy(src.join(&f), dir.join(&f))?;
        }
    }
    let board = dir.join(pcb.file_name().unwrap());
    let log = finish(&board, root, t, opts)?;
    Ok((crate::gates::drc_open(&dir, &name, waivers)?, log))
}

/// Put one order's tracks and vias on `root` (the board with only its locked tracks), save
/// it at `pcb`, fill, stitch, fill. Returns the log lines.
fn finish(pcb: &Path, root: &Sexp, t: &Try, opts: &RouteOptions) -> Result<Vec<String>> {
    let name = board_name(pcb);
    let mut root = root.clone();
    let mut log = vec![format!("routed: {} track segments, {} vias", t.tracks.len(), t.vias.len())];
    for (i, tr) in t.tracks.iter().enumerate() {
        root.push(node!(
            "segment",
            node!("start", tr.start.x, tr.start.y),
            node!("end", tr.end.x, tr.end.y),
            node!("width", tr.width),
            node!("layer", &tr.layer),
            node!("net", &tr.net),
            node!("uuid", uid(&name, &["track", &i.to_string()]))
        ));
    }
    for (i, v) in t.vias.iter().enumerate() {
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
            let (added, notes) = stitch::stitch(&board, opts, &net, pitch);
            log.extend(notes);
            log.push(format!("stitching: {} {net} vias", added.len()));
            for (i, p) in added.into_iter().enumerate() {
                let (d, h) = opts.stitch_via;
                root.push(via_node(&name, &format!("stitch/{net}/{i}"), p, d, h, &net));
                board.vias.push(crate::board::Via { at: p, size: d, drill: h, net: net.clone(), locked: false });
            }
        }
        save(pcb, &root)?;
        fill_zones(pcb)?;
    }
    Ok(log)
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
    /// Whether the route stage tried to finish and DRC-check this order.
    pub checked: bool,
    /// Its open DRC items once finished, if its check ran.
    pub drc: Option<Vec<serde_json::Value>>,
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
            "drc_open": self.drc.as_ref().map(Vec::len),
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
        checked: false,
        drc: None,
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
            checked: false,
            drc: None,
        }
    }

    #[test]
    fn picks_first_clean_order() {
        assert_eq!(pick(&[2, 0, 0]), Some(1));
        assert_eq!(pick(&[0, 0]), Some(0));
        assert_eq!(pick(&[3, 1, 1]), Some(1));
        assert_eq!(pick(&[]), None);
    }

    /// The escalation ladder (D-020): stop on a clean order; else check what's routed,
    /// then route extra rounds, then stop.
    #[test]
    fn ladder_cheapest_first() {
        let o = RouteOptions::default(); // 1 extra round of 8
        assert_eq!(next_step(Some(0), 5, 0, &o), Step::Stop);
        assert_eq!(next_step(Some(2), 5, 0, &o), Step::CheckRest);
        assert_eq!(next_step(None, 5, 0, &o), Step::CheckRest); // no check finished yet
        assert_eq!(next_step(Some(2), 0, 0, &o), Step::RouteMore);
        assert_eq!(next_step(Some(2), 8, 1, &o), Step::CheckRest); // the new round's orders
        assert_eq!(next_step(Some(2), 0, 1, &o), Step::Stop);
        let none = RouteOptions { extra_rounds: 0, ..RouteOptions::default() };
        assert_eq!(next_step(Some(2), 0, 0, &none), Step::Stop);
        let off = RouteOptions { drc_checks: 0, ..RouteOptions::default() };
        assert_eq!(next_step(Some(2), 5, 0, &off), Step::Stop);
    }

    /// The kept order among checked ones follows the global rank, not the check order.
    #[test]
    fn best_checked_order() {
        let open = |n: usize| Some(vec![serde_json::json!({}); n]);
        let mut v = vec![t(0, 0, 0, 0.0, 20, 100.0), t(1, 0, 0, 0.0, 10, 100.0), t(2, 0, 0, 0.0, 30, 100.0), t(3, 0, 0, 0.0, 5, 100.0)];
        assert_eq!(best(&v), None);
        (v[1].drc, v[0].drc) = (open(2), open(2));
        assert_eq!(best(&v), Some(1)); // ranked above order 0 (fewer vias)
        v[2].drc = open(0);
        assert_eq!(best(&v), Some(2));
        v[3].drc = open(0); // order 3 ranks first of all
        assert_eq!(best(&v), Some(3));
        assert_eq!(pool(&[3, 1, 2], 2, |x| x * 10), [30, 10, 20]);
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
