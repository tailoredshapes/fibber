//! The prelude macros of §4.4, as Rust rewrites.
//!
//! Each takes the whole call form and returns its expansion, which the
//! expander expands again. Forms a macro builds carry the call's
//! position; forms taken from the arguments keep their own (§1.3). The
//! expansions are unhygienic, as §3.16 decides: names such as `if`,
//! `cons`, `spawn`, `trap` are emitted as plain symbols, and only the
//! bindings a macro introduces itself are gensyms.

mod forms;
mod logic;
mod loops;

use crate::syntax::{Form, FormKind};

use super::ctx::ExpandCtx;
use super::derive;
use super::error::ExpandError;

/// The prelude macros, by name.
pub const PRELUDE_MACROS: [&str; 18] = [
    "when", "unless", "cond", "and", "or", "if-let", "when-let", "list", "plet", "while",
    "dotimes", "for-each", "->", "->>", "doto", "assert", "dbg", "derive",
];

/// What a prelude macro did with a call.
pub(crate) enum Outcome {
    /// The call's expansion.
    Expanded(Form),
    /// The call, unchanged: it is not a use of the macro (only
    /// `for-each` declines, when it is the library function, §4.4).
    Declined(Form),
}

/// Whether `name` is a prelude macro.
pub(crate) fn is_macro(name: &str) -> bool {
    PRELUDE_MACROS.contains(&name)
}

/// Expands one call of a prelude macro.
pub(crate) fn expand(ctx: &ExpandCtx, form: Form) -> Result<Outcome, ExpandError> {
    let pos = form.pos;
    let items = match form.kind {
        FormKind::List(items) => items,
        kind => return Ok(Outcome::Declined(Form::new(kind, pos))),
    };
    let name = items
        .first()
        .and_then(Form::as_sym)
        .unwrap_or("")
        .to_string();
    let p = &pos;
    let out = match name.as_str() {
        "when" => logic::when(items, p, false)?,
        "unless" => logic::when(items, p, true)?,
        "cond" => logic::cond(items, p)?,
        "and" => logic::and_or(items, p, true),
        "or" => logic::and_or(items, p, false),
        "if-let" => logic::if_let(items, p)?,
        "when-let" => logic::when_let(items, p)?,
        "list" => forms::list_macro(items, p),
        "plet" => forms::plet(ctx, items, p)?,
        "->" => forms::thread(items, p, true)?,
        "->>" => forms::thread(items, p, false)?,
        "doto" => forms::doto(ctx, items, p)?,
        "assert" | "dbg" => forms::assert(&name, items, p)?,
        "while" => loops::while_loop(items, p)?,
        "dotimes" => loops::dotimes(ctx, items, p)?,
        "for-each" => return loops::for_each(ctx, items, pos),
        _ => derive::derive(ctx, items, p)?,
    };
    Ok(Outcome::Expanded(out))
}
