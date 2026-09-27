//! User macros at expansion time (syntax §3.16): each `defmacro` body is
//! type-checked as a function over `Form` in a macro-time module, and
//! run by this evaluator. Phase separation: the macro-time module is
//! the prelude (every module this one requires) and the macro, nothing
//! of the module being expanded, so a macro that calls a function of
//! its own module is the expansion error `macro m calls f, which is not
//! available at expansion time; move f to a required module`.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::expand::{ExpandCtx, ExpandError, ExpandErrorKind, MacroDef, MacroRunner};
use crate::own::program::BodyKey;
use crate::own::{check_forms, CheckError, Checked};
use crate::syntax::{Form, FormKind, Pos};

use super::alloc::Placement;
use super::error::RunError;
use super::interp::Interp;
use super::pipeline::summary;
use super::value::Val;

/// Runs user macros through the evaluator, on the expander's thread,
/// which must have more than [`MACRO_STACK_BUDGET`] of stack left (the
/// evaluator's own thread, [`run_source`], does).
///
/// [`MACRO_STACK_BUDGET`]: super::MACRO_STACK_BUDGET
/// [`run_source`]: super::run_source
pub struct MacroEvaluator {
    prelude: Vec<Form>,
    /// The names the module being expanded defines (`defun`, `def`).
    module_names: HashSet<String>,
    /// Each macro's checked macro-time module, or why it did not check.
    checked: HashMap<String, Result<Arc<Checked>, ExpandErrorKind>>,
}

impl MacroEvaluator {
    /// A runner for the module whose top-level forms are `forms`, with
    /// the prelude's expanded forms.
    pub fn new(forms: &[Form], prelude: Vec<Form>) -> Self {
        let mut module_names = HashSet::new();
        for f in forms {
            collect_names(f, &mut module_names);
        }
        MacroEvaluator {
            prelude,
            module_names,
            checked: HashMap::new(),
        }
    }

    fn macro_module(&mut self, m: &MacroDef) -> Result<Arc<Checked>, ExpandErrorKind> {
        if let Some(c) = self.checked.get(&m.name) {
            return c.clone();
        }
        let forms = macro_forms(m);
        let c = check_forms(&forms, &self.prelude)
            .map(Arc::new)
            .map_err(|e| self.check_error(m, e));
        self.checked.insert(m.name.clone(), c.clone());
        c
    }

    fn check_error(&self, m: &MacroDef, e: CheckError) -> ExpandErrorKind {
        let name = m.name.clone();
        for msg in e.messages() {
            if let Some(f) = msg.strip_prefix("unbound name ") {
                if self.module_names.contains(f) {
                    return ExpandErrorKind::MacroPhase {
                        name,
                        fun: f.to_string(),
                    };
                }
            }
        }
        ExpandErrorKind::MacroFailed {
            name,
            message: e.to_string(),
        }
    }
}

impl MacroRunner for MacroEvaluator {
    fn run(&mut self, m: &MacroDef, args: Vec<Form>, ctx: &ExpandCtx) -> Result<Form, ExpandError> {
        let pos = ctx.call_pos().clone();
        let checked = self
            .macro_module(m)
            .map_err(|k| ExpandError::new(k, &pos))?;
        run_macro(&checked, m, args, ctx).map_err(|e| {
            let kind = match e {
                MacroRunError::Expand(e) => return e,
                MacroRunError::Run(r) => ExpandErrorKind::MacroFailed {
                    name: m.name.clone(),
                    message: r,
                },
            };
            ExpandError::new(kind, &pos)
        })
    }
}

enum MacroRunError {
    Expand(ExpandError),
    Run(String),
}

impl From<RunError> for MacroRunError {
    fn from(e: RunError) -> Self {
        MacroRunError::Run(e.to_string())
    }
}

/// Runs the macro's function over `args` in a fresh interpreter, and
/// audits its heap.
fn run_macro(
    c: &Checked,
    m: &MacroDef,
    args: Vec<Form>,
    ctx: &ExpandCtx,
) -> Result<Form, MacroRunError> {
    let mut it = Interp::new(&c.typed, &c.owned);
    it.ctx = Some(ctx);
    it.stack_budget = super::interp::MACRO_STACK_BUDGET;
    let form = call_macro(&mut it, m, args, ctx.call_pos());
    if let Some(e) = it.expand_error.take() {
        return Err(MacroRunError::Expand(e));
    }
    let form = form?;
    let report = it.finish();
    if !report.is_clean() {
        let s = summary(&report);
        return Err(MacroRunError::Run(format!(
            "the audit of its run is not clean: {s}"
        )));
    }
    Ok(form)
}

fn call_macro(
    it: &mut Interp<'_>,
    m: &MacroDef,
    args: Vec<Form>,
    pos: &Pos,
) -> Result<Form, RunError> {
    let f =
        it.p.globals
            .funs
            .iter()
            .position(|d| d.is_macro && d.name == m.name)
            .ok_or_else(|| RunError::internal(format!("no macro {}", m.name)))?;
    it.eval_defs()?;
    it.recording_inputs = true;
    let mut vals = Vec::new();
    let fixed = m.params.len().min(args.len());
    for a in &args[..fixed] {
        vals.push(it.form_value(a, Placement::Immortal)?);
    }
    if m.rest.is_some() {
        let mut rest = Vec::new();
        for a in &args[fixed..] {
            rest.push(it.form_value(a, Placement::Immortal)?);
        }
        vals.push(it.build_vec(&rest, Placement::Immortal)?);
    }
    it.recording_inputs = false;
    let key = BodyKey::Fun(crate::types::ast::FunId(f as u32));
    let v: Val = it.call_body(key, vals)?;
    let form = it.value_form(&v, pos)?;
    it.release(&v)?;
    Ok(form)
}

/// The macro-time module of `m`: the macro, and a `main` for the
/// checker, which requires one.
fn macro_forms(m: &MacroDef) -> Vec<Form> {
    let p = &m.pos;
    let sym = |s: &str| Form::new(FormKind::Sym(s.to_string()), p.clone());
    let list = |items: Vec<Form>| Form::new(FormKind::List(items), p.clone());
    let mut params: Vec<Form> = m.params.iter().map(|s| sym(s)).collect();
    if let Some(r) = &m.rest {
        params.push(sym("..."));
        params.push(sym(r));
    }
    let mut def = vec![sym("defmacro"), sym(&m.name), list(params)];
    def.extend(m.body.iter().cloned());
    let zero = Form::new(
        FormKind::Int {
            v: 0,
            width: crate::syntax::IntWidth::I64,
        },
        p.clone(),
    );
    let main = list(vec![
        sym("defun"),
        sym("main"),
        list(Vec::new()),
        sym("->"),
        sym("i64"),
        zero,
    ]);
    vec![list(def), main]
}

/// The names a top-level form defines (through top-level `do`s).
fn collect_names(f: &Form, out: &mut HashSet<String>) {
    let Some(items) = f.as_list() else { return };
    match items.first().and_then(Form::as_sym) {
        Some("defun") | Some("def") => {
            if let Some(n) = items.get(1).and_then(Form::as_sym) {
                out.insert(n.trim_end_matches(':').to_string());
            }
        }
        Some("do") => items[1..].iter().for_each(|x| collect_names(x, out)),
        _ => {}
    }
}
