//! Unit tests for the reader, split by what they cover.

mod atoms;
mod errors;
mod gen;
mod numbers;
mod positions;
mod roundtrip;
mod shorthand;
mod structure;
mod text;

use super::{read_all, Form, FormKind, ReadErrorKind};

/// Reads `src`, which must hold exactly one form.
fn one(src: &str) -> Form {
    let mut forms = read_all(src, "t.fib").unwrap_or_else(|e| panic!("{src:?}: {e}"));
    assert_eq!(forms.len(), 1, "{src:?} read as {forms:?}");
    forms.remove(0)
}

/// The kind of the single form in `src`.
fn kind(src: &str) -> FormKind {
    one(src).kind
}

/// Every form of `src`, printed and joined by spaces.
fn show(src: &str) -> String {
    let forms = read_all(src, "t.fib").unwrap_or_else(|e| panic!("{src:?}: {e}"));
    forms
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(" ")
}

/// The error kind of `src`, which must not read.
fn err(src: &str) -> ReadErrorKind {
    match read_all(src, "t.fib") {
        Ok(forms) => panic!("{src:?} read as {forms:?}"),
        Err(e) => e.kind,
    }
}

fn sym(name: &str) -> FormKind {
    FormKind::Sym(name.to_string())
}
