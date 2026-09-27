//! User macros (§3.16 `defmacro`): what the expander records for one,
//! and the [`MacroRunner`] interface through which an evaluator runs it.

use crate::syntax::{Form, Pos};

use super::build::malformed;
use super::ctx::ExpandCtx;
use super::error::{ExpandError, ExpandErrorKind};

/// A macro defined with `(defmacro name (param*) body)`, `param ::= sym
/// | ... sym`.
#[derive(Clone, Debug, PartialEq)]
pub struct MacroDef {
    /// The macro's name.
    pub name: String,
    /// The fixed parameters, each bound to one argument `Form`.
    pub params: Vec<String>,
    /// The rest parameter after `...`, bound to the remaining arguments
    /// as a `(Vec Form)`.
    pub rest: Option<String>,
    /// The body, already expanded: quasiquotes rewritten to
    /// `Form`-constructing calls, prelude macros expanded and literal
    /// collections rewritten (§1.4, §3.16). Evaluated as a `do`.
    pub body: Vec<Form>,
    /// Where the `defmacro` is.
    pub pos: Pos,
}

impl MacroDef {
    /// Whether the macro accepts `n` arguments.
    pub fn accepts(&self, n: usize) -> bool {
        match self.rest {
            Some(_) => n >= self.params.len(),
            None => n == self.params.len(),
        }
    }

    /// The accepted argument count, in words, for an arity error.
    pub fn arity_text(&self) -> String {
        match self.rest {
            Some(_) => format!("at least {}", self.params.len()),
            None => format!("{}", self.params.len()),
        }
    }
}

/// Runs a user macro's body at expansion time.
///
/// The expander has already checked the argument count against the
/// macro's parameters. The runner binds each of `macro_def.params` to
/// one of `args` and `macro_def.rest` (if any) to the remaining ones as
/// a `(Vec Form)`, evaluates `macro_def.body`, and returns the resulting
/// `Form`, which the expander expands again (outermost-first, §3.16).
/// Through `ctx` it answers `gensym` ([`ExpandCtx::gensym`]) and the
/// reflection calls ([`ExpandCtx::reflect`]), finds the macros defined
/// so far ([`ExpandCtx::macro_def`]), and gives the forms it builds from
/// nothing the call's position ([`ExpandCtx::call_pos`], §1.3). An
/// error in the body is returned as an [`ExpandError`].
pub trait MacroRunner {
    /// Expands one call of `macro_def` with the argument forms `args`.
    fn run(
        &mut self,
        macro_def: &MacroDef,
        args: Vec<Form>,
        ctx: &ExpandCtx,
    ) -> Result<Form, ExpandError>;
}

/// The runner to use until the evaluator exists: every user macro call
/// fails with [`ExpandErrorKind::MacroNeedsEvaluator`], which a caller
/// reports as *pending*, not as a failure.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoRunner;

impl MacroRunner for NoRunner {
    fn run(
        &mut self,
        macro_def: &MacroDef,
        _: Vec<Form>,
        ctx: &ExpandCtx,
    ) -> Result<Form, ExpandError> {
        let name = macro_def.name.clone();
        let kind = ExpandErrorKind::MacroNeedsEvaluator { name };
        Err(ExpandError::new(kind, ctx.call_pos()))
    }
}

/// Parses the parameter list of a `defmacro`.
pub(crate) fn parse_params(form: &Form) -> Result<(Vec<String>, Option<String>), ExpandError> {
    let Some(items) = form.as_list() else {
        return Err(malformed(
            "defmacro",
            "parameters must be a list",
            &form.pos,
        ));
    };
    let mut params = Vec::new();
    let mut i = 0;
    while i < items.len() {
        let Some(name) = items[i].as_sym() else {
            return Err(malformed(
                "defmacro",
                "a parameter must be a symbol",
                &items[i].pos,
            ));
        };
        if name != "..." {
            params.push(name.to_string());
            i += 1;
            continue;
        }
        let rest = items.get(i + 1).and_then(Form::as_sym);
        return match rest {
            Some(r) if i + 2 == items.len() && r != "..." => Ok((params, Some(r.to_string()))),
            _ => Err(malformed(
                "defmacro",
                "... must be followed by one last symbol",
                &items[i].pos,
            )),
        };
    }
    Ok((params, None))
}
