//! `str`, `println`, `print`, `prn`, `pr` (stdlib §2.7, §2.10, §4.14;
//! tranche 1 R5): the printing macros that every program uses, so Rust
//! rewrites (§6.3). Each has a function twin in `fib.print` (L11) for
//! value position, `(map str xs)`: a macro expands only in head
//! position, so the two coexist.
//!
//! Decided here (the plan's R5 text; the page where it is silent):
//! - `(str)` is `""`. `(str a b ..)` is a right fold of
//!   `fib.prelude/str-concat` over the pieces, a piece being a string
//!   literal as it is, a literal `nil` as `""` (the macro sees it, the bare
//!   `nil` has no type, §2.7) and anything else `(fib.core/to-str x)`;
//!   `(str a)` is that piece alone, so `(str a)` is `(fib.core/to-str a)`
//!   for any `a` that is not a literal.
//! - `(println a b ..)` is `(fib.prelude/println S)` and `(print a b ..)`
//!   `(fib.prelude/print-raw S)`, `S` the fold of the pieces joined by one
//!   space, a piece being a literal `nil` as `"nil"` (the page's table:
//!   `(println nil)` prints `nil`) and anything else
//!   `(fib.prelude/show x)`, a string literal included (the plan's text
//!   has no exception for it, and the free trace of case 614 counts the
//!   copy `show` of a string makes). `(println)` and `(print)` write
//!   `""`.
//! - `prn` and `pr` are `println` and `print` over `Debug`: a piece is a
//!   literal `nil` as `"nil"` and anything else `(fib.core/debug x)`
//!   (a string literal included: the text is quoted).
//! - The `fib.core/` heads are the qualified names of §6.2: they resolve
//!   in a module that has `fib.core` in scope (a `:use`, or the implicit
//!   list once it is filled); the other heads are `fib.prelude/NAME`.
//!   Generated forms carry the call's position, the arguments their own.

use crate::syntax::{Form, FormKind, Pos};

use crate::expand::build::{call, string};
use crate::expand::collections::prelude_name;

/// Which of the four printers a macro is.
#[derive(Clone, Copy)]
pub(super) enum Printer {
    /// `println`: `Show`, then a newline.
    Println,
    /// `print`: `Show`, no newline.
    Print,
    /// `prn`: `Debug`, then a newline.
    Prn,
    /// `pr`: `Debug`, no newline.
    Pr,
}

/// A string literal, as `str` takes it whole.
fn is_string(form: &Form) -> bool {
    matches!(form.kind, FormKind::Str(_))
}

/// `a` then `b` as one text: `(fib.prelude/str-concat a b)`.
pub(super) fn concat(a: Form, b: Form, pos: &Pos) -> Form {
    call(&prelude_name("str-concat"), vec![a, b], pos)
}

/// The pieces as one text, `sep` between two (none for `str`): a right
/// fold, so `(a b c)` is `(str-concat a (str-concat sep (str-concat b
/// c)))`. No pieces is `""`.
pub(super) fn join(pieces: Vec<Form>, sep: Option<&str>, pos: &Pos) -> Form {
    let mut it = pieces.into_iter().rev();
    let Some(mut acc) = it.next() else {
        return string("", pos);
    };
    for piece in it {
        if let Some(sep) = sep {
            acc = concat(string(sep, pos), acc, pos);
        }
        acc = concat(piece, acc, pos);
    }
    acc
}

/// `(str a b ..)`: the pieces of the call's arguments, joined.
pub(super) fn str_macro(items: Vec<Form>, pos: &Pos) -> Form {
    let pieces = items
        .into_iter()
        .skip(1)
        .map(|x| match x.kind {
            FormKind::Nil => string("", pos),
            _ if is_string(&x) => x,
            _ => call("fib.core/to-str", vec![x], pos),
        })
        .collect();
    join(pieces, None, pos)
}

/// The text of each argument of the call: a literal `nil` is the word,
/// anything else its `Show` (`Debug` when `debug`, a string literal
/// quoted then).
pub(super) fn pieces(items: Vec<Form>, pos: &Pos, debug: bool) -> Vec<Form> {
    items
        .into_iter()
        .skip(1)
        .map(|x| match x.kind {
            FormKind::Nil => string("nil", pos),
            _ if debug => call("fib.core/debug", vec![x], pos),
            _ => call(&prelude_name("show"), vec![x], pos),
        })
        .collect()
}

/// `(println a b ..)`, `(print ..)`, `(prn ..)`, `(pr ..)`: one write of
/// the arguments' texts joined by a space. The write without a newline is
/// `fib.prelude/print-raw` (`print-str` is the macro of `text.rs` now).
pub(super) fn printer(items: Vec<Form>, pos: &Pos, which: Printer) -> Form {
    let (write, debug) = match which {
        Printer::Println => ("println", false),
        Printer::Print => ("print-raw", false),
        Printer::Prn => ("println", true),
        Printer::Pr => ("print-raw", true),
    };
    let pieces = pieces(items, pos, debug);
    call(
        &prelude_name(write),
        vec![join(pieces, Some(" "), pos)],
        pos,
    )
}
