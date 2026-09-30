//! The readiness page (docs/workflow.md "Right the first time"): what the owner reads before
//! saying "freeze". A board is ordered once and a mistake costs a second shipment, so the page
//! shows, per requirement, how it was proven (`[[requirement.proof]]`), and every guess the
//! design still rests on (`[[risk]]`) sorted by what a miss would cost. Red blocks freeze:
//! a requirement nothing proves, or a guess that would need a new board and that the owner
//! hasn't accepted by name.
//!
//! The `review` stage writes it next to the review page: `<base>/review/readiness.html` (an
//! Artifact page body, like the review page) and `readiness.json`, which `scripts/freeze.sh`
//! reads so a freeze can't happen past a red item or a stale page.

use std::fmt::Write as _;
use std::path::Path;

use anyhow::Result;
use serde_json::json;

use crate::boardfile::{BoardFile, Fix, How, Requirement, Risk, Tag};
use crate::circuit::Circuit;
use crate::review::{STYLE, chip, esc};

pub const PAGE: &str = "readiness.html";
pub const JSON: &str = "readiness.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// Blocks freeze.
    Red,
    /// A known, limited risk: only the delivered board can show it, or a miss is fixable on it,
    /// or the owner accepted it.
    Amber,
    /// Proven, or fixable for free by a firmware update.
    Green,
}

impl State {
    /// The chip class the review page's STYLE gives each state.
    fn class(self) -> &'static str {
        match self {
            State::Red => "bad",
            State::Amber => "warn",
            State::Green => "ok",
        }
    }
}

/// Red with no proof at all; amber when every proof says "can't be proven before delivery".
pub fn requirement_state(r: &Requirement) -> State {
    if r.proofs.is_empty() {
        State::Red
    } else if r.proofs.iter().all(|p| p.how == How::Unprovable) {
        State::Amber
    } else {
        State::Green
    }
}

/// Only a miss that needs a new board can block freeze, and the owner's acceptance lifts it to
/// amber (a blank `accepted` is a gate problem and doesn't count).
pub fn risk_state(r: &Risk) -> State {
    let accepted = r.accepted.as_deref().is_some_and(|a| !a.trim().is_empty());
    match (r.fix, accepted) {
        (Fix::NewBoard, false) => State::Red,
        (Fix::NewBoard, true) | (Fix::Rework, _) => State::Amber,
        (Fix::Firmware, _) => State::Green,
    }
}

/// The counts behind the verdict. `red` holds each blocking item in plain text (not HTML).
#[derive(Debug, Default, PartialEq)]
pub struct Readiness {
    pub red: Vec<String>,
    pub amber: usize,
    pub green: usize,
    /// False while board.toml has no proof and no risk at all: nobody has done this yet.
    pub filled: bool,
}

pub fn assess(bf: Option<&BoardFile>) -> Readiness {
    let mut r = Readiness::default();
    let Some(bf) = bf else {
        // never "ready" by having nothing to look at
        r.red.push("There is no board.toml, so nothing says what the board must do or how that was proven.".into());
        return r;
    };
    r.filled = !bf.risks.is_empty() || bf.requirements.iter().any(|q| !q.proofs.is_empty());
    let mut count = |state: State, red: String| match state {
        State::Red => r.red.push(red),
        State::Amber => r.amber += 1,
        State::Green => r.green += 1,
    };
    for q in &bf.requirements {
        count(requirement_state(q), format!("{} ({}): nothing shows yet that it is met.", q.id, q.text.trim()));
    }
    for k in &bf.risks {
        count(risk_state(k), format!("{}: a miss would need a new board, and it hasn't been accepted.", k.what.trim().trim_end_matches('.')));
    }
    r
}

impl Readiness {
    pub fn ready(&self) -> bool {
        self.red.is_empty()
    }

    /// The line at the top of the page.
    pub fn verdict(&self) -> String {
        match self.red.len() {
            0 => "Ready to freeze".into(),
            1 => "1 thing blocks freeze".into(),
            n => format!("{n} things block freeze"),
        }
    }

    /// The review page's summary chip.
    pub fn summary_chip(&self) -> String {
        if !self.filled {
            chip("warn", "Readiness: not filled in")
        } else if self.ready() {
            chip("ok", "Readiness: ready")
        } else {
            chip("bad", &format!("Readiness: {} blocking", self.red.len()))
        }
    }

    fn count_chips(&self) -> String {
        format!(
            "{}{}{}",
            chip(if self.ready() { "ok" } else { "bad" }, &format!("{} blocking", self.red.len())),
            chip(if self.amber == 0 { "ok" } else { "warn" }, &format!("{} to keep in mind", self.amber)),
            chip("ok", &format!("{} settled", self.green)),
        )
    }

    /// The review page's short "Readiness" section.
    pub fn section(&self) -> String {
        let lead = if !self.filled {
            "Not filled in yet: <code>board.toml</code> names no proof for any requirement and no open risk, so every requirement counts as unproven."
        } else if self.ready() {
            "Nothing blocks freeze."
        } else {
            "Freeze is blocked until these are proven, designed out, or accepted by name."
        };
        format!(
            "<p>{} {lead}</p>\n<div class=\"chips\">{}</div>\n\
             <p class=\"muted\">The readiness page (<code>review/{PAGE}</code>, made with this one) shows how each requirement was proven, \
             every guess the design still rests on, and what a miss would cost. Read it before saying freeze.</p>\n",
            chip(if self.ready() { "ok" } else { "bad" }, &self.verdict()),
            self.count_chips()
        )
    }

    fn json(&self, date: &str) -> serde_json::Value {
        json!({"red": self.red.len(), "amber": self.amber, "green": self.green, "date": date, "filled": self.filled, "blocking": self.red})
    }
}

/// A proof's kind in the owner's words.
fn how_label(h: How) -> &'static str {
    match h {
        How::Simulated => "Simulated",
        How::Datasheet => "Checked against the datasheet",
        How::Devboard => "Measured on the dev board",
        How::Gate => "Automatic check",
        How::Unprovable => "Can't be proven before delivery",
    }
}

fn tag_label(t: Tag) -> &'static str {
    match t {
        Tag::Unverified => "Not verified yet",
        Tag::Inferred => "An estimate",
    }
}

/// The three groups of open items, most expensive first.
const GROUPS: [(Fix, &str, &str); 3] = [
    (Fix::NewBoard, "Would need a new board", "A miss here means a second order and a second shipment. Each one blocks freeze until it is proven or accepted by name."),
    (Fix::Rework, "Fixable on the delivered board (rework)", "A miss here means a soldering iron on the board that arrives: a swapped part or a cut track."),
    (Fix::Firmware, "Fixable by a firmware update", "A miss here costs nothing but an update over USB or Wi-Fi."),
];

/// What no check on this laptop can show, said the same way on every board's page.
const CANNOT_PROVE: &str = "<section><h2>What simulation can never prove</h2>\n\
<p>Some things only the real device can show, however many checks pass here: how far the radio reaches, how the microphone sounds, \
how much current the board really draws asleep (and so how long the battery lasts), and how the case feels in the hand. \
These are measured on the first delivered board.</p>\n</section>\n";

/// The few rules this page adds to the review page's STYLE.
const EXTRA_STYLE: &str = "<style>\n\
.verdict { font-family: var(--display); font-weight: 600; font-size: 1.7rem; line-height: 1.15; margin: .25rem 0 0; }\n\
.verdict.ok { color: var(--ok-fg); }\n.verdict.bad { color: var(--bad-fg); }\n\
.proof { margin-bottom: .35rem; }\n\
ul.open { list-style: none; padding: 0; }\n\
ul.open li { border-left: 3px solid var(--line); padding: .1rem 0 .1rem .75rem; margin: .75rem 0; }\n\
ul.open li.bad { border-color: var(--bad-fg); }\nul.open li.warn { border-color: var(--warn-fg); }\n\
ul.open .chip { margin-left: .35rem; }\n\
ul.open p { margin: .2rem 0; }\n\
</style>\n";

fn requirement_rows(bf: &BoardFile) -> String {
    let mut rows = String::new();
    for q in &bf.requirements {
        let state = requirement_state(q);
        let word = match state {
            State::Red => "Not proven",
            State::Amber => "Only on the real board",
            State::Green => "Proven",
        };
        let mut proofs: String = q
            .proofs
            .iter()
            .map(|p| format!("<div class=\"proof\">{}{}</div>", chip(if p.how == How::Unprovable { "warn" } else { "ok" }, how_label(p.how)), esc(p.evidence.trim())))
            .collect();
        if proofs.is_empty() {
            proofs = "<span class=\"muted\">Nothing shows this yet.</span>".into();
        }
        let _ = writeln!(rows, "<tr><td class=\"rid\">{}</td><td>{}</td><td>{}</td><td>{proofs}</td></tr>", esc(&q.id), esc(&q.text), chip(state.class(), word));
    }
    rows
}

fn open_items(bf: &BoardFile) -> String {
    let mut out = String::new();
    for (fix, heading, what) in GROUPS {
        let _ = write!(out, "<h3>{heading}</h3>\n<p class=\"muted\">{what}</p>\n");
        let items: Vec<&Risk> = bf.risks.iter().filter(|k| k.fix == fix).collect();
        if items.is_empty() {
            out += "<p>Nothing listed.</p>\n";
            continue;
        }
        out += "<ul class=\"open\">\n";
        for k in items {
            let state = risk_state(k);
            // only the new-board group can block, so only there does an item say where it stands
            let standing = match (fix, state) {
                (Fix::NewBoard, State::Red) => chip("bad", "Blocks freeze"),
                (Fix::NewBoard, _) => chip("warn", "Accepted"),
                _ => String::new(),
            };
            let _ = write!(out, "<li class=\"{}\"><p><strong>{}</strong>{}{standing}</p>\n<p>If it is wrong: {}</p>\n", state.class(), esc(k.what.trim()), chip("warn", tag_label(k.tag)), esc(k.miss.trim()));
            if let Some(c) = &k.check {
                let _ = writeln!(out, "<p class=\"muted\">How it gets settled: {}</p>", esc(c.trim()));
            }
            if let Some(a) = k.accepted.as_deref().filter(|a| !a.trim().is_empty()) {
                let _ = writeln!(out, "<p>Accepted by the owner: {}</p>", esc(a.trim()));
            }
            out += "</li>\n";
        }
        out += "</ul>\n";
    }
    out
}

/// The page. `flagged` is the review page's automatic "things to check" (escaped HTML, one item
/// each): what the pipeline noticed on its own, next to what board.toml declares.
pub fn page(c: &Circuit, bf: Option<&BoardFile>, r: &Readiness, flagged: &[String], date: &str) -> String {
    let mut h = format!("<title>{}</title>\n", esc(&format!("{} readiness", c.title)));
    h += STYLE;
    h += EXTRA_STYLE;
    let _ = write!(
        h,
        "<main>\n<header>\n<p class=\"eyebrow\">{} · revision {} · before freeze</p>\n<h1>{}</h1>\n<p class=\"verdict {}\">{}</p>\n\
         <p class=\"meta\">Red blocks freeze. Amber is a known risk with a limited cost. Green is proven, or free to fix later. Page made {date}.</p>\n\
         <div class=\"chips\">{}</div>\n</header>\n",
        esc(&c.name),
        esc(&c.rev),
        esc(&c.title),
        if r.ready() { "ok" } else { "bad" },
        esc(&r.verdict()),
        r.count_chips()
    );
    if !r.ready() {
        let items: String = r.red.iter().map(|t| format!("<li>{}</li>\n", esc(t))).collect();
        let _ = write!(
            h,
            "<section><h2>What blocks freeze</h2>\n<ul class=\"risks\">\n{items}</ul>\n\
             <p class=\"muted\">Each goes away by proving it, by changing the design so a miss no longer needs a new board, or by the owner accepting it by name.</p>\n</section>\n"
        );
    }
    let (reqs, open) = match bf {
        Some(bf) if !bf.requirements.is_empty() => (
            format!(
                "<div class=\"scroll\"><table><thead><tr><th>ID</th><th>It must</th><th>Where it stands</th><th>How we know</th></tr></thead><tbody>\n{}</tbody></table></div>\n",
                requirement_rows(bf)
            ),
            open_items(bf),
        ),
        Some(bf) => ("<p>No requirements are listed in <code>board.toml</code>.</p>\n".into(), open_items(bf)),
        None => ("<p>No <code>board.toml</code>.</p>\n".into(), "<p>No <code>board.toml</code>.</p>\n".into()),
    };
    let _ = write!(h, "<section><h2>What it must do, and how we know it will</h2>\n<p class=\"muted\">Every requirement from the spec, with what showed that it is met.</p>\n{reqs}</section>\n");
    let _ = write!(
        h,
        "<section><h2>Still open</h2>\n<p class=\"muted\">Every guess and unchecked fact the design still rests on, sorted by what a miss would cost, most expensive first.</p>\n{open}</section>\n"
    );
    let list: String = flagged.iter().map(|f| format!("<li>{f}</li>\n")).collect();
    let _ = write!(
        h,
        "<section><h2>Also flagged by the pipeline</h2>\n<p class=\"muted\">Found automatically while making the review page; they don't change the verdict above.</p>\n{}</section>\n",
        if list.is_empty() { "<p>Nothing.</p>\n".to_string() } else { format!("<ul class=\"risks\">\n{list}</ul>\n") }
    );
    h += CANNOT_PROVE;
    h += "</main>\n";
    h
}

/// Writes `<dir>/readiness.html` and `<dir>/readiness.json`; returns the counts for the review page.
pub fn write(dir: &Path, c: &Circuit, bf: Option<&BoardFile>, flagged: &[String], date: &str) -> Result<Readiness> {
    let r = assess(bf);
    std::fs::create_dir_all(dir)?;
    std::fs::write(dir.join(PAGE), page(c, bf, &r, flagged, date))?;
    std::fs::write(dir.join(JSON), serde_json::to_string_pretty(&r.json(date))? + "\n")?;
    Ok(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOARD: &str = r#"
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

[[requirement]]
id = "R1"
text = "Records a note"
covered_by = ["part:U1"]
[[requirement.proof]]
how = "simulated"
evidence = "QEMU scenario <record>"
[[requirement.proof]]
how = "unprovable"
evidence = "How it sounds; listened to at bring-up"

[[requirement]]
id = "R2"
text = "Reaches the home Wi-Fi"
covered_by = ["part:U1"]
[[requirement.proof]]
how = "unprovable"
evidence = "Radio range; tried at bring-up"

[[requirement]]
id = "R3"
text = "Lasts a week"
covered_by = ["part:U1"]

[[risk]]
what = "Microphone pinout."
tag = "UNVERIFIED"
fix = "new_board"
miss = "No sound at all"
check = "Datasheet checker"

[[risk]]
what = "Antenna keep-out"
tag = "INFERRED"
fix = "new_board"
miss = "Short range"
accepted = "I accept the antenna keep-out risk"

[[risk]]
what = "Charge resistor"
tag = "INFERRED"
fix = "rework"
miss = "Slow charging"

[[risk]]
what = "Sleep timing"
tag = "INFERRED"
fix = "firmware"
miss = "A shorter battery life until an update"
"#;

    fn board() -> BoardFile {
        crate::boardfile::parse(BOARD).unwrap()
    }

    #[test]
    fn counts_red_amber_green() {
        let bf = board();
        let states: Vec<State> = bf.requirements.iter().map(requirement_state).collect();
        assert_eq!(states, [State::Green, State::Amber, State::Red]);
        let states: Vec<State> = bf.risks.iter().map(risk_state).collect();
        assert_eq!(states, [State::Red, State::Amber, State::Amber, State::Green]);
        let r = assess(Some(&bf));
        assert_eq!(
            r.red,
            ["R3 (Lasts a week): nothing shows yet that it is met.", "Microphone pinout: a miss would need a new board, and it hasn't been accepted."]
        );
        assert_eq!((r.amber, r.green, r.filled, r.ready()), (3, 2, true, false));
        assert_eq!(r.verdict(), "2 things block freeze");
        assert!(r.summary_chip().contains("chip bad\">Readiness: 2 blocking"));
        let j = r.json("2026-09-30");
        assert_eq!((j["red"].as_u64(), j["amber"].as_u64(), j["green"].as_u64(), j["date"].as_str()), (Some(2), Some(3), Some(2), Some("2026-09-30")));
        assert_eq!(j["blocking"].as_array().map(Vec::len), Some(2));
        // freeze.sh reads the count from a line of its own
        assert!(serde_json::to_string_pretty(&j).unwrap().lines().any(|l| l.trim() == "\"red\": 2,"));

        // accepting the last new-board guess and proving R3 clears the page
        let done = BOARD
            .replace("check = \"Datasheet checker\"", "accepted = \"Go ahead with the microphone pinout\"")
            .replace("text = \"Lasts a week\"\ncovered_by = [\"part:U1\"]\n", "text = \"Lasts a week\"\ncovered_by = [\"part:U1\"]\n[[requirement.proof]]\nhow = \"devboard\"\nevidence = \"42 µA measured\"\n");
        let r = assess(Some(&crate::boardfile::parse(&done).unwrap()));
        assert_eq!((r.red.len(), r.amber, r.green, r.verdict().as_str()), (0, 4, 3, "Ready to freeze"));
        assert!(r.summary_chip().contains("chip ok\">Readiness: ready"));
        // a blank acceptance accepts nothing
        let blank = crate::boardfile::parse(&done.replace("\"Go ahead with the microphone pinout\"", "\" \"")).unwrap();
        assert_eq!(assess(Some(&blank)).verdict(), "1 thing blocks freeze");
    }

    #[test]
    fn not_filled_in_is_never_ready() {
        let bare = BOARD.split("[[risk]]").next().unwrap().split("[[requirement.proof]]").next().unwrap();
        let r = assess(Some(&crate::boardfile::parse(bare).unwrap()));
        assert_eq!((r.red.len(), r.amber, r.green, r.filled), (1, 0, 0, false));
        assert!(r.summary_chip().contains("chip warn\">Readiness: not filled in"));
        assert!(r.section().contains("Not filled in yet"));
        let r = assess(None);
        assert!(!r.ready() && !r.filled && r.red[0].contains("no board.toml"), "{r:?}");
    }

    #[test]
    fn page_text() {
        let bf = board();
        let r = assess(Some(&bf));
        let c = Circuit::new("t", "Capture clip", "A");
        let h = page(&c, Some(&bf), &r, &["No <code>round.md</code>.".to_string()], "2026-09-30");
        assert!(h.starts_with("<title>Capture clip readiness</title>\n"), "{}", &h[..60]);
        for want in [
            "<p class=\"verdict bad\">2 things block freeze</p>",
            "<h2>What blocks freeze</h2>",
            "<li>R3 (Lasts a week): nothing shows yet that it is met.</li>",
            "<h3>Would need a new board</h3>",
            "<h3>Fixable on the delivered board (rework)</h3>",
            "<h3>Fixable by a firmware update</h3>",
            "<span class=\"chip ok\">Simulated</span>QEMU scenario &lt;record&gt;",
            "<span class=\"chip warn\">Can't be proven before delivery</span>Radio range; tried at bring-up",
            "<span class=\"chip bad\">Not proven</span>",
            "<li class=\"bad\"><p><strong>Microphone pinout.</strong><span class=\"chip warn\">Not verified yet</span><span class=\"chip bad\">Blocks freeze</span></p>",
            "<p>If it is wrong: No sound at all</p>",
            "<p class=\"muted\">How it gets settled: Datasheet checker</p>",
            "<p>Accepted by the owner: I accept the antenna keep-out risk</p>",
            "<li class=\"ok\"><p><strong>Sleep timing</strong><span class=\"chip warn\">An estimate</span></p>",
            "<h2>Also flagged by the pipeline</h2>",
            "<li>No <code>round.md</code>.</li>",
            "how far the radio reaches, how the microphone sounds",
            "how the case feels in the hand",
        ] {
            assert!(h.contains(want), "missing {want:?} in\n{h}");
        }
        // the groups come most expensive first
        let at = |s: &str| h.find(s).unwrap();
        assert!(at("Would need a new board") < at("Fixable on the delivered board") && at("Fixable on the delivered board") < at("Fixable by a firmware update"));
        // nothing red: the verdict changes and the blockers' section goes
        let ok = Readiness { red: vec![], amber: 1, green: 2, filled: true };
        let h = page(&c, Some(&bf), &ok, &[], "2026-09-30");
        assert!(h.contains("<p class=\"verdict ok\">Ready to freeze</p>") && !h.contains("What blocks freeze"), "{h}");
        assert!(ok.section().contains("review/readiness.html"));
    }
}
