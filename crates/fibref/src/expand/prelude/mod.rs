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
//! macros an expansion names (`and`, `or`) and the core forms (`if`,
//! `let`, `match`, `loop`, `recur`, `fn`, `do`, `.`) are plain, as are the
//! bindings a macro introduces itself, which are gensyms.
//!
//! The registry is [`MACROS`]: a macro is one row, its name and the
//! function that expands a call of it. [`PRELUDE_MACROS`] is the names
//! of the rows.

mod forms;
mod logic;
mod loops;

use crate::syntax::{Form, FormKind, Pos};

use super::ctx::ExpandCtx;
use super::derive;
use super::error::ExpandError;

/// What a prelude macro did with a call.
pub(crate) enum Outcome {
    /// The call's expansion.
    Expanded(Form),
    /// The call, unchanged: it is not a use of the macro (only
    /// `for-each` and the one-argument `range` decline, when they are
    /// the library functions, §4.4).
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

/// Expands one call of a prelude macro. A call whose head names no row
/// (`is_macro` never lets one through) is declined, unchanged.
pub(crate) fn expand(ctx: &ExpandCtx, form: Form) -> Answer {
    let pos = form.pos;
    let items = match form.kind {
        FormKind::List(items) => items,
        kind => return Ok(Outcome::Declined(Form::new(kind, pos))),
    };
    let name = items.first().and_then(Form::as_sym).unwrap_or("");
    match MACROS.iter().find(|(n, _)| *n == name) {
        Some((_, run)) => run(ctx, items, pos),
        None => Ok(Outcome::Declined(Form::new(FormKind::List(items), pos))),
    }
}
