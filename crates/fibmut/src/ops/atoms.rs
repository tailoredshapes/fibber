//! The operators that change a symbol or a literal in place: `cmp`,
//! `arith`, `bool` and `const`.

use super::Mutant;
use crate::sexp::{Kind, Node};

fn edit(op: &'static str, at: &Node, text: &str) -> Mutant {
    Mutant {
        op,
        start: at.start,
        end: at.end,
        text: text.to_string(),
    }
}

/// Replaces the head of a list by `table`'s answer for it.
fn rename_head(
    op: &'static str,
    src: &str,
    n: &Node,
    out: &mut Vec<Mutant>,
    table: fn(&str) -> Option<&'static str>,
) {
    if let (Some(head), Some(first)) = (n.head(src), n.kids.first()) {
        if let Some(to) = table(head) {
            out.push(edit(op, first, to));
        }
    }
}

/// `<` and `<=`, `>` and `>=`, `=` and `!=`.
pub fn cmp(src: &str, n: &Node, out: &mut Vec<Mutant>) {
    rename_head("cmp", src, n, out, |h| match h {
        "<" => Some("<="),
        "<=" => Some("<"),
        ">" => Some(">="),
        ">=" => Some(">"),
        "=" => Some("!="),
        "!=" => Some("="),
        _ => None,
    });
}

/// `+` to `-`, `-` to `+`, `*` to `+`.
pub fn arith(src: &str, n: &Node, out: &mut Vec<Mutant>) {
    rename_head("arith", src, n, out, |h| match h {
        "+" => Some("-"),
        "-" | "*" => Some("+"),
        _ => None,
    });
}

/// `true` and `false`, `and` and `or`, and `(not x)` to `x`.
pub fn boolean(src: &str, n: &Node, out: &mut Vec<Mutant>) {
    match (n.kind, n.atom(src)) {
        (Kind::Atom, Some("true")) => out.push(edit("bool", n, "false")),
        (Kind::Atom, Some("false")) => out.push(edit("bool", n, "true")),
        _ => {}
    }
    rename_head("bool", src, n, out, |h| match h {
        "and" => Some("or"),
        "or" => Some("and"),
        _ => None,
    });
    if n.head(src) == Some("not") && n.kids.len() == 2 {
        let inner = n.kids[1].text(src);
        out.push(edit("bool", n, inner));
    }
}

/// An integer literal as the splitter's atoms spell it: the value and the
/// width suffix (`i8`, `i16`, `i32`, `i64`, or none).
fn parse_int(text: &str) -> Option<(i128, &str, u32)> {
    let (body, suffix, bits) = ["i8", "i16", "i32", "i64"]
        .iter()
        .find_map(|s| {
            let body = text.strip_suffix(s)?;
            // `0x1i8` is hex digits, then a suffix: no hex digit is `i`.
            Some((body, *s, s[1..].parse::<u32>().ok()?))
        })
        .unwrap_or((text, "", 64));
    let (negative, digits) = match body.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, body),
    };
    if !digits.chars().next()?.is_ascii_digit() {
        return None;
    }
    let (radix, digits) = match digits.get(..2) {
        Some("0x") => (16, &digits[2..]),
        Some("0b") => (2, &digits[2..]),
        _ => (10, digits),
    };
    let clean: String = digits.chars().filter(|&c| c != '_').collect();
    let value = i128::from_str_radix(&clean, radix).ok()?;
    Some((if negative { -value } else { value }, suffix, bits))
}

/// An integer literal `n` to `n+1`, `n-1` and `0`, in its own width.
pub fn constant(src: &str, n: &Node, out: &mut Vec<Mutant>) {
    let Some(text) = n.atom(src) else { return };
    let Some((value, suffix, bits)) = parse_int(text) else {
        return;
    };
    let (low, high) = (-(1i128 << (bits - 1)), (1i128 << (bits - 1)) - 1);
    let mut seen = Vec::new();
    for candidate in [value + 1, value - 1, 0] {
        if candidate < low || candidate > high || candidate == value || seen.contains(&candidate) {
            continue;
        }
        seen.push(candidate);
        out.push(edit("const", n, &format!("{candidate}{suffix}")));
    }
}
