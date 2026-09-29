"""Minimal KiCad S-expression reader/writer.

Bare atoms parse to `Sym`, quoted strings to plain `str`, so a round trip keeps
the distinction KiCad cares about (keywords and numbers unquoted, text quoted).
"""

from __future__ import annotations

import re


class Sym(str):
    """An unquoted atom (keyword or number)."""


_TOKEN = re.compile(r'\s*(?:(\()|(\))|"((?:[^"\\]|\\.)*)"|([^\s()"]+))', re.S)


def parse(text: str) -> list:
    stack: list[list] = [[]]
    pos = 0
    n = len(text)
    while pos < n:
        m = _TOKEN.match(text, pos)
        if not m:
            if text[pos:].strip():
                raise ValueError(f"bad s-expression near offset {pos}: {text[pos:pos+40]!r}")
            break
        pos = m.end()
        lp, rp, qs, atom = m.groups()
        if lp:
            stack.append([])
        elif rp:
            done = stack.pop()
            stack[-1].append(done)
        elif qs is not None:
            stack[-1].append(qs.replace('\\"', '"').replace("\\n", "\n").replace("\\\\", "\\"))
        elif atom is not None:
            stack[-1].append(Sym(atom))
    if len(stack) != 1 or len(stack[0]) != 1:
        raise ValueError("unbalanced s-expression")
    return stack[0][0]


def fmt_num(v: float | int) -> str:
    if isinstance(v, bool):
        raise TypeError("bool is not a KiCad number")
    if isinstance(v, int):
        return str(v)
    s = f"{v:.4f}".rstrip("0").rstrip(".")
    return "0" if s in ("-0", "") else s


def _atom(v) -> str:
    if isinstance(v, Sym):
        return str(v)
    if isinstance(v, str):
        return '"' + v.replace("\\", "\\\\").replace('"', '\\"').replace("\n", "\\n") + '"'
    if isinstance(v, (int, float)):
        return fmt_num(v)
    raise TypeError(f"cannot serialise {type(v).__name__}: {v!r}")


def dumps(node, indent: int = 0) -> str:
    """Serialise with one child list per line (KiCad re-formats on upgrade anyway)."""
    if not isinstance(node, list):
        return _atom(node)
    pad = "\t" * indent
    i = 0
    while i < len(node) and not isinstance(node[i], list):
        i += 1
    head = "(" + " ".join(_atom(a) for a in node[:i])
    if i == len(node):
        return pad + head + ")"
    body = "\n".join(dumps(c, indent + 1) for c in node[i:])
    return pad + head + "\n" + body + "\n" + pad + ")"


def L(key: str, *items) -> list:
    """Build a node whose first element is a keyword."""
    return [Sym(key), *items]


def find(node: list, key: str) -> list | None:
    for c in node:
        if isinstance(c, list) and c and c[0] == key:
            return c
    return None


def find_all(node: list, key: str) -> list[list]:
    return [c for c in node if isinstance(c, list) and c and c[0] == key]


def yes(v: bool) -> Sym:
    return Sym("yes" if v else "no")
