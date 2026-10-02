//! The output of the checker: [`TypedProgram`], what the ownership pass
//! (types §6) and the interpreter consume.
//!
//! # Reading a typed program
//!
//! **Definitions** are in [`TypedProgram::globals`]: every `defun`
//! ([`Globals::funs`], indexed by [`FunId`]; `defmacro`s are there too
//! with `is_macro` set), `def` ([`Globals::defs`]), struct and enum
//! ([`Globals::types`]), protocol ([`Globals::protos`], each method's
//! parameters with their declared `:borrow`/`:owned`), instance
//! ([`Globals::instances`], with method bodies), `extern`, and every
//! binding site ([`Globals::bindings`], indexed by [`BindingId`], with
//! its kind: parameter, `&` parameter, `let`, pattern, `loop`, a named
//! `fn`'s self-name). Bodies are the resolved AST of [`super::ast`]:
//! every name is a [`BindingId`] or a [`GlobalRef`], every expression
//! has an [`ExprId`], every `fn` and `async` carries its capture set.
//! The prelude's definitions are included; each says its module.
//!
//! **Types** are keyed by id:
//!
//! - [`expr_types`](TypedProgram::expr_types): every expression that
//!   was typed. `Ty::Gen(i)` in the type of an expression inside a
//!   `defun` is the `i`th quantified variable of that `defun`'s scheme
//!   ([`fun_schemes`](TypedProgram::fun_schemes)); inside an `impl`
//!   method body it is the `i`th variable of the instance
//!   ([`InstanceDef::var_names`](super::decls::InstanceDef)), then the
//!   method signature's own variables; inside a `def` there is none.
//!   A variable that no scheme quantifies is `i64` (§3.6). There are no
//!   unification variables, rigid variables or unsolved colour
//!   variables left (a unit test checks this on the cases).
//! - [`binding_types`](TypedProgram::binding_types): every binding
//!   site; an `&` parameter's type is `(Cell T)` (§2.14).
//! - [`fun_schemes`](TypedProgram::fun_schemes), by [`FunId`]; the
//!   scheme's `amps` marks `&` positions, `preds` its bounds, and
//!   `colour_bounds` its colour constraints. `None` only for a function
//!   whose unit failed (never in an `Ok` program).
//! - [`def_types`](TypedProgram::def_types), by [`DefId`]: closed.
//!
//! **Uses**: [`instantiations`](TypedProgram::instantiations) gives, for
//! every expression that names a global with a scheme (a function,
//! method, builtin, constructor), the types and colours its quantified
//! variables were instantiated at, in the caller's `Gen` numbering.
//! [`resolutions`](TypedProgram::resolutions) gives, for every method
//! name, how its dispatch constraint was discharged: an instance with
//! its arguments, a bound of the enclosing scheme (to be resolved per
//! specialisation, §4.3), or `dyn`.
//!
//! **Colours**: [`fn_colours`](TypedProgram::fn_colours) gives the
//! solved colour of every `fn` literal: `Send`, `Local`, or `Gen(i)`
//! when it depends on the enclosing scheme's colour variables.
//!
//! **Escape kinds**: [`builtins`](TypedProgram::builtins) is the table
//! of builtin signatures with the escape kind of every parameter
//! ([`Escape`](super::builtins::Escape)); a builtin use is a
//! `GlobalRef::Builtin(BuiltinId(i))` indexing it.
//!
//! **Order**: [`units`](TypedProgram::units) lists the units in the
//! order they were checked (SCCs dependencies first, then `impl`
//! bodies, then macros), prelude first: the order §3.5 step 5f runs
//! the ownership pass in.
//!
//! **Kinds for the ownership pass**: a protocol method's declared
//! `:borrow`/`:owned` per parameter is in
//! [`MethodDef::params`](super::decls::MethodDef); a `defun`
//! parameter's written `:borrow` in
//! [`ParamDecl::borrow`](super::decls::ParamDecl); whether a call
//! argument is `&x` in [`Arg`](super::ast::Arg). Not checked here, and
//! left to that pass: the syntactic `&` rules of types §6.5 and §6.9
//! (distinct variables, escaping captures of `&` parameters, `&`
//! parameters of async functions), escape summaries and count kinds.

use std::collections::HashMap;

use super::ast::{BindingId, DefId, ExprId, FunId, GlobalRef};
use super::builtins::{BuiltinSig, BUILTINS};
use super::decls::Globals;
use super::display::Printer;
use super::infer::{Instantiation, Resolution, UnitRef};
use super::scheme::Scheme;
use super::ty::{Colour, Ty};

/// A checked program. See the module documentation.
#[derive(Clone, Debug)]
pub struct TypedProgram {
    /// Every definition and binding site, lowered.
    pub globals: Globals,
    /// The scheme of every `defun` and `defmacro`, by `FunId`.
    pub fun_schemes: Vec<Option<Scheme>>,
    /// The closed type of every `def`, by `DefId`.
    pub def_types: Vec<Option<Ty>>,
    /// The type of every expression.
    pub expr_types: HashMap<ExprId, Ty>,
    /// The type of every binding site.
    pub binding_types: HashMap<BindingId, Ty>,
    /// The instantiation of every use of a global with a scheme.
    pub instantiations: HashMap<ExprId, Instantiation>,
    /// How every method use was dispatched.
    pub resolutions: HashMap<ExprId, Resolution>,
    /// The colour of every `fn` literal.
    pub fn_colours: HashMap<ExprId, Colour>,
    /// The builtin table with escape kinds.
    pub builtins: &'static [BuiltinSig],
    /// The units in checking order.
    pub units: Vec<UnitRef>,
}

impl TypedProgram {
    pub(crate) fn new(
        globals: Globals,
        env: super::infer::Env,
        t: super::infer::Tables,
        units: Vec<UnitRef>,
    ) -> Self {
        TypedProgram {
            globals,
            fun_schemes: env.funs,
            def_types: env.defs,
            expr_types: t.expr_types,
            binding_types: t.binding_types,
            instantiations: t.instantiations,
            resolutions: t.resolutions,
            fn_colours: t.fn_colours,
            builtins: BUILTINS,
            units,
        }
    }

    /// The `defun` named `name` in the user module, else the prelude.
    pub fn fun(&self, name: &str) -> Option<FunId> {
        match self.globals.value(self.globals.main, name)? {
            GlobalRef::Fun(f) => Some(f),
            _ => None,
        }
    }

    /// The scheme of the `defun` `name`.
    pub fn scheme(&self, name: &str) -> Option<&Scheme> {
        self.fun_schemes.get(self.fun(name)?.0 as usize)?.as_ref()
    }

    /// A scheme as text: `∀a b. (P a) ⇒ (fn (a) b)`, or the bare type
    /// when nothing is quantified and there is no context. Colour
    /// constraints print after the predicates as `κ ⊑ κ'` and `κ ⊒
    /// Caps{T}`.
    pub fn show_scheme(&self, s: &Scheme) -> String {
        show_scheme_in(&self.globals, s)
    }

    /// The scheme of the `defun` `name` as text.
    pub fn show_fun(&self, name: &str) -> Option<String> {
        self.scheme(name).map(|s| self.show_scheme(s))
    }

    /// A type inside the `defun` `f`'s body as text.
    pub fn show_in(&self, f: FunId, t: &Ty) -> String {
        let names = self
            .fun_schemes
            .get(f.0 as usize)
            .cloned()
            .flatten()
            .map(|s| s.var_names)
            .unwrap_or_default();
        Printer::with_names(&self.globals, &names, &[]).ty(t)
    }

    /// The type of the `def` `name`.
    pub fn def_type(&self, name: &str) -> Option<&Ty> {
        match self.globals.value(self.globals.main, name)? {
            GlobalRef::Def(DefId(d)) => self.def_types.get(d as usize)?.as_ref(),
            _ => None,
        }
    }
}

/// [`TypedProgram::show_scheme`] over the global tables alone, so that a
/// scheme can be printed before inference (the lowered protocol methods).
pub fn show_scheme_in(g: &Globals, s: &Scheme) -> String {
    let p = Printer::with_names(g, &s.var_names, &[]);
    let ty = match &s.ty {
        Ty::Fn(_, ps, r) if s.has_amp() => {
            let params: Vec<String> = ps
                .iter()
                .zip(&s.amps)
                .map(|(t, amp)| {
                    if *amp {
                        format!("(& {})", p.ty(t))
                    } else {
                        p.ty(t)
                    }
                })
                .collect();
            format!("(fn ({}) {})", params.join(" "), p.ty(r))
        }
        t => p.ty(t),
    };
    let mut ctx: Vec<String> = s.preds.iter().map(|q| p.pred(q)).collect();
    for b in &s.colour_bounds {
        ctx.push(match b {
            super::scheme::ColourBound::Flow(a, c) => {
                format!("{} ⊑ {}", colour(*a), colour(*c))
            }
            super::scheme::ColourBound::Caps(k, t) => {
                format!("{} ⊒ Caps{{{}}}", colour(*k), p.ty(t))
            }
        });
    }
    let mut vars = s.var_names.clone();
    vars.extend((0..s.n_colours).map(|i| format!("ς{i}")));
    let head = if vars.is_empty() {
        String::new()
    } else {
        format!("∀{}. ", vars.join(" "))
    };
    let ctx = if ctx.is_empty() {
        String::new()
    } else {
        format!("{} ⇒ ", ctx.join(" "))
    };
    format!("{head}{ctx}{ty}")
}

fn colour(k: Colour) -> String {
    match k {
        Colour::Send => "send".into(),
        Colour::Local => "local".into(),
        Colour::Gen(i) => format!("ς{i}"),
        Colour::Var(v) => format!("?ς{}", v.0),
        Colour::Rigid(i) => format!("κ{i}"),
    }
}
