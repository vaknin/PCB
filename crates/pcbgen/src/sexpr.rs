//! Minimal KiCad S-expression reader/writer.
//!
//! Bare atoms parse to `Sym` with their exact text, quoted strings to `Str`, so a round
//! trip keeps the distinction KiCad cares about (keywords and numbers unquoted, text
//! quoted) and never re-rounds a number it did not change.

use anyhow::{Result, bail};

#[derive(Clone, Debug, PartialEq)]
pub enum Sexp {
    List(Vec<Sexp>),
    /// An unquoted atom: keyword or number.
    Sym(String),
    /// A quoted string (unescaped).
    Str(String),
}

/// A keyword atom, for building nodes: `node!("hide", Kw("yes"))`.
pub struct Kw<'a>(pub &'a str);

/// Builds a list whose first element is a keyword: `node!("at", 1.0, 2.0)`.
#[macro_export]
macro_rules! node {
    ($key:expr $(, $item:expr)* $(,)?) => {
        $crate::sexpr::Sexp::List(vec![$crate::sexpr::Sexp::Sym(($key).to_string())
            $(, $crate::sexpr::Sexp::from($item))*])
    };
}

impl From<&str> for Sexp {
    fn from(s: &str) -> Self {
        Sexp::Str(s.to_string())
    }
}
impl From<String> for Sexp {
    fn from(s: String) -> Self {
        Sexp::Str(s)
    }
}
impl From<&String> for Sexp {
    fn from(s: &String) -> Self {
        Sexp::Str(s.clone())
    }
}
impl From<Kw<'_>> for Sexp {
    fn from(k: Kw<'_>) -> Self {
        Sexp::Sym(k.0.to_string())
    }
}
impl From<f64> for Sexp {
    fn from(v: f64) -> Self {
        Sexp::Sym(fmt_num(v))
    }
}
impl From<i64> for Sexp {
    fn from(v: i64) -> Self {
        Sexp::Sym(v.to_string())
    }
}
impl From<i32> for Sexp {
    fn from(v: i32) -> Self {
        Sexp::Sym(v.to_string())
    }
}
impl From<u32> for Sexp {
    fn from(v: u32) -> Self {
        Sexp::Sym(v.to_string())
    }
}
impl From<usize> for Sexp {
    fn from(v: usize) -> Self {
        Sexp::Sym(v.to_string())
    }
}
impl From<bool> for Sexp {
    /// KiCad's yes/no flags.
    fn from(v: bool) -> Self {
        Sexp::Sym(if v { "yes" } else { "no" }.to_string())
    }
}

/// A number as KiCad writes it: at most 6 decimals (1 nm), no trailing zeros, no "-0".
pub fn fmt_num(v: f64) -> String {
    let s = format!("{v:.6}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" || s.is_empty() { "0".to_string() } else { s.to_string() }
}

impl Sexp {
    pub fn items(&self) -> &[Sexp] {
        match self {
            Sexp::List(v) => v,
            _ => &[],
        }
    }

    pub fn items_mut(&mut self) -> &mut Vec<Sexp> {
        match self {
            Sexp::List(v) => v,
            _ => panic!("not a list: {self:?}"),
        }
    }

    /// Text of an atom (keyword, number or string).
    pub fn atom(&self) -> Option<&str> {
        match self {
            Sexp::Sym(s) | Sexp::Str(s) => Some(s),
            Sexp::List(_) => None,
        }
    }

    /// The keyword a list starts with.
    pub fn head(&self) -> Option<&str> {
        match self.items().first() {
            Some(Sexp::Sym(s)) => Some(s),
            _ => None,
        }
    }

    pub fn is(&self, key: &str) -> bool {
        self.head() == Some(key)
    }

    /// Atom text of item `i` (0 is the keyword).
    pub fn arg(&self, i: usize) -> Option<&str> {
        self.items().get(i).and_then(Sexp::atom)
    }

    /// Item `i` as a number; 0 if missing or not a number.
    pub fn num(&self, i: usize) -> f64 {
        self.arg(i).and_then(|s| s.parse().ok()).unwrap_or(0.0)
    }

    pub fn find(&self, key: &str) -> Option<&Sexp> {
        self.items().iter().find(|c| c.is(key))
    }

    pub fn find_mut(&mut self, key: &str) -> Option<&mut Sexp> {
        match self {
            Sexp::List(v) => v.iter_mut().find(|c| c.is(key)),
            _ => None,
        }
    }

    pub fn find_all<'a>(&'a self, key: &'a str) -> impl Iterator<Item = &'a Sexp> + 'a {
        self.items().iter().filter(move |c| c.is(key))
    }

    /// First argument of the child `(key value ...)`.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.find(key).and_then(|c| c.arg(1))
    }

    /// Does the list contain the bare keyword `flag` or `(flag yes)`?
    pub fn flag(&self, flag: &str) -> bool {
        self.items().iter().any(|c| matches!(c, Sexp::Sym(s) if s == flag))
            || self.find(flag).is_some_and(|c| c.arg(1).is_none_or(|v| v == "yes"))
    }

    pub fn push(&mut self, item: Sexp) {
        self.items_mut().push(item);
    }

    /// Replace the child `(key ...)` or append it.
    pub fn set(&mut self, item: Sexp) {
        let key = item.head().expect("set() needs a keyword node").to_string();
        match self.find_mut(&key) {
            Some(c) => *c = item,
            None => self.push(item),
        }
    }

    pub fn remove_all(&mut self, key: &str) {
        self.items_mut().retain(|c| !c.is(key));
    }

    /// Visit every list node, depth first.
    pub fn walk_mut(&mut self, f: &mut dyn FnMut(&mut Sexp)) {
        f(self);
        if let Sexp::List(v) = self {
            for c in v {
                c.walk_mut(f);
            }
        }
    }
}

pub fn parse(text: &str) -> Result<Sexp> {
    let b = text.as_bytes();
    let mut stack: Vec<Vec<Sexp>> = vec![vec![]];
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b' ' | b'\t' | b'\n' | b'\r' => i += 1,
            b'(' => {
                stack.push(vec![]);
                i += 1;
            }
            b')' => {
                let done = stack.pop().unwrap();
                match stack.last_mut() {
                    Some(parent) => parent.push(Sexp::List(done)),
                    None => bail!("unbalanced ')' at offset {i}"),
                }
                i += 1;
            }
            b'"' => {
                let mut s = Vec::new();
                i += 1;
                loop {
                    match b.get(i) {
                        None => bail!("unterminated string"),
                        Some(b'"') => break,
                        Some(b'\\') => {
                            match b.get(i + 1) {
                                Some(b'n') => s.push(b'\n'),
                                Some(b't') => s.push(b'\t'),
                                Some(&c @ (b'"' | b'\\')) => s.push(c),
                                Some(&c) => s.extend([b'\\', c]),
                                None => bail!("unterminated string"),
                            }
                            i += 2;
                        }
                        Some(&c) => {
                            s.push(c);
                            i += 1;
                        }
                    }
                }
                i += 1;
                stack.last_mut().unwrap().push(Sexp::Str(String::from_utf8(s)?));
            }
            _ => {
                let start = i;
                while i < b.len() && !matches!(b[i], b' ' | b'\t' | b'\n' | b'\r' | b'(' | b')' | b'"') {
                    i += 1;
                }
                stack.last_mut().unwrap().push(Sexp::Sym(text[start..i].to_string()));
            }
        }
    }
    if stack.len() != 1 || stack[0].len() != 1 {
        bail!("unbalanced s-expression");
    }
    Ok(stack.pop().unwrap().pop().unwrap())
}

fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Serialise with one child list per line (KiCad re-formats on upgrade anyway).
pub fn dumps(node: &Sexp) -> String {
    let mut out = String::new();
    write_node(node, 0, &mut out);
    out
}

fn write_node(node: &Sexp, indent: usize, out: &mut String) {
    let items = match node {
        Sexp::Sym(s) => return out.push_str(s),
        Sexp::Str(s) => return out.push_str(&quote(s)),
        Sexp::List(v) => v,
    };
    let pad = "\t".repeat(indent);
    out.push_str(&pad);
    out.push('(');
    let n_atoms = items.iter().take_while(|c| !matches!(c, Sexp::List(_))).count();
    for (k, a) in items[..n_atoms].iter().enumerate() {
        if k > 0 {
            out.push(' ');
        }
        write_node(a, 0, out);
    }
    if n_atoms == items.len() {
        out.push(')');
        return;
    }
    for c in &items[n_atoms..] {
        out.push('\n');
        match c {
            Sexp::List(_) => write_node(c, indent + 1, out),
            _ => {
                out.push_str(&pad);
                out.push('\t');
                write_node(c, 0, out);
            }
        }
    }
    out.push('\n');
    out.push_str(&pad);
    out.push(')');
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let text = r#"(a (b 1 2.50) "q \"x\" \\ y" (c) sym)"#;
        let t = parse(text).unwrap();
        assert_eq!(t.find("b").unwrap().arg(2), Some("2.50"));
        assert_eq!(t.items()[2], Sexp::Str("q \"x\" \\ y".into()));
        assert_eq!(parse(&dumps(&t)).unwrap(), t);
    }

    #[test]
    fn numbers() {
        assert_eq!(fmt_num(0.1 + 0.2), "0.3");
        assert_eq!(fmt_num(-0.0000001), "0");
        assert_eq!(fmt_num(146.5), "146.5");
        assert_eq!(fmt_num(90.0), "90");
    }
}
