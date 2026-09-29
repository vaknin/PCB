//! Autorouting with Freerouting via Specctra DSN/SES, written and read here.
//!
//! Freerouting 2.4.1's Linux bundle carries its own Java 25 runtime, so nothing is
//! installed system-wide. Fetch it with scripts/fetch-tools.sh.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
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
    let (dsn_path, ses_path, log) = (work.join("board.dsn"), work.join("board.ses"), work.join("freerouting.log"));
    let name = board_name(pcb);

    // drop earlier routing so re-runs start clean; locked tracks (escape stubs) stay
    let mut root = load(pcb)?;
    root.items_mut().retain(|c| !((c.is("segment") || c.is("arc") || c.is("via")) && !c.flag("locked")));
    save(pcb, &root)?;

    let rules = project::read_classes(&pcb.with_extension("kicad_pro"))?;
    let board = Board::from_sexp(&root)?;
    let hole_clearance = hole_clearance(&pcb.with_extension("kicad_pro"));

    let args: Vec<String> = vec![
        "-de".into(),
        dsn_path.to_string_lossy().into(),
        "-do".into(),
        ses_path.to_string_lossy().into(),
        "-mp".into(),
        opts.max_passes.to_string(),
        "--gui.enabled=false".into(),
        format!("--router.copperToEdgeClearanceUm={}", opts.edge_clearance_um),
        format!("--router.via_costs={}", opts.via_costs),
        "--usage_and_diagnostic_data.disable_analytics=true".into(),
        format!("--user_data_path={}", work.join("fr-data").display()),
    ];
    let mut vias = Default::default();
    for attempt in 1..=opts.tries {
        // Freerouting is deterministic, so a retry gets the footprints in another order
        vias = dsn::write(&board, &rules, hole_clearance, (attempt - 1) as u64, &dsn_path)?;
        let _ = std::fs::remove_file(&ses_path);
        let t0 = Instant::now();
        let fh = std::fs::File::create(&log)?;
        let mut child = Command::new(&fr).args(&args).stdout(fh.try_clone()?).stderr(fh).stdin(Stdio::null()).spawn()?;
        let status = loop {
            if let Some(s) = child.try_wait()? {
                break s;
            }
            if t0.elapsed() > Duration::from_secs(opts.timeout_s) {
                child.kill()?;
                bail!("Freerouting took over {} s; see {}", opts.timeout_s, log.display());
            }
            std::thread::sleep(Duration::from_millis(200));
        };
        if !ses_path.exists() {
            bail!("Freerouting produced no session file; see {}", log.display());
        }
        let unrouted = unrouted(&log, opts.stitch_net.as_deref())?;
        println!(
            "freerouting: try {attempt}, exit {} in {:.0}s, unrouted: {} (excluding the pour net), log {}",
            status.code().map_or("signal".into(), |c| c.to_string()),
            t0.elapsed().as_secs_f64(),
            if unrouted.is_empty() { "none".into() } else { unrouted.join(", ") },
            log.display()
        );
        if unrouted.is_empty() {
            break;
        }
    }

    let (tracks, routed_vias) = ses::read(&ses_path, &vias)?;
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

/// Nets Freerouting left unrouted, except the pour net (its pour and stitching vias
/// finish those; DRC checks). ["?"] if the log has no final score.
fn unrouted(log: &Path, pour_net: Option<&str>) -> Result<Vec<String>> {
    let text = std::fs::read_to_string(log)?;
    if !text.contains("final score:") {
        return Ok(vec!["?".into()]);
    }
    let re = Regex::new(r"Net '(.+)' \(\d+ unrouted")?;
    Ok(re.captures_iter(&text).map(|c| c[1].to_string()).filter(|n| Some(n.as_str()) != pour_net).collect())
}
