//! The printer: `Display` for forms, in the syntax of spec/syntax.md §1.
//!
//! Reading the output of `Display` gives back an equal form for every
//! form the reader can produce (a property test in `tests/roundtrip.rs`
//! checks this). Prefix-macro lists print in their long spelling, so
//! `'x` prints as `(quote x)` and `&x` as `(& x)`; both read back equal.
//! Integers print their suffix unless `i64`, floats their suffix unless
//! `f64`, in the shortest spelling that reads back to the same bits.
//! A symbol or keyword the reader could not have produced (built by
//! hand, e.g. `Sym("a b")`) prints as is and will not read back.

use std::fmt::{self, Write};

use super::chars::is_forbidden;
use super::form::{FltWidth, Form, FormKind, IntWidth};

impl fmt::Display for Form {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.kind)
    }
}

impl fmt::Display for FormKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FormKind::Sym(name) => f.write_str(name),
            FormKind::Kw(name) => write!(f, ":{name}"),
            FormKind::Int { v, width } => match width {
                IntWidth::I64 => write!(f, "{v}"),
                _ => write!(f, "{v}{}", width.suffix()),
            },
            FormKind::Flt { v, width } => match width {
                FltWidth::F64 => write!(f, "{v:?}"),
                // Exact: an F32 form holds an f32 value widened to f64.
                FltWidth::F32 => write!(f, "{:?}f32", *v as f32),
            },
            FormKind::Str(s) => write_str_lit(f, s),
            FormKind::Chr(c) => write_chr_lit(f, *c),
            FormKind::Bool(b) => write!(f, "{b}"),
            FormKind::Nil => f.write_str("nil"),
            FormKind::List(items) => write_seq(f, '(', items, ')'),
            FormKind::Vec(items) => write_seq(f, '[', items, ']'),
            FormKind::Map(items) => write_seq(f, '{', items, '}'),
        }
    }
}

fn write_seq(f: &mut fmt::Formatter<'_>, open: char, items: &[Form], close: char) -> fmt::Result {
    f.write_char(open)?;
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            f.write_char(' ')?;
        }
        write!(f, "{item}")?;
    }
    f.write_char(close)
}

fn write_str_lit(f: &mut fmt::Formatter<'_>, s: &str) -> fmt::Result {
    f.write_char('"')?;
    for c in s.chars() {
        match c {
            '"' => f.write_str("\\\"")?,
            '\\' => f.write_str("\\\\")?,
            '\n' => f.write_str("\\n")?,
            '\t' => f.write_str("\\t")?,
            '\r' => f.write_str("\\r")?,
            '\0' => f.write_str("\\0")?,
            c if is_forbidden(c) => write!(f, "\\u{{{:X}}}", u32::from(c))?,
            c => f.write_char(c)?,
        }
    }
    f.write_char('"')
}

fn write_chr_lit(f: &mut fmt::Formatter<'_>, c: char) -> fmt::Result {
    match c {
        '\n' => f.write_str("\\newline"),
        ' ' => f.write_str("\\space"),
        '\t' => f.write_str("\\tab"),
        '\r' => f.write_str("\\return"),
        c if is_forbidden(c) => write!(f, "\\u{{{:X}}}", u32::from(c)),
        c => write!(f, "\\{c}"),
    }
}
