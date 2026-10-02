//! The prelude macros of §4.4, as Rust rewrites.
//!
//! Each takes the whole call form and returns its expansion, which the
//! expander expands again. Forms a macro builds carry the call's
//! position; forms taken from the arguments keep their own (§1.3).
//!
//! The expansions are hygienic in the one way a Rust rewrite can be
//! (stdlib design §6.3, tranche 1 R2): every head or constant an
//! expansion emits that is not a core form is written
//! `fib.prelude/NAME` ([`prelude_name`](super::collections::prelude_name)), which a binding of the program's
//! or a library's own `cons`, `trap`, `show`, `+` cannot capture. The
//! macros an expansion names (`and`, `or`) are `fib.prelude/and` and
//! `fib.prelude/or` as well (R14); the core forms (`if`, `let`, `match`,
//! `loop`, `recur`, `fn`, `do`, `.`) are plain, as are the bindings a
//! macro introduces itself, which are gensyms.
//!
//! A module's own definitions beat the macros (R14): `macro_of` says which
//! macro a call's head reaches in the module being expanded, a bare name
//! only when the module does not define it, `fib.prelude/NAME` always.
//!
//! The registry is [`MACROS`]: a macro is one row, its name and the
//! function that expands a call of it. [`PRELUDE_MACROS`] is the names
//! of the rows.

mod atoms;
mod colls;
mod defn;
mod fold;
mod forms;
mod logic;
mod loops;
mod print;
mod reduce;
mod update;

use crate::syntax::{Form, FormKind, Pos};

use super::ctx::ExpandCtx;
use super::derive;
use super::error::ExpandError;

/// What a prelude macro did with a call.
pub(crate) enum Outcome {
    /// The call's expansion.
    Expanded(Form),
    /// The call, unchanged: it is not a use of the macro (only
    /// `for-each`, the one-argument `range`, `update` and `reduce`
    /// decline, when they are the library functions, §4.4; and the
    /// operators and collection functions of R6a, `+ < max conj assoc
    /// merge swap!` and the rest, when the call is the binary one the
    /// builtin or the function serves). A macro that the module hides (it
    /// defines the name itself, `ExpandCtx::hides_macro`) is not asked.
    Declined(Form),
}

/// What a macro's function answers.
type Answer = Result<Outcome, ExpandError>;

/// A prelude macro: the context, the call's items (the head first) and
/// the call's position.
type MacroFn = fn(&ExpandCtx, Vec<Form>, Pos) -> Answer;

/// `Ok(Expanded(form))` for a macro that always expands.
fn expanded(form: Result<Form, ExpandError>) -> Answer {
    form.map(Outcome::Expanded)
}

/// `Ok(Expanded(form))` for one of the printing macros (R5).
fn printing(items: Vec<Form>, pos: &Pos, which: print::Printer) -> Answer {
    expanded(Ok(print::printer(items, pos, which)))
}

/// The registry, in the order of the table of §4.4: one row per macro.
const MACROS: &[(&str, MacroFn)] = &[
    ("when", |_, i, p| expanded(logic::when(i, &p, false))),
    ("unless", |_, i, p| expanded(logic::when(i, &p, true))),
    ("cond", |_, i, p| expanded(logic::cond(i, &p))),
    ("and", |_, i, p| expanded(Ok(logic::and_or(i, &p, true)))),
    ("or", |_, i, p| expanded(Ok(logic::and_or(i, &p, false)))),
    ("if-let", |_, i, p| expanded(logic::if_let(i, &p))),
    ("when-let", |_, i, p| expanded(logic::when_let(i, &p))),
    ("list", |_, i, p| expanded(Ok(forms::list_macro(i, &p)))),
    ("plet", |c, i, p| expanded(forms::plet(c, i, &p))),
    ("while", |_, i, p| expanded(loops::while_loop(i, &p))),
    ("dotimes", |c, i, p| expanded(loops::dotimes(c, i, &p))),
    ("for-each", |c, i, p| loops::for_each(c, i, p)),
    ("range", |_, i, p| loops::range(i, p)),
    ("->", |_, i, p| expanded(forms::thread(i, &p, true))),
    ("->>", |_, i, p| expanded(forms::thread(i, &p, false))),
    ("doto", |c, i, p| expanded(forms::doto(c, i, &p))),
    ("assert", |_, i, p| expanded(forms::assert(i, &p))),
    ("dbg", |c, i, p| expanded(forms::dbg(c, i, &p))),
    ("derive", |c, i, p| expanded(derive::derive(c, i, &p))),
    ("defn", |_, i, p| expanded(defn::defn(i, &p, false))),
    ("defn-", |_, i, p| expanded(defn::defn(i, &p, true))),
    ("update", |c, i, p| update::update(c, i, p)),
    ("reduce", |c, i, p| reduce::reduce(c, i, p)),
    ("str", |_, i, p| expanded(Ok(print::str_macro(i, &p)))),
    ("println", |_, i, p| {
        printing(i, &p, print::Printer::Println)
    }),
    ("print", |_, i, p| printing(i, &p, print::Printer::Print)),
    ("prn", |_, i, p| printing(i, &p, print::Printer::Prn)),
    ("pr", |_, i, p| printing(i, &p, print::Printer::Pr)),
    ("+", |_, i, p| Ok(fold::sum_or_product(i, &p, "+", 0))),
    ("-", |_, i, p| fold::difference(i, &p)),
    ("*", |_, i, p| Ok(fold::sum_or_product(i, &p, "*", 1))),
    ("<", |c, i, p| Ok(fold::comparison(c, i, &p, "<"))),
    (">", |c, i, p| Ok(fold::comparison(c, i, &p, ">"))),
    ("<=", |c, i, p| Ok(fold::comparison(c, i, &p, "<="))),
    (">=", |c, i, p| Ok(fold::comparison(c, i, &p, ">="))),
    ("=", |c, i, p| Ok(fold::comparison(c, i, &p, "="))),
    ("max", |_, i, p| Ok(fold::extremum(i, &p, "max"))),
    ("min", |_, i, p| Ok(fold::extremum(i, &p, "min"))),
    ("bit-and", |_, i, p| Ok(fold::bits(i, &p, "bit-and"))),
    ("bit-or", |_, i, p| Ok(fold::bits(i, &p, "bit-or"))),
    ("bit-xor", |_, i, p| Ok(fold::bits(i, &p, "bit-xor"))),
    ("conj", |_, i, p| Ok(colls::conj(i, &p))),
    ("assoc", |_, i, p| colls::assoc(i, &p)),
    ("dissoc", |_, i, p| Ok(colls::dissoc(i, &p))),
    ("merge", |_, i, p| colls::merge(i, &p)),
    ("swap!", |c, i, p| Ok(atoms::swap(c, i, &p))),
];

/// The names of the registry's rows: the prelude macros, by name.
pub const PRELUDE_MACROS: [&str; MACROS.len()] = macro_names();

const fn macro_names() -> [&'static str; MACROS.len()] {
    let mut names = [""; MACROS.len()];
    let mut i = 0;
    while i < MACROS.len() {
        names[i] = MACROS[i].0;
        i += 1;
    }
    names
}

/// Whether `name` is a prelude macro.
pub(crate) fn is_macro(name: &str) -> bool {
    PRELUDE_MACROS.contains(&name)
}

/// The one macro whose name, written `fib.prelude/NAME`, is the prelude's
/// function and not the macro: the expansion of the printing macros
/// (R5) calls `fib.prelude/println`, which the macro `println` would
/// expand again for ever. Every other expansion's `fib.prelude/NAME` that
/// is also a macro's name (`+ < = swap!`..) is a call the macro declines.
const FUNCTION_TWINS: [&str; 1] = ["println"];

/// The prelude macro that the head `name` of a call reaches in the module
/// `ctx` is expanding (R14, stdlib §6.3; the module's own definitions
/// come first, `ExpandCtx::hides_macro`):
///
/// - a bare name that is a prelude macro the module does not define
///   itself or see a program module export;
/// - `fib.prelude/NAME`, which is the prelude's macro NAME whatever the
///   module defines, so the expansions that name `fib.prelude/and` reach
///   the macro even in a module with its own `and`. A call the macro
///   declines (the binary `fib.prelude/+`) stays that call, and
///   `fib.prelude/println` is the prelude's function ([`FUNCTION_TWINS`]).
///
/// The row's own name is the answer, without the qualifier.
pub(crate) fn macro_of<'a>(ctx: &ExpandCtx, name: &'a str) -> Option<&'a str> {
    match name.strip_prefix(QUALIFIER) {
        Some(base) => (is_macro(base) && !FUNCTION_TWINS.contains(&base)).then_some(base),
        None => (is_macro(name) && !ctx.hides_macro(name)).then_some(name),
    }
}

/// `fib.prelude/`, which a head names the prelude's own with.
const QUALIFIER: &str = "fib.prelude/";

/// Whether the head `name` is written `fib.prelude/NAME`.
pub(crate) fn is_qualified(name: &str) -> bool {
    name.starts_with(QUALIFIER)
}

/// Expands one call of a prelude macro, named by its row or qualified. A
/// call whose head names no row (`macro_of` never lets one through) is
/// declined, unchanged.
pub(crate) fn expand(ctx: &ExpandCtx, form: Form) -> Answer {
    let pos = form.pos;
    let items = match form.kind {
        FormKind::List(items) => items,
        kind => return Ok(Outcome::Declined(Form::new(kind, pos))),
    };
    let head = items.first().and_then(Form::as_sym).unwrap_or("");
    let name = head.strip_prefix(QUALIFIER).unwrap_or(head);
    match MACROS.iter().find(|(n, _)| *n == name) {
        Some((_, run)) => run(ctx, items, pos),
        None => Ok(Outcome::Declined(Form::new(FormKind::List(items), pos))),
    }
}
