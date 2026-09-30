//! The review page for one design round (D-022 step 4, D-023): one self-contained HTML
//! file, `<board>/review/index.html` (gitignored), for the owner. Plain language, no
//! schematic. It reads what the other stages left behind and never re-runs them:
//! `kicad/reports/{gates,erc,drc,routing}.json`, `fab/<name>-bom.csv`, `fab/cost.json`,
//! `fab/README.md`, the board's `board.toml`, `spec.md` and `round.md` (Claude's notes for
//! the round: what changed and why, open risks), and git for the changes since the last
//! `<name>-draft-*` tag. Missing inputs show as "not run", never as a pass.
//!
//! The file is written as an Artifact page body (no <html>/<head>; the publisher wraps it).

use std::fmt::Write as _;
use std::path::Path;
use std::process::Command;

use anyhow::Result;
use regex::Regex;
use serde_json::Value;

use crate::boardfile::{BoardFile, GATES};
use crate::circuit::Circuit;
use crate::cost;
use crate::gates::{classify, violations};
use crate::layout::Waiver;

pub struct Inputs<'a> {
    /// The board's crate directory (board.toml, spec.md, round.md, git).
    pub dir: &'a Path,
    /// Where kicad/ and fab/ are (the board directory, or --out).
    pub base: &'a Path,
    pub circuit: &'a Circuit,
    pub board_file: Option<&'a BoardFile>,
    pub waivers: &'a [Waiver],
}

/// Plain-language names of the gates.
fn gate_label(g: &str) -> &'static str {
    match g {
        "netlist" => "The drawing matches the circuit code",
        "netclasses" => "Track-width rules reach the right connections",
        "board_toml" => "Pins, power budget and requirements agree",
        "erc" => "Electrical rules (ERC)",
        "drc" => "JLCPCB's manufacturing rules (DRC)",
        "routing" => "Every connection is routed",
        _ => "unknown check",
    }
}

/// The firmware's simulation results (`firmware/sim.json`, written by the `sim` stage, D-025):
/// one table per simulator, with each self-test's result in plain words.
fn firmware_section(path: &Path, risks: &mut Vec<String>) -> String {
    let Some(sim) = std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()) else {
        risks.push("The firmware hasn't been run in simulation (the <code>sim</code> stage).".into());
        return "<p>Not run yet.</p>".into();
    };
    let kinds = [
        ("qemu", "The whole firmware on a simulated chip (QEMU: free, no limit). It has no pins, so pin tests skip here."),
        ("wokwi", "Buttons and lights on simulated pins (Wokwi: 50 free minutes a month)."),
    ];
    let mut out = String::new();
    for (key, what) in kinds {
        let r = &sim[key];
        if r.is_null() {
            out += &format!("<h3>{}</h3>\n<p class=\"muted\">{what} Not run yet.</p>\n", if key == "qemu" { "QEMU" } else { "Wokwi" });
            continue;
        }
        let ok = r["ok"] == true;
        let date = r["date"].as_str().or(sim["date"].as_str()).unwrap_or("?");
        let rows = test_rows(r);
        let problems: Vec<String> = r["problems"].as_array().into_iter().flatten().filter_map(|p| p.as_str()).map(|p| format!("<li>{}</li>", esc(p))).collect();
        let name = if key == "qemu" { "QEMU" } else { "Wokwi" };
        if !ok {
            risks.push(format!("The firmware fails in {name} (see Firmware in simulation)."));
        }
        let extra = match r["sim_seconds"].as_f64() {
            Some(s) => format!(" Used about {s:.1} s of the month's Wokwi time."),
            None => String::new(),
        };
        out += &format!(
            "<h3>{name} {}</h3>\n<p class=\"muted\">{what} Run {}.{extra}</p>\n{}\
             <div class=\"scroll\"><table><thead><tr><th>Test</th><th>Result</th><th>Detail</th></tr></thead><tbody>\n{rows}</tbody></table></div>\n",
            chip(if ok { "ok" } else { "bad" }, if ok { "Pass" } else { "Fail" }),
            esc(date),
            if problems.is_empty() { String::new() } else { format!("<ul class=\"risks\">{}</ul>\n", problems.join("")) },
        );
    }
    out
}

/// One table row per self-test in a `SELFTEST` report (sim.json's runs, devctl's bring-up files).
fn test_rows(r: &Value) -> String {
    let mut rows = String::new();
    for t in r["tests"].as_array().into_iter().flatten() {
        let (state, word) = match t["result"].as_str().unwrap_or("?") {
            "pass" => ("ok", "Pass"),
            "skip" => ("warn", "Skipped"),
            _ => ("bad", "Fail"),
        };
        rows += &format!(
            "<tr><td><code>{}</code></td><td>{}</td><td>{}</td></tr>\n",
            esc(t["test"].as_str().unwrap_or("?")),
            chip(state, word),
            esc(t["detail"].as_str().unwrap_or(""))
        );
    }
    rows
}

/// The newest real-board self-test (`bringup/selftest-<date>.json`, written by `devctl selftest`):
/// "not run yet" until a board exists.
fn bringup_section(dir: &Path, risks: &mut Vec<String>) -> String {
    let newest = std::fs::read_dir(dir.join("bringup"))
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|n| n.starts_with("selftest-") && n.ends_with(".json"))
        .max();
    let Some(r) = newest.as_ref().and_then(|n| read_json(&dir.join("bringup").join(n))) else {
        return "<p class=\"muted\">Not run yet: this runs once a built board is plugged in (<code>devctl selftest</code>).</p>".into();
    };
    let ok = r["ok"] == true;
    if !ok {
        risks.push("The built board fails its self-test (see Bring-up).".into());
    }
    let mut notes: Vec<String> = r["problems"].as_array().into_iter().flatten().filter_map(Value::as_str).map(|p| format!("<li>{}</li>", esc(p))).collect();
    for l in r["needs_person"].as_array().into_iter().flatten() {
        notes.push(format!(
            "<li>Needs a look: <code>{}</code> should show {}.</li>",
            esc(l["test"].as_str().unwrap_or("?")),
            esc(l["look"].as_str().unwrap_or("?"))
        ));
    }
    format!(
        "<p>{} Run {} on <code>{}</code>.</p>\n{}<div class=\"scroll\"><table><thead><tr><th>Test</th><th>Result</th><th>Detail</th></tr></thead><tbody>\n{}</tbody></table></div>\n",
        chip(if ok { "ok" } else { "bad" }, if ok { "Pass" } else { "Fail" }),
        esc(r["date"].as_str().unwrap_or("?")),
        esc(r["port"].as_str().unwrap_or("?")),
        if notes.is_empty() { String::new() } else { format!("<ul class=\"risks\">{}</ul>\n", notes.join("")) },
        test_rows(&r)
    )
}

/// The power budgets from `board.toml [power]` (D-025 C.2): a table per source, and what the
/// board draws asleep. Returns the section, the summary chip's text and its state.
fn power_section(bf: &BoardFile, risks: &mut Vec<String>) -> (String, String, &'static str) {
    let guess = |src: &str| if src.contains("INFERRED") { chip("warn", "estimate") + " " } else { String::new() };
    let (mut out, mut heads, mut state) = (String::new(), vec![], "ok");
    let mut guesses = 0;
    for t in bf.power.tally() {
        let s = t.source;
        let mut rows = String::new();
        for l in &t.loads {
            let _ = writeln!(rows, "<tr><td>{}</td><td class=\"num\">{}</td><td class=\"muted\">{}{}</td></tr>", esc(&l.name), l.ma, guess(&l.source), esc(&l.source));
        }
        let limits: Vec<String> = s.limits.iter().map(|l| format!("{} {} mA <span class=\"muted\">({})</span>", esc(&l.name), l.ma, esc(&l.source))).collect();
        guesses += t.loads.iter().filter(|l| l.source.contains("INFERRED")).count() + s.limits.iter().filter(|l| l.source.contains("INFERRED")).count();
        let spare = t.budget_ma - t.ma;
        let _ = writeln!(
            rows,
            "<tr class=\"sum\"><td>Total, worst case</td><td class=\"num\">{:.1}</td><td>of {} mA: {}</td></tr>",
            t.ma,
            t.budget_ma,
            if t.over() { format!("{:.1} mA over", -spare) } else { format!("{spare:.1} mA to spare") }
        );
        if t.over() {
            state = "bad";
            risks.push(format!("Power from {}: {:.1} mA drawn, over its {} mA budget.", esc(&s.name), t.ma, t.budget_ma));
        }
        let mut head = format!("{} {:.0} of {} mA", s.name, t.ma, t.budget_ma);
        let _ = write!(
            out,
            "<h3>From {} {}</h3>\n<p class=\"muted\">Limited by: {}. The budget is the smallest.</p>\n\
             <div class=\"scroll\"><table><thead><tr><th>Load</th><th class=\"num\">mA</th><th>Source</th></tr></thead><tbody>\n{rows}</tbody></table></div>\n",
            esc(&s.what),
            chip(if t.over() { "bad" } else { "ok" }, if t.over() { "Over budget" } else { "Fits" }),
            limits.join("; ")
        );
        if let Some(b) = s.sleep_ua {
            let mut rows = String::new();
            for l in &t.sleep_loads {
                let _ = writeln!(rows, "<tr><td>{}</td><td class=\"num\">{}</td><td class=\"muted\">{}{}</td></tr>", esc(&l.name), l.ua, guess(&l.source), esc(&l.source));
            }
            let est = t.sleep_loads.iter().filter(|l| l.source.contains("INFERRED")).count();
            guesses += est;
            let _ = writeln!(rows, "<tr class=\"sum\"><td>Total asleep</td><td class=\"num\">{:.1}</td><td>of {b} µA</td></tr>", t.sleep_ua);
            if t.sleep_over() {
                state = "bad";
                risks.push(format!("Asleep on {}: {:.1} µA, over the {b} µA budget: the battery won't last as long as the spec says.", esc(&s.name), t.sleep_ua));
            }
            let _ = write!(
                out,
                "<h4>Asleep on {} {}</h4>\n<p class=\"muted\">Everything that still draws current while the board sleeps; this sets how long the battery lasts.{}</p>\n\
                 <div class=\"scroll\"><table><thead><tr><th>Part</th><th class=\"num\">µA</th><th>Source</th></tr></thead><tbody>\n{rows}</tbody></table></div>\n",
                esc(&s.name),
                sleep_chip(&t),
                if est > 0 { format!(" {est} of these figures are estimates; the real number is measured on the first board.") } else { String::new() }
            );
            head += &format!(", asleep {:.1} of {b} µA", t.sleep_ua);
        }
        heads.push(head);
    }
    if guesses > 0 {
        risks.push(format!("{guesses} of the power figures are estimates, not datasheet values."));
    }
    (out, heads.join("; "), state)
}

/// The sleep total against its budget, as a chip: amber while any figure is an estimate.
fn sleep_chip(t: &crate::boardfile::Tally) -> String {
    let b = t.source.sleep_ua.unwrap_or(f64::NAN);
    let estimated = t.sleep_loads.iter().any(|l| l.source.contains("INFERRED"));
    let state = if t.sleep_over() { "bad" } else if estimated { "warn" } else { "ok" };
    chip(state, &format!("Asleep: {:.1} of {b} µA{}", t.sleep_ua, if estimated { ", estimated" } else { "" }))
}

/// Days before a secret's expiry that the page starts warning (renewing means re-provisioning
/// every device that shares it).
const EXPIRY_WARN_DAYS: i64 = 60;

/// Plain words for a case opening's kind (board.toml `[[case.opening]] kind`).
fn opening_label(kind: &str) -> &str {
    match kind {
        "usb_c" => "USB-C socket",
        "connector" => "connector",
        "button" => "button cap",
        "pinhole" => "pinhole (paper clip)",
        "led" => "light window",
        "vent" => "air vent",
        "mic" => "microphone hole",
        _ => kind,
    }
}

/// The printed case (`case/fit.json` and its renders, written by the `case` stage, D-025 B.5):
/// the pictures, the fit check in words, each opening, and the parts to print.
fn case_section(dir: &Path, pcb: &Path, risks: &mut Vec<String>) -> String {
    let Some(fit) = read_json(&dir.join("fit.json")) else {
        risks.push("The case hasn't been designed or fit-checked (the <code>case</code> stage).".into());
        return "<p>Not run yet.</p>".into();
    };
    let ok = fit["ok"] == true;
    let stale = match (std::fs::metadata(dir.join("fit.json")), std::fs::metadata(pcb)) {
        (Ok(f), Ok(p)) => p.modified().ok() > f.modified().ok(),
        _ => false,
    };
    if !ok {
        risks.push("The case fails its fit check (see The case).".into());
    }
    if stale {
        risks.push("The board changed after the case was last checked; re-run <code>case</code>.".into());
    }
    for c in fit["checks"].as_array().into_iter().flatten().filter(|c| c["ok"] != true) {
        risks.push(format!("Case check <code>{}</code> fails: {}", esc(c["name"].as_str().unwrap_or("?")), esc(c["detail"].as_str().unwrap_or(""))));
    }
    for p in fit["problems"].as_array().into_iter().flatten().filter_map(Value::as_str) {
        risks.push(format!("Case: {}", esc(p)));
    }

    let captions = [("case-iso.png", "The case, closed"), ("case-exploded.png", "Opened up, with the board inside"), ("case-top.png", "From above")];
    let pics: String = fit["renders"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(|f| {
            let what = captions.iter().find(|c| c.0 == f).map_or(f, |c| c.1);
            match std::fs::read(dir.join(f)) {
                Ok(png) => format!("<figure><img src=\"data:image/png;base64,{}\" alt=\"{}\"><figcaption>{}</figcaption></figure>", base64(&png), esc(what), esc(what)),
                Err(_) => format!("<figure class=\"none\"><figcaption>{}: picture missing</figcaption></figure>", esc(what)),
            }
        })
        .collect();

    let checks = fit["checks"].as_array().map_or(&[][..], |a| a);
    let passed = checks.iter().filter(|c| c["ok"] == true).count();
    let clear: Vec<String> = fit["clearance"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|c| format!("{} {:.2} mm (nearest {})", esc(c["part"].as_str().unwrap_or("?")), c["min_mm"].as_f64().unwrap_or(f64::NAN), esc(c["nearest"].as_str().unwrap_or("?"))))
        .collect();
    let mut openings = String::new();
    for o in fit["openings"].as_array().into_iter().flatten() {
        let _ = writeln!(
            openings,
            "<tr><td><code>{}</code></td><td>{}</td><td>{}</td><td class=\"muted\">{}</td></tr>",
            esc(o["ref"].as_str().unwrap_or("?")),
            esc(opening_label(o["kind"].as_str().unwrap_or("?"))),
            if o["ok"] == true { chip("ok", "Lines up") } else { chip("bad", "Off") },
            esc(o["detail"].as_str().unwrap_or(""))
        );
    }
    let mut prints = String::new();
    for p in fit["parts"].as_array().into_iter().flatten() {
        let s: Vec<String> = p["size_mm"].as_array().into_iter().flatten().filter_map(Value::as_f64).map(|v| format!("{v:.1}")).collect();
        let _ = writeln!(
            prints,
            "<tr><td>{}</td><td class=\"num\">{} mm</td><td class=\"num\">{:.1} cm³</td></tr>",
            esc(p["name"].as_str().unwrap_or("?")),
            s.join(" × "),
            p["volume_mm3"].as_f64().unwrap_or(0.0) / 1000.0
        );
    }
    let sc = &fit["screw"];
    let screw = if sc.is_object() {
        format!(" Closed with {} × {} × {} mm self-tapping screws.", sc["count"], esc(sc["size"].as_str().unwrap_or("?")), sc["length_mm"])
    } else {
        String::new()
    };
    let state = match (ok, stale) {
        (false, _) => chip("bad", "Fit check fails"),
        (true, true) => chip("warn", "Passed, but the board changed since"),
        (true, false) => chip("ok", "Fits"),
    };
    format!(
        "<p>{state} {passed} of {} checks pass, run {}. Printed in {}.{screw}</p>\n\
         <p class=\"muted\">The fit check loads the board's 3D model and the case and measures them: nothing overlaps, every part keeps its gap \
         (smallest: {}), each opening sits over its part, and the button cap reaches its switch.</p>\n\
         <section class=\"pics\">{pics}</section>\n\
         <h3>Openings</h3>\n<div class=\"scroll\"><table><thead><tr><th>Part</th><th>Opening</th><th>Fit</th><th>Detail</th></tr></thead><tbody>\n{openings}</tbody></table></div>\n\
         <h3>Pieces to print</h3>\n<div class=\"scroll\"><table><thead><tr><th>Piece</th><th class=\"num\">Size</th><th class=\"num\">Material used</th></tr></thead><tbody>\n{prints}</tbody></table></div>\n",
        checks.len(),
        esc(fit["date"].as_str().unwrap_or("?")),
        esc(fit["material"].as_str().unwrap_or("?")),
        clear.join(", "),
    )
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn read_json(p: &Path) -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(p).ok()?).ok()
}

fn git(dir: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git").arg("-C").arg(dir).args(args).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim_end().to_string())
}

fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            s.push(if i <= c.len() { T[(n >> (18 - 6 * i) & 63) as usize] as char } else { '=' });
        }
    }
    s
}

/// One side of the board as an SVG data URI (kicad-cli), or None if it can't be drawn.
fn render(pcb: &Path, layers: &str, mirror: bool) -> Option<String> {
    let tmp = std::env::temp_dir().join(format!("pcbgen-review-{}-{mirror}.svg", std::process::id()));
    let (pcb_s, tmp_s) = (pcb.to_string_lossy(), tmp.to_string_lossy());
    let mut args = vec!["pcb", "export", "svg", "--mode-single", "--fit-page-to-board", "--exclude-drawing-sheet", "-l", layers];
    if mirror {
        args.push("--mirror");
    }
    args.extend(["-o", &tmp_s, &pcb_s]);
    crate::kicad_cli(&args).ok()?;
    let svg = std::fs::read(&tmp).ok()?;
    let _ = std::fs::remove_file(&tmp);
    Some(format!("data:image/svg+xml;base64,{}", base64(&svg)))
}

/// Just enough Markdown for round.md: headings, list items (one level of nesting),
/// paragraphs, `code` and **bold**. Everything is escaped first.
pub fn markdown(md: &str) -> String {
    let code = Regex::new(r"`([^`]+)`").unwrap();
    let bold = Regex::new(r"\*\*([^*]+)\*\*").unwrap();
    let inline = |s: &str| bold.replace_all(&code.replace_all(&esc(s), "<code>$1</code>"), "<strong>$1</strong>").into_owned();
    let (mut html, mut para, mut depth) = (String::new(), Vec::<String>::new(), 0usize);
    let flush = |html: &mut String, para: &mut Vec<String>| {
        if !para.is_empty() {
            *html += &format!("<p>{}</p>\n", inline(&para.join(" ")));
            para.clear();
        }
    };
    let close = |html: &mut String, depth: &mut usize, to: usize| {
        while *depth > to {
            *html += "</ul>\n";
            *depth -= 1;
        }
    };
    for line in md.lines() {
        let t = line.trim_start();
        let indent = line.len() - t.len();
        if let Some(item) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")) {
            flush(&mut html, &mut para);
            let want = if indent >= 2 { 2 } else { 1 };
            close(&mut html, &mut depth, want);
            while depth < want {
                html += "<ul>\n";
                depth += 1;
            }
            html += &format!("<li>{}</li>\n", inline(item));
            continue;
        }
        close(&mut html, &mut depth, 0);
        if t.is_empty() {
            flush(&mut html, &mut para);
        } else if let Some(h) = t.strip_prefix('#') {
            flush(&mut html, &mut para);
            let level = 1 + h.chars().take_while(|&c| c == '#').count();
            let text = h.trim_start_matches('#').trim();
            html += &format!("<h{0}>{1}</h{0}>\n", (level + 2).min(6), inline(text));
        } else {
            para.push(t.to_string());
        }
    }
    flush(&mut html, &mut para);
    close(&mut html, &mut depth, 0);
    html
}

/// Bullets under a `## <heading>` of fab/README.md.
fn readme_list(readme: &str, heading: &str) -> Vec<String> {
    let mut on = false;
    let mut out = vec![];
    for line in readme.lines() {
        if line.starts_with("## ") {
            on = line.starts_with(heading);
        } else if on && let Some(item) = line.strip_prefix("- ") {
            out.push(item.to_string());
        }
    }
    out
}

fn money(v: &Value) -> String {
    v.as_f64().map_or("?".into(), |x| format!("${x:.2}"))
}

fn chip(state: &str, text: &str) -> String {
    format!("<span class=\"chip {state}\">{}</span>", esc(text))
}

/// Builds the page and writes `<base>/review/index.html`.
pub fn write(i: &Inputs) -> Result<std::path::PathBuf> {
    let c = i.circuit;
    let name = &c.name;
    let kicad = i.base.join("kicad");
    let reports = kicad.join("reports");
    let fab = i.base.join("fab");
    let pcb = kicad.join(format!("{name}.kicad_pcb"));
    let bf = i.board_file;
    let mut risks: Vec<String> = vec![]; // escaped HTML, one item each

    // --- git: where this round stands -----------------------------------------------
    let head = git(i.dir, &["rev-parse", "--short", "HEAD"]).unwrap_or_else(|| "?".into());
    let dirty = git(i.dir, &["status", "--porcelain", "--", ".", ":!review"]).is_some_and(|s| !s.is_empty());
    let at_head = git(i.dir, &["tag", "--points-at", "HEAD", "-l", &format!("{name}-*")]).unwrap_or_default();
    let draft_n = |t: &str| t.strip_prefix(&format!("{name}-draft-")).and_then(|n| n.parse::<u32>().ok());
    let mut drafts: Vec<(u32, String)> = git(i.dir, &["tag", "-l", &format!("{name}-draft-*")])
        .unwrap_or_default()
        .lines()
        .filter_map(|t| draft_n(t).map(|n| (n, t.to_string())))
        .collect();
    drafts.sort();
    let this_draft = at_head.lines().find(|t| draft_n(t).is_some()).map(str::to_string);
    let frozen = at_head.lines().find(|t| t.ends_with("-freeze")).map(str::to_string);
    let prev = drafts.iter().rev().map(|d| &d.1).find(|t| Some(*t) != this_draft.as_ref()).cloned();
    let round = match (&frozen, &this_draft, dirty) {
        (Some(f), _, false) => format!("Frozen ({f})"),
        (_, Some(d), false) => format!("Round {}", draft_n(d).unwrap()),
        _ => format!("Round {} (in progress)", drafts.last().map_or(1, |d| d.0 + 1)),
    };
    if dirty {
        risks.push("Uncommitted changes: this page shows work in progress, not a tagged round.".into());
    }

    // --- gates -------------------------------------------------------------------------
    let gates = read_json(&reports.join("gates.json"));
    let stale = match (std::fs::metadata(reports.join("gates.json")), std::fs::metadata(&pcb)) {
        (Ok(g), Ok(p)) => p.modified()? > g.modified()?,
        _ => false,
    };
    if stale {
        risks.push("The board file changed after the last full check; re-run <code>check</code>.".into());
    }
    let gate_ok = |g: &str| gates.as_ref().and_then(|v| v[g].as_bool());
    let all_pass = gates.is_some() && !stale && GATES.iter().all(|g| gate_ok(g) == Some(true));

    let mut checks = String::new();
    for g in GATES {
        let (state, word) = match gate_ok(g) {
            Some(true) if !stale => ("ok", "Pass"),
            Some(true) => ("warn", "Pass, stale"),
            Some(false) => ("bad", "Fail"),
            None => ("warn", "Not run"),
        };
        let mut detail = String::new();
        if (g == "erc" || g == "drc")
            && let Some(rep) = read_json(&reports.join(format!("{g}.json")))
        {
                let all = violations(&rep);
                let (open, waived) = classify(&all, i.waivers);
                detail = format!("{} open, {} waived", open.len(), waived.len());
                // one line per (type, reason), with a count: the same waiver often hits many items
                let mut groups: Vec<(&str, &str, &str, usize)> = vec![];
                for (v, why) in &waived {
                    let (kind, desc) = (v["type"].as_str().unwrap_or(""), v["description"].as_str().unwrap_or(""));
                    match groups.iter_mut().find(|x| x.0 == kind && x.2 == *why) {
                        Some(x) => x.3 += 1,
                        None => groups.push((kind, desc, why, 1)),
                    }
                }
                for (kind, desc, why, n) in groups {
                    let what = if n == 1 { "warning" } else { "warnings" };
                    risks.push(format!(
                        "{n} accepted {} {what}, <code>{}</code> ({}): {}",
                        g.to_uppercase(),
                        esc(kind),
                        esc(desc),
                        esc(why)
                    ));
                }
        }
        if g == "routing"
            && let Some(r) = read_json(&reports.join("routing.json"))
        {
            let len: f64 = r["track_length_mm"].as_object().map_or(0.0, |m| m.values().filter_map(Value::as_f64).sum());
            detail = format!("{} unrouted, {} vias, {:.0} mm of track", r["unrouted_connections"], r["vias_total"], len);
        }
        let _ = writeln!(
            checks,
            "<tr><td>{}</td><td>{}</td><td class=\"muted\">{}</td></tr>",
            esc(gate_label(g)),
            chip(state, word),
            esc(&detail)
        );
    }

    // --- parts and cost ------------------------------------------------------------------
    let bom = cost::read_bom(&fab.join(format!("{name}-bom.csv"))).unwrap_or_default();
    let costj = read_json(&fab.join("cost.json"));
    let budget = bf.and_then(|b| b.order.budget_usd);
    let mut parts = String::new();
    for (comment, refs, lcsc) in &bom {
        let line = costj.as_ref().and_then(|j| j["lines"].as_array()?.iter().find(|l| l["lcsc"] == lcsc.as_str()).cloned());
        let (lib, unit, total, stock) = match &line {
            Some(l) => (
                l["library"].as_str().unwrap_or("").to_string(),
                l["unit_usd"].as_f64().map_or("".into(), |u| format!("${u:.4}")),
                money(&l["line_usd"]),
                l["stock"].to_string(),
            ),
            None => Default::default(),
        };
        if line.as_ref().is_some_and(|l| l["short"] == true) {
            risks.push(format!("Not enough JLCPCB stock of {} ({}).", esc(lcsc), esc(comment)));
        }
        let lib_chip = match lib.as_str() {
            "extended" => chip("warn", "Extended +$3.07"),
            "preferred" => chip("ok", "Preferred"),
            "basic" => chip("ok", "Basic"),
            _ => String::new(),
        };
        let _ = writeln!(
            parts,
            "<tr><td>{}</td><td>{}</td><td><code>{}</code></td><td>{lib_chip}</td><td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td></tr>",
            esc(comment),
            esc(&refs.join(", ")),
            esc(lcsc),
            esc(&stock),
            esc(&unit),
            esc(&total)
        );
    }
    let (cost_head, cost_body) = match &costj {
        Some(j) => {
            let t = &j["totals_usd"];
            let per = t["per_design"].as_f64().unwrap_or(0.0);
            let vs = match budget {
                Some(b) if per > b => {
                    risks.push(format!("Over budget: ${per:.2} per design against ${b:.2}."));
                    format!(" against a ${b:.0} budget: <strong>over by ${:.2}</strong>", per - b)
                }
                Some(b) => format!(" against a ${b:.0} budget: ${:.2} to spare", b - per),
                None => " (no budget set in board.toml)".into(),
            };
            let alone = &j["alone_in_a_parcel_usd"];
            (
                format!("{} per design", money(&t["per_design"])),
                format!(
                    "<p>{} bare boards, {} assembled, prices fetched {}{vs}.</p>\n<dl class=\"sums\">\
                     <dt>Parts</dt><dd>{}</dd><dt>Extended-part fees ({})</dt><dd>{}</dd>\
                     <dt>PCB, setup, stencil</dt><dd>{}</dd><dt>Solder joints</dt><dd>{}</dd>\
                     <dt>Per design</dt><dd class=\"total\">{}</dd></dl>\n\
                     <p class=\"muted\">Shipping (about {} by FedEx) and Israeli VAT are paid once per parcel, which several designs share. \
                     Shipped alone this design would come to {} delivered.</p>",
                    j["order"]["bare_boards"],
                    j["order"]["assembled"],
                    esc(j["date"].as_str().unwrap_or("?")),
                    money(&t["parts"]),
                    t["extended_parts"].as_array().map_or(0, |a| a.len()),
                    money(&t["extended_fees"]),
                    money(&t["pcb_setup_stencil"]),
                    money(&t["joints"]),
                    money(&t["per_design"]),
                    money(&alone["shipping_fedex"]),
                    money(&alone["total"]),
                ),
            )
        }
        None => (
            "not computed".into(),
            "<p>Not computed for this round: run the <code>cost</code> stage (it fetches live JLCPCB prices). \
             Fixed fees per design: PCB $4.00, assembly setup $8.18, stencil $1.53, plus $3.07 for each Extended part.</p>"
                .into(),
        ),
    };

    // --- risks the pipeline knows about ----------------------------------------------
    let readme = std::fs::read_to_string(fab.join("README.md")).unwrap_or_default();
    let unverified = readme_list(&readme, "## Rotation UNVERIFIED");
    if !unverified.is_empty() {
        let list: Vec<String> = unverified.iter().map(|u| esc(u.split(' ').next().unwrap_or(u))).collect();
        risks.push(format!(
            "Placement angle not yet confirmed for {} parts ({}); check each in JLCPCB's placement preview before paying.",
            list.len(),
            list.join(", ")
        ));
    }
    if bom.is_empty() {
        risks.push("No fab files yet (run the <code>fab</code> stage).".into());
    }

    // --- board.toml: requirements, power and secrets ----------------------------------------
    let (mut reqs, mut power, mut power_head, mut power_state) = (String::new(), String::new(), "no board.toml".to_string(), "warn");
    if let Some(bf) = bf {
        for r in &bf.requirements {
            let covers: Vec<String> = r.covered_by.iter().map(|cov| describe_cover(cov, c, bf, &gate_ok, stale)).collect();
            let _ = writeln!(reqs, "<tr><td class=\"rid\">{}</td><td>{}</td><td>{}</td></tr>", esc(&r.id), esc(&r.text), covers.join(" "));
        }
        (power, power_head, power_state) = power_section(bf, &mut risks);
        for (key, date, days) in crate::boardfile::expiries(bf, &crate::schematic::today()) {
            if days < 0 {
                risks.push(format!("The secret <code>{}</code> expired on {}: renew it, then load it again with <code>devctl provision</code>.", esc(&key), esc(&date)));
            } else if days <= EXPIRY_WARN_DAYS {
                risks.push(format!(
                    "The secret <code>{}</code> expires on {} (in {days} days): renew it, then load it again with <code>devctl provision</code>.",
                    esc(&key),
                    esc(&date)
                ));
            }
        }
    }

    // --- changes since the last round ------------------------------------------------------
    // the board, and the shared code and libraries that can change it (in this repo's layout)
    let paths: Vec<&str> = [".", "../../crates/pcbgen", "../../lib", "../../rules"].into_iter().filter(|p| i.dir.join(p).exists()).collect();
    let changes = match &prev {
        Some(p) => {
            let range = format!("{p}..HEAD");
            let mut args = vec!["log", "--format=%h %s", &range, "--"];
            args.extend(paths);
            let log = git(i.dir, &args).unwrap_or_default();
            let items: Vec<String> = log.lines().map(|l| format!("<li><code>{}</code>{}</li>", esc(&l[..l.find(' ').unwrap_or(0)]), esc(&l[l.find(' ').unwrap_or(0)..]))).collect();
            if items.is_empty() {
                format!("<p>No commits touch this board since <code>{}</code>.</p>", esc(p))
            } else {
                format!("<p>Commits since <code>{}</code>:</p>\n<ul class=\"log\">{}</ul>", esc(p), items.join("\n"))
            }
        }
        None => "<p>This is the first round: no earlier <code>draft</code> tag to compare with.</p>".into(),
    };
    let notes = std::fs::read_to_string(i.dir.join("round.md")).map(|m| markdown(&m)).unwrap_or_else(|_| {
        risks.push("No <code>round.md</code>: nothing written about why things changed this round.".into());
        String::new()
    });

    let firmware = firmware_section(&i.base.join("firmware/sim.json"), &mut risks);
    let case = case_section(&i.base.join("case"), &pcb, &mut risks);
    let bringup = bringup_section(i.dir, &mut risks);

    // --- pictures ---------------------------------------------------------------------------
    let top = render(&pcb, "F.Cu,B.Cu,F.Fab,F.Courtyard,F.SilkS,Edge.Cuts", false);
    let bottom = render(&pcb, "B.Cu,B.Fab,B.Courtyard,B.SilkS,Edge.Cuts", true);
    let pic = |uri: &Option<String>, what: &str| match uri {
        Some(u) => format!("<figure><img src=\"{u}\" alt=\"{what} of the board\"><figcaption>{what}</figcaption></figure>"),
        None => format!("<figure class=\"none\"><figcaption>{what}: no board file to draw</figcaption></figure>"),
    };

    // --- summary chips ------------------------------------------------------------------------
    let checks_chip = if gates.is_none() {
        chip("warn", "Checks not run")
    } else if all_pass {
        chip("ok", "All checks pass")
    } else {
        chip("bad", "Checks failing")
    };
    let cost_state = match (&costj, budget) {
        (None, _) => "warn",
        (Some(j), Some(b)) if j["totals_usd"]["per_design"].as_f64().unwrap_or(0.0) > b => "bad",
        _ => "ok",
    };
    let risk_list: String = risks.iter().map(|r| format!("<li>{r}</li>\n")).collect();
    let title = format!("{} review", c.title);
    let date = crate::schematic::today();

    let mut h = String::new();
    h += &format!("<title>{}</title>\n", esc(&title));
    h += STYLE;
    let _ = write!(
        h,
        "<main>\n<header>\n<p class=\"eyebrow\">{} · revision {} · {}</p>\n<h1>{}</h1>\n<p class=\"meta\">Commit <code>{head}</code>{}, page made {date}</p>\n\
         <div class=\"chips\">{checks_chip}{}{}{}</div>\n</header>\n",
        esc(name),
        esc(&c.rev),
        esc(&round),
        esc(&c.title),
        if dirty { " with uncommitted changes" } else { "" },
        chip(cost_state, &format!("Cost: {cost_head}")),
        chip(power_state, &format!("Power: {power_head}")),
        chip(if risks.is_empty() { "ok" } else { "warn" }, &format!("{} things to check", risks.len())),
    );
    let _ = writeln!(h, "<section class=\"pics\">{}{}</section>", pic(&top, "Top"), pic(&bottom, "Bottom, seen from below"));
    let _ = writeln!(h, "<section><h2>This round</h2>\n{notes}\n{changes}\n</section>");
    let _ = writeln!(h, "<section><h2>Things to check</h2>\n<ul class=\"risks\">\n{risk_list}</ul>\n</section>");
    let _ = writeln!(h, "<section><h2>The case</h2>\n{case}</section>");
    let _ = write!(
        h,
        "<section><h2>What it must do</h2>\n<p class=\"muted\">From <code>spec.md</code>; each line names what shows it is met.</p>\n\
         <div class=\"scroll\"><table><thead><tr><th>ID</th><th>Requirement</th><th>Covered by</th></tr></thead><tbody>\n{reqs}</tbody></table></div>\n</section>\n"
    );
    let _ = writeln!(h, "<section><h2>Cost</h2>\n{cost_body}\n<div class=\"scroll\"><table class=\"parts\"><thead><tr><th>Part</th><th>On the board</th><th>LCSC</th><th>Library</th><th class=\"num\">JLCPCB stock</th><th class=\"num\">Each</th><th class=\"num\">Line</th></tr></thead><tbody>\n{parts}</tbody></table></div>\n</section>");
    let _ = writeln!(h, "<section><h2>Power budget</h2>\n{power}</section>");
    let _ = writeln!(h, "<section><h2>Checks</h2>\n<div class=\"scroll\"><table><tbody>\n{checks}</tbody></table></div>\n</section>");
    let _ = writeln!(h, "<section><h2>Firmware in simulation</h2>\n{firmware}\n</section>");
    let _ = writeln!(h, "<section><h2>Bring-up</h2>\n<p class=\"muted\">The same self-test, on a real board.</p>\n{bringup}</section>\n</main>");

    let dir = i.base.join("review");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("index.html");
    std::fs::write(&path, h)?;
    Ok(path)
}

/// One `covered_by` entry in words, as a chip.
fn describe_cover(cov: &str, c: &Circuit, bf: &BoardFile, gate_ok: &dyn Fn(&str) -> Option<bool>, stale: bool) -> String {
    let (kind, x) = cov.split_once(':').unwrap_or(("", cov));
    match kind {
        "part" => {
            let value = c.find_part(x).map(|p| c.parts[p.0].value.as_str()).unwrap_or("?");
            format!("<span class=\"cov\">{} <span class=\"muted\">{}</span></span>", esc(x), esc(value))
        }
        "pin" => {
            let g = bf.pins.iter().find(|p| p.signal == x).map_or(String::new(), |p| format!(" GPIO {}", p.gpio));
            format!("<span class=\"cov\">{}<span class=\"muted\">{g}</span></span>", esc(x))
        }
        "test" => format!("<span class=\"cov\">self-test <span class=\"muted\">{}</span></span>", esc(x)),
        "power" | "sleep" => {
            let tally = bf.power.tally();
            let Some(t) = tally.iter().find(|t| t.source.name == x) else { return esc(cov) };
            if kind == "sleep" {
                sleep_chip(t)
            } else {
                chip(if t.over() { "bad" } else { "ok" }, &format!("Power from {x}: {:.0} of {} mA", t.ma, t.budget_ma))
            }
        }
        "gate" => {
            let state = match gate_ok(x) {
                Some(true) if !stale => "ok",
                Some(false) => "bad",
                _ => "warn",
            };
            chip(state, gate_label(x))
        }
        _ => esc(cov),
    }
}

/// Board-house palette: soldermask-green neutrals, copper accent; pictures on white.
const STYLE: &str = r#"<link rel="preconnect" href="https://fonts.googleapis.com">
<link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=Barlow+Semi+Condensed:wght@500;600&family=IBM+Plex+Sans:wght@400;500;600&family=IBM+Plex+Mono:wght@400;500&display=swap">
<style>
/* One column of stacked sections; summary chips first, then pictures, then detail tables. */
:root {
  --bg: #f5f7f4; --surface: #ffffff; --fg: #1c2620; --muted: #5d6b62; --line: #d9e0da;
  --accent: #a4561c; --paper: #ffffff;
  --ok-bg: #dcefe2; --ok-fg: #1d5c33; --warn-bg: #f7ead2; --warn-fg: #7a4b0c; --bad-bg: #f6d9d6; --bad-fg: #8c231a;
  --display: "Barlow Semi Condensed", "Arial Narrow", system-ui, sans-serif;
  --body: "IBM Plex Sans", system-ui, -apple-system, "Segoe UI", sans-serif;
  --mono: "IBM Plex Mono", ui-monospace, "SFMono-Regular", Menlo, monospace;
}
@media (prefers-color-scheme: dark) { :root:not([data-theme="light"]) {
  --bg: #111814; --surface: #18211c; --fg: #e3ebe5; --muted: #98a89e; --line: #2a372f;
  --accent: #e0925a; --paper: #f4f6f3;
  --ok-bg: #1e3a28; --ok-fg: #a6dcb6; --warn-bg: #3d2f16; --warn-fg: #f0c987; --bad-bg: #45201c; --bad-fg: #f3aaa2;
  color-scheme: dark; } }
:root[data-theme="dark"] {
  --bg: #111814; --surface: #18211c; --fg: #e3ebe5; --muted: #98a89e; --line: #2a372f;
  --accent: #e0925a; --paper: #f4f6f3;
  --ok-bg: #1e3a28; --ok-fg: #a6dcb6; --warn-bg: #3d2f16; --warn-fg: #f0c987; --bad-bg: #45201c; --bad-fg: #f3aaa2;
  color-scheme: dark; }
body { background: var(--bg); color: var(--fg); font: 15px/1.55 var(--body); }
main { max-width: 60rem; margin: 0 auto; padding-inline: 16px; padding-block: 2rem 4rem; display: grid; gap: 2.25rem; }
main > * { min-width: 0; }
header { display: grid; gap: .5rem; }
h1, h2, h3, h4, h5 { font-family: var(--display); font-weight: 600; text-wrap: balance; margin: 0; line-height: 1.15; }
h1 { font-size: 2.3rem; }
h2 { font-size: 1.45rem; padding-bottom: .35rem; border-bottom: 2px solid var(--accent); margin-bottom: .75rem; }
h3, h4, h5 { font-size: 1.15rem; margin-top: 1rem; }
p { margin: .5rem 0; max-width: 68ch; }
.eyebrow { text-transform: uppercase; letter-spacing: .08em; font-size: .78rem; color: var(--accent); font-weight: 600; margin: 0; }
.meta, .muted { color: var(--muted); }
.meta { margin: 0; font-size: .9rem; }
code { font-family: var(--mono); font-size: .88em; }
.chips { display: flex; flex-wrap: wrap; gap: .5rem; margin-top: .5rem; }
.chip { display: inline-block; padding: .15rem .6rem; border-radius: 999px; font-size: .82rem; font-weight: 500; white-space: nowrap; }
.chips .chip { font-size: .92rem; padding: .3rem .8rem; }
.chip.ok { background: var(--ok-bg); color: var(--ok-fg); }
.chip.warn { background: var(--warn-bg); color: var(--warn-fg); }
.chip.bad { background: var(--bad-bg); color: var(--bad-fg); }
.pics { display: grid; grid-template-columns: repeat(auto-fit, minmax(16rem, 1fr)); gap: 1rem; }
figure { margin: 0; display: grid; gap: .4rem; }
figure img { background: var(--paper); border: 1px solid var(--line); border-radius: 6px; width: 100%; height: auto; padding: .75rem; box-sizing: border-box; }
figure.none { border: 1px dashed var(--line); border-radius: 6px; padding: 2rem 1rem; text-align: center; }
figcaption { font-size: .85rem; color: var(--muted); }
.scroll { overflow-x: auto; }
table { border-collapse: collapse; width: 100%; font-size: .9rem; }
th { text-align: left; font-weight: 600; color: var(--muted); font-size: .78rem; text-transform: uppercase; letter-spacing: .05em; }
th, td { padding: .45rem .6rem; border-bottom: 1px solid var(--line); vertical-align: top; }
.num { text-align: right; font-variant-numeric: tabular-nums; white-space: nowrap; }
tr.sum td { font-weight: 600; border-bottom: none; }
.rid { font-family: var(--mono); font-weight: 500; color: var(--accent); }
td .cov, td .chip { margin: 0 .35rem .3rem 0; }
.cov { display: inline-block; font-size: .85rem; border: 1px solid var(--line); border-radius: 4px; padding: 0 .4rem; background: var(--surface); }
ul { padding-left: 1.2rem; margin: .5rem 0; }
li { margin: .25rem 0; max-width: 72ch; }
ul.risks li::marker { color: var(--accent); }
ul.log { list-style: none; padding: 0; }
ul.log code { color: var(--accent); margin-right: .5rem; }
dl.sums { display: grid; grid-template-columns: max-content max-content; gap: .2rem 1.5rem; margin: .75rem 0; font-variant-numeric: tabular-nums; }
dl.sums dt { color: var(--muted); }
dl.sums dd { margin: 0; text-align: right; }
dl.sums .total { font-weight: 600; color: var(--accent); }
</style>
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_rfc4648() {
        for (i, o) in [("", ""), ("f", "Zg=="), ("fo", "Zm8="), ("foo", "Zm9v"), ("foob", "Zm9vYg=="), ("fooba", "Zm9vYmE="), ("foobar", "Zm9vYmFy")] {
            assert_eq!(base64(i.as_bytes()), o);
        }
    }

    #[test]
    fn markdown_subset() {
        let h = markdown("## What changed\nMoved **U4** away\nfrom `U2`.\n\n- one <b>\n  - nested\n- two\n\ntext");
        assert_eq!(
            h,
            "<h4>What changed</h4>\n<p>Moved <strong>U4</strong> away from <code>U2</code>.</p>\n<ul>\n<li>one &lt;b&gt;</li>\n<ul>\n<li>nested</li>\n</ul>\n<li>two</li>\n</ul>\n<p>text</p>\n"
        );
    }

    #[test]
    fn readme_bullets_under_a_heading() {
        let r = "# x\n## Rotation corrections applied\n- J1: a\n## Rotation UNVERIFIED (no known)\n\n- D1 (LED)\n- U1 (ESP)\n## Other\n- no\n";
        assert_eq!(readme_list(r, "## Rotation UNVERIFIED"), vec!["D1 (LED)", "U1 (ESP)"]);
    }

    const BATTERY_BOARD: &str = r#"
[board]
name = "t"
revision = "A"
module = "U1"

[[power.source]]
name = "usb"
what = "USB-C"
[[power.source.limit]]
name = "USB 2.0"
ma = 500
source = "spec"

[[power.source]]
name = "battery"
what = "LiPo"
sleep_ua = 20
[[power.source.limit]]
name = "LDO"
ma = 600
source = "datasheet"

[[power.load]]
name = "module"
ma = 355
source = "datasheet"

[[power.sleep_load]]
name = "module asleep"
ua = 8
source = "VERIFIED: datasheet"

[[power.sleep_load]]
name = "LDO"
ua = 4
source = "INFERRED"
"#;

    #[test]
    fn power_section_per_source_and_asleep() {
        let bf = crate::boardfile::parse(BATTERY_BOARD).unwrap();
        let mut risks = vec![];
        let (html, head, state) = power_section(&bf, &mut risks);
        assert_eq!((head.as_str(), state), ("usb 355 of 500 mA; battery 355 of 600 mA, asleep 12.0 of 20 µA", "ok"));
        assert!(html.contains("<h3>From USB-C") && html.contains("<h3>From LiPo") && html.contains("<h4>Asleep on battery"), "{html}");
        assert!(html.contains("Asleep: 12.0 of 20 µA, estimated"), "{html}");
        assert_eq!(risks, vec!["1 of the power figures are estimates, not datasheet values."]);
        // over the sleep budget: a red chip and a risk
        let bf = crate::boardfile::parse(&BATTERY_BOARD.replace("ua = 8", "ua = 18")).unwrap();
        let mut risks = vec![];
        let (html, _, state) = power_section(&bf, &mut risks);
        assert_eq!(state, "bad");
        assert!(html.contains("chip bad\">Asleep: 22.0 of 20 µA"), "{html}");
        assert!(risks.iter().any(|r| r.contains("Asleep on battery: 22.0 µA, over the 20 µA budget")), "{risks:?}");
    }

    #[test]
    fn bringup_reads_the_newest_selftest() {
        let dir = std::env::temp_dir().join(format!("pcbgen-bringup-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut risks = vec![];
        assert!(bringup_section(&dir, &mut risks).contains("Not run yet"));
        assert!(risks.is_empty());
        std::fs::create_dir_all(dir.join("bringup")).unwrap();
        let report = |ok: bool, result: &str| {
            format!(r#"{{"ok": {ok}, "date": "d", "port": "/dev/ttyACM0", "tests": [{{"test": "mic", "result": "{result}", "detail": "rms 0"}}], "needs_person": [{{"test": "rgb", "look": "red"}}], "problems": []}}"#)
        };
        std::fs::write(dir.join("bringup/selftest-2027-01-01.json"), report(true, "pass")).unwrap();
        std::fs::write(dir.join("bringup/selftest-2027-01-02.json"), report(false, "fail")).unwrap();
        let html = bringup_section(&dir, &mut risks);
        assert!(html.contains("chip bad\">Fail") && html.contains("<code>mic</code>") && html.contains("should show red"), "{html}");
        assert_eq!(risks, vec!["The built board fails its self-test (see Bring-up)."]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

}
