//! `devctl provision`: the `[provision]` values into the board's NVS, over the line protocol
//! in `firmware/components/provision/include/provision.h`. A value is never printed, logged or
//! put in an error: only its key and byte length are.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use pcbgen::boardfile::BoardFile;
use zeroize::Zeroizing;

use crate::{Link, Session};

/// The longest value the firmware stores (`PROV_MAX_VALUE`).
pub const MAX_VALUE: usize = 512;
/// The self-tests run before the console: a button test alone waits 10 s for a press.
const READY_TIMEOUT: Duration = Duration::from_secs(60);
const REPLY_TIMEOUT: Duration = Duration::from_secs(10);

/// A value on its way to the board; zeroed when dropped.
pub type Secret = Zeroizing<String>;

/// Sends every `[provision]` value and checks the board lists each with its length; returns
/// the keys and byte lengths stored. Every value is read before the board is touched, and none
/// is sent unless the banner shows this board and `target`.
pub fn run<L: Link>(bf: &BoardFile, s: &mut Session<L>, target: &str) -> Result<Vec<(String, usize)>> {
    if bf.provision.is_empty() {
        return Ok(vec![]);
    }
    for (key, date, days) in pcbgen::boardfile::expiries(bf, &pcbgen::schematic::today()) {
        if days < 0 {
            println!("   warning: {key} expired on {date} (board.toml); renew it before relying on the board");
        }
    }
    let mut values: Vec<(&str, Secret)> = vec![];
    for (key, reference) in &bf.provision {
        values.push((key, resolve(key, reference.reference())?));
    }

    let (_, problems) = crate::boot(bf, s, target)?;
    if !problems.is_empty() {
        bail!(
            "refusing to send anything: {} is not {} rev {} ({target}): {}",
            s.link.name(),
            bf.board.name,
            bf.board.revision,
            problems.join("; ")
        );
    }
    s.expect("PROV READY", READY_TIMEOUT)?;
    let mut sent = vec![];
    for (key, value) in &values {
        let n = value.len();
        let hex = hex(value.as_bytes());
        let mut line = Zeroizing::new(String::with_capacity(10 + key.len() + hex.len()));
        line.push_str("PROV SET ");
        line.push_str(key);
        line.push(' ');
        line.push_str(&hex);
        s.send(&line, &format!("PROV SET {key} ({n} bytes hidden)"))?;
        check_set(&s.expect("PROV ", REPLY_TIMEOUT)?, key, n)?;
        println!("   {key}: stored ({n} bytes)");
        sent.push((key.to_string(), n));
    }
    drop(values);

    s.send("PROV LIST", "PROV LIST")?;
    let mut listed = vec![];
    loop {
        let l = s.expect("PROV ", REPLY_TIMEOUT)?;
        if l == "PROV END" {
            break;
        }
        listed.push(l);
    }
    let problems = check_list(&listed, &sent);
    if !problems.is_empty() {
        bail!("the board's PROV LIST doesn't match what was sent: {}", problems.join("; "));
    }
    Ok(sent)
}

/// One `[provision]` reference's value. Errors name the key and the file, never the value.
pub fn resolve(key: &str, reference: &str) -> Result<Secret> {
    let value = if reference == "prompt" {
        prompt(key)?
    } else if let Some((path, field)) = reference.strip_prefix("file:").and_then(|r| r.rsplit_once('#')) {
        from_file(&expand_home(path, &std::env::var("HOME").unwrap_or_default()), field).with_context(|| format!("provision {key}"))?
    } else {
        bail!("provision {key}: {reference:?} must be \"prompt\" or \"file:<path>#<field>\"");
    };
    if value.len() > MAX_VALUE {
        bail!("provision {key}: the value is {} bytes; the firmware stores at most {MAX_VALUE}", value.len());
    }
    Ok(value)
}

/// `~` and `~/...` under `home`.
pub fn expand_home(path: &str, home: &str) -> PathBuf {
    match path.strip_prefix('~') {
        Some("") => PathBuf::from(home),
        Some(rest) if rest.starts_with('/') => PathBuf::from(format!("{home}{rest}")),
        _ => PathBuf::from(path),
    }
}

/// The value of the last `field=` line in `path`, trimmed: what Capture's
/// `sed -n "s/^field=//p" | tail -n1` reads.
pub fn from_file(path: &Path, field: &str) -> Result<Secret> {
    let text = Zeroizing::new(std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?);
    let prefix = format!("{field}=");
    match text.lines().rev().find_map(|l| l.strip_prefix(prefix.as_str())) {
        None => bail!("no `{field}=` line in {}", path.display()),
        Some(v) if v.trim().is_empty() => bail!("`{field}` is empty in {}", path.display()),
        Some(v) => Ok(Zeroizing::new(v.trim().to_string())),
    }
}

/// Reads a value typed at the terminal, not echoed.
fn prompt(key: &str) -> Result<Secret> {
    if std::fs::File::open("/dev/tty").is_err() {
        bail!("provision {key} is typed in (`prompt`), which needs a terminal: run this devctl provision command yourself in one");
    }
    let v = Zeroizing::new(rpassword::prompt_password(format!("{key} (typing is hidden): ")).context("reading from the terminal")?);
    if v.is_empty() {
        bail!("provision {key}: nothing typed");
    }
    Ok(v)
}

/// Lower-case hex, in a buffer zeroed when dropped.
pub fn hex(bytes: &[u8]) -> Zeroizing<String> {
    let mut s = Zeroizing::new(String::with_capacity(bytes.len() * 2));
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// The reply to `PROV SET <key>` must be exactly `PROV OK <key> <bytes>`.
pub fn check_set(reply: &str, key: &str, n: usize) -> Result<()> {
    let want = format!("PROV OK {key} {n}");
    if reply != want {
        bail!("{key}: the board answered `{reply}`, expected `{want}`");
    }
    Ok(())
}

/// Problems with a `PROV LIST` answer (its `PROV KEY` lines) against the keys and lengths sent.
pub fn check_list(listed: &[String], sent: &[(String, usize)]) -> Vec<String> {
    let mut problems = vec![];
    for (key, n) in sent {
        if listed.iter().any(|l| *l == format!("PROV KEY {key} {n}")) {
            continue;
        }
        match listed.iter().find(|l| l.split(' ').nth(2) == Some(key.as_str())) {
            Some(l) => problems.push(format!("{key} is listed as `{l}`, expected {n} bytes")),
            None => problems.push(format!("{key} is not listed")),
        }
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A file in the temp dir, removed when dropped.
    struct TempFile(PathBuf);
    impl TempFile {
        fn new(name: &str, text: &str) -> TempFile {
            let p = std::env::temp_dir().join(format!("devctl-test-{}-{name}", std::process::id()));
            std::fs::write(&p, text).unwrap();
            TempFile(p)
        }
    }
    impl Drop for TempFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    #[test]
    fn file_reference() {
        let f = TempFile::new(
            "config",
            "notes_dir=/x\napi_key=first\n api_key=indented\napi_key=  sk-a=b==c  \r\nother=1\nempty=\nblank=   \n",
        );
        let r = |field: &str| resolve("k", &format!("file:{}#{field}", f.0.display()));
        // the last `api_key=` line wins (the indented one isn't one); `=` inside a value stays
        assert_eq!(r("api_key").unwrap().as_str(), "sk-a=b==c");
        assert_eq!(r("other").unwrap().as_str(), "1");
        for (field, why) in [("missing", "no `missing=` line in"), ("empty", "`empty` is empty in"), ("blank", "`blank` is empty in")] {
            let e = format!("{:#}", r(field).unwrap_err());
            assert!(e.contains("provision k") && e.contains(why) && e.contains(&f.0.display().to_string()), "{e}");
        }
        // an error never shows a value, even the file's others
        let e = format!("{:#}", resolve("k", "file:/nonexistent/devctl#api_key").unwrap_err());
        assert!(e.contains("/nonexistent/devctl") && !e.contains("sk-a"), "{e}");
    }

    #[test]
    fn too_long() {
        let f = TempFile::new("long", &format!("big={}\nok={}\n", "x".repeat(MAX_VALUE + 1), "x".repeat(MAX_VALUE)));
        let e = format!("{:#}", resolve("k", &format!("file:{}#big", f.0.display())).unwrap_err());
        assert!(e.contains("513 bytes") && !e.contains("xxx"), "{e}");
        assert_eq!(resolve("k", &format!("file:{}#ok", f.0.display())).unwrap().len(), MAX_VALUE);
        assert!(resolve("k", "env:X").is_err());
    }

    #[test]
    fn home() {
        assert_eq!(expand_home("~/.config/x", "/home/u"), PathBuf::from("/home/u/.config/x"));
        assert_eq!(expand_home("~", "/home/u"), PathBuf::from("/home/u"));
        assert_eq!(expand_home("~other/x", "/home/u"), PathBuf::from("~other/x"));
        assert_eq!(expand_home("/etc/x", "/home/u"), PathBuf::from("/etc/x"));
    }

    #[test]
    fn hex_bytes() {
        assert_eq!(hex(b"sim").as_str(), "73696d");
        assert_eq!(hex(&[0, 0x0f, 0xff]).as_str(), "000fff");
        assert_eq!(hex("é".as_bytes()).as_str(), "c3a9");
        assert_eq!(hex(b"").as_str(), "");
    }

    #[test]
    fn replies() {
        assert!(check_set("PROV OK wifi_ssid 7", "wifi_ssid", 7).is_ok());
        for bad in ["PROV OK wifi_ssid 6", "PROV OK wifi 7", "PROV ERR wifi_ssid nvs", "PROV OK wifi_ssid 7 x", "PROV ERR line"] {
            assert!(check_set(bad, "wifi_ssid", 7).is_err(), "{bad}");
        }
        let listed: Vec<String> = ["PROV KEY a 3", "PROV KEY b 5", "PROV KEY other 1"].map(String::from).to_vec();
        assert!(check_list(&listed, &[("a".into(), 3), ("b".into(), 5)]).is_empty());
        let p = check_list(&listed, &[("a".into(), 4), ("c".into(), 1)]);
        assert_eq!(p, ["a is listed as `PROV KEY a 3`, expected 4 bytes", "c is not listed"]);
    }
}
