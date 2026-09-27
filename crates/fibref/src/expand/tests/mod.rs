//! Unit tests for the expander, split by what they cover.

mod derive;
mod errors;
mod limits;
mod positions;
mod prelude;
mod quasi;
mod reflect;
mod walk;

use super::{expand_expr, expand_program, ExpandCtx, ExpandError, ExpandErrorKind, NoRunner};
use crate::syntax::{read_all, Form};

/// Reads `src`, which must read.
fn read(src: &str) -> Vec<Form> {
    read_all(src, "t.fib").unwrap_or_else(|e| panic!("{src:?}: {e}"))
}

/// Reads `src`, which must hold exactly one form.
fn one(src: &str) -> Form {
    let mut forms = read(src);
    assert_eq!(forms.len(), 1, "{src:?}");
    forms.remove(0)
}

/// Expands the program `src` in a fresh context.
fn program(src: &str) -> Result<Vec<Form>, ExpandError> {
    expand_program(read(src), &mut ExpandCtx::new(), &mut NoRunner)
}

/// Expands the program `src`, which must succeed, and prints each form.
fn prog(src: &str) -> Vec<String> {
    let forms = program(src).unwrap_or_else(|e| panic!("{src:?}: {e}"));
    forms.iter().map(|f| f.to_string()).collect()
}

/// Expands the single expression `src`, which must succeed, and prints
/// the result.
fn ex(src: &str) -> String {
    let form = expand_expr(one(src), &mut ExpandCtx::new(), &mut NoRunner)
        .unwrap_or_else(|e| panic!("{src:?}: {e}"));
    form.to_string()
}

/// Expands the single expression `src`, which must fail.
fn ex_err(src: &str) -> ExpandError {
    match expand_expr(one(src), &mut ExpandCtx::new(), &mut NoRunner) {
        Ok(f) => panic!("{src:?} expanded to {f}"),
        Err(e) => e,
    }
}

/// Expands the program `src`, which must fail, and returns the kind.
fn prog_err(src: &str) -> ExpandErrorKind {
    match program(src) {
        Ok(forms) => panic!("{src:?} expanded to {forms:?}"),
        Err(e) => e.kind,
    }
}

/// `[..]` of the §1.4 rewrite, for writing expectations.
fn v(items: &[&str]) -> String {
    let mut acc = "(fib.prelude/vec-empty)".to_string();
    for i in items {
        acc = format!("(fib.prelude/conj {acc} {i})");
    }
    acc
}
