//! The fibber macro expander: reader [`Form`]s to a fully expanded
//! program (spec/syntax.md §1.4, §2, §3.16, §4).
//!
//! [`expand_program`] takes the top-level forms of one module and
//! returns them expanded, in order: a top-level `(do ..)` is spliced
//! (§2), `defstruct`/`defenum` are registered before the next form is
//! expanded (§3.16), `defmacro` records a [`MacroDef`] and stays in the
//! program with its body expanded, and every other definition has its
//! expression positions expanded. What remains is core forms (§4.2)
//! and calls, nothing else:
//!
//! - macros expand outermost-first, repeatedly, until none remains
//!   (§3.16): user macros through a [`MacroRunner`] (which shadow a
//!   prelude macro of the same name), the prelude macros of §4.4 by the
//!   Rust rewrites in `prelude` and `derive`;
//! - `quasiquote` is rewritten to `Form`-constructing calls (§3.16);
//! - `[..]` and `{..}` in expression position become calls of
//!   `fib.prelude/vec-empty`, `conj`, `map-empty`, `assoc` (§1.4);
//!   inside `quote` they stay data;
//! - the symbol `nil` becomes the form `(Nil)` in expressions and
//!   patterns (§3.9);
//! - the non-expression operands of the primitive forms `set-field!`,
//!   `dyn` and the conversions (§4.3), and the names, parameter lists,
//!   types and patterns of core forms, are not expanded.
//!
//! Positions follow §1.3: a form a macro builds carries the position of
//! the macro call; a form taken from the input keeps its own. Errors are
//! [`ExpandError`] values with positions, never panics. Expansion always
//! terminates: see [`Limits`].
//!
//! `gensym` names start with `#`, which no reader symbol can
//! ([`ExpandCtx::gensym`]).
//!
//! Choices where the spec is silent: `cond` takes `(test body+)` clauses
//! with an optional final `else`/`:else` clause and traps when none
//! matches; `dbg` is `assert` with its own message (§4.4 gives it only
//! "`if` and `trap`"); the derived `Ord`, `Hash` and `Show` bodies are
//! described in `derive`.

mod build;
mod collections;
mod core;
mod ctx;
mod derive;
mod error;
mod expr;
mod heads;
mod prelude;
mod quasi;
mod reflect;
mod runner;
mod top;
mod types;
mod walk;

#[cfg(test)]
mod tests;

use crate::syntax::{read_all, Form};

pub use collections::PRELUDE_NS;
pub use ctx::{ExpandCtx, Limits, MAX_EXPAND_DEPTH, MAX_STEPS};
pub use error::{ExpandError, ExpandErrorKind};
pub use heads::{is_core, CORE_FORMS};
pub use prelude::PRELUDE_MACROS;
pub use reflect::REFLECTION_CALLS;
pub use runner::{MacroDef, MacroRunner, NoRunner};
pub use types::{EnumInfo, StructInfo, VariantInfo};

use expr::Expander;

/// Expands the top-level forms of one module, in order, into `ctx`.
/// Stops at the first error.
pub fn expand_program(
    forms: Vec<Form>,
    ctx: &mut ExpandCtx,
    runner: &mut dyn MacroRunner,
) -> Result<Vec<Form>, ExpandError> {
    let mut ex = Expander::new(ctx, runner);
    let mut out = Vec::new();
    for form in forms {
        ex.ctx.steps = 0;
        top::top_form(&mut ex, form, &mut out)?;
    }
    Ok(out)
}

/// Expands one form in expression position (for a REPL or a test).
pub fn expand_expr(
    form: Form,
    ctx: &mut ExpandCtx,
    runner: &mut dyn MacroRunner,
) -> Result<Form, ExpandError> {
    ctx.steps = 0;
    let mut ex = Expander::new(ctx, runner);
    walk::expr(&mut ex, form)
}

/// The prelude forms the expander itself needs (§4.4, §4.5): the `List`
/// enum, and `derive` of `Eq`, `Ord`, `Hash` and `Show` for `Option`
/// and `List`. The rest of the prelude is library code, not here.
pub const PRELUDE_SOURCE: &str = "\
(defenum (List a) (empty) (cons head: a tail: (List a)))
(derive Eq Option) (derive Ord Option) (derive Hash Option) (derive Show Option)
(derive Eq List) (derive Ord List) (derive Hash List) (derive Show List)
";

/// Reads and expands [`PRELUDE_SOURCE`] into `ctx`, registering `List`,
/// and returns the expanded forms (the `defenum` and eight `impl`s).
pub fn expand_prelude(ctx: &mut ExpandCtx) -> Result<Vec<Form>, ExpandError> {
    // PRELUDE_SOURCE is a constant that the unit test
    // `prelude_expands` reads, so this cannot fail.
    let forms = read_all(PRELUDE_SOURCE, "<prelude>").expect("PRELUDE_SOURCE reads");
    expand_program(forms, ctx, &mut NoRunner)
}
