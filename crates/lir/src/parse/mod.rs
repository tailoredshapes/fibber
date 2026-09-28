//! Forms to the AST: the grammar of spec/lir.md §2 to §7.

mod atomics;
mod control;
mod expr;
mod instr;
mod items;
mod literal;
mod memory;
mod names;
mod ty;

pub use items::module;
pub use names::{is_reserved, valid_name};

use crate::diag::{err, Pos, Result};
use crate::sexp::Sexp;

/// `op expects N operands, found M` unless `args` has exactly `n`.
fn arity(op: &str, args: &[Sexp], n: usize, pos: Pos) -> Result<()> {
    if args.len() == n {
        Ok(())
    } else {
        let s = if n == 1 { "" } else { "s" };
        err(
            pos,
            format!("{op} expects {n} operand{s}, found {}", args.len()),
        )
    }
}

/// The atom's text, or an error naming what was expected.
fn atom<'a>(s: &'a Sexp, what: &str) -> Result<&'a str> {
    match s {
        Sexp::Atom(a, _) => Ok(a),
        other => err(
            other.pos(),
            format!("expected {what}, found {}", other.describe()),
        ),
    }
}

/// A label: an atom that is a valid name.
fn label(s: &Sexp) -> Result<String> {
    let a = atom(s, "a block label")?;
    if a.starts_with('@') || a.starts_with('%') || a.is_empty() {
        return err(s.pos(), format!("invalid label {a}"));
    }
    Ok(a.to_string())
}
