//! The front end of the dump: the steps of [`crate::front::check_in`], so
//! that the program is the one `fibc emit` lowers (the macros run through
//! the same runner), but each way they stop is the record the matching
//! `fibref` dump prints (spec/bootstrap.md §5.1, §6.4, §7.4) and not a
//! message.

use fibref::expand::{expand_program, ExpandCtx};
use fibref::expand_dump::{expand_record, load_record, record};
use fibref::modules::{begin_spec, try_load_in, ModuleSpec};
use fibref::own::program::OwnedProgram;
use fibref::own::{check_forms, check_modules, CheckError, Checked};
use fibref::roots::Roots;
use fibref::syntax::Form;
use fibref::types::{infer_lowered, lower_modules, prelude_forms, TypeError};

use crate::compile::{compile_macro, Unsupported};
use crate::front::macro_runner;
use crate::macros::macro_forms;

/// Why a file has no sections.
pub(crate) enum Stop {
    /// The record or records of the step that failed: an expander's,
    /// a type error's or an ownership error's.
    Record(String),
    /// The tool itself failed (a runner that cannot start, a checker
    /// that panicked): not a verdict on the program.
    Internal(String),
    /// The compiler cannot lower the program yet; the message.
    Unsupported(String),
}

impl From<Unsupported> for Stop {
    fn from(u: Unsupported) -> Stop {
        Stop::Unsupported(u.0)
    }
}

/// A program read, loaded and expanded: its modules in dependency order
/// with the forms each expanded to, the prelude's forms, and the context
/// that holds the macros the modules defined.
pub(crate) struct Expanded {
    pub modules: Vec<(ModuleSpec, Vec<Form>)>,
    pub prelude: Vec<Form>,
    pub ctx: ExpandCtx,
}

/// The program whose main module is `source` (the file `file`), expanded
/// as `fibc emit` expands it, or the record that stops it. A module that
/// does not expand prints its `-- module NS FILE` line before the record.
pub(crate) fn expand(source: &str, file: &str, roots: &Roots) -> Result<Expanded, Stop> {
    let mut ctx = ExpandCtx::new();
    let prelude =
        prelude_forms(&mut ctx).map_err(|m| Stop::Record(record("Prelude", None, file, &m)))?;
    let loaded =
        try_load_in(source, file, roots).map_err(|e| Stop::Record(load_record(&e, file)))?;
    let all: Vec<Form> = loaded.iter().flat_map(|l| l.forms.clone()).collect();
    let mut runner = macro_runner(&all, prelude.clone()).map_err(Stop::Internal)?;
    let mut modules = Vec::with_capacity(loaded.len());
    for l in loaded {
        begin_spec(&mut ctx, &l.spec);
        match expand_program(l.forms, &mut ctx, runner.as_mut()) {
            Ok(forms) => {
                ctx.end_module();
                modules.push((l.spec, forms));
            }
            Err(e) => {
                let head = format!("-- module {} {}\n", l.spec.ns, l.file);
                return Err(Stop::Record(head + &expand_record(&e, &l.file)));
            }
        }
    }
    Ok(Expanded {
        modules,
        prelude,
        ctx,
    })
}

/// The program through the whole front end, as `fibc emit` has it.
pub(crate) fn checked(x: &Expanded, file: &str) -> Result<Checked, Stop> {
    check_modules(&x.modules, &x.prelude).map_err(|e| check_stop(e, file))
}

/// The program as far as the type checker takes it: the ownership pass is
/// not run, so the result's plan is empty, and `--layout` needs none.
pub(crate) fn typed(x: &Expanded, file: &str) -> Result<Checked, Stop> {
    let lowered = lower_modules(&x.modules, &x.prelude).map_err(|e| type_stop(&e, file))?;
    let typed = infer_lowered(lowered, true).map_err(|e| type_stop(&e, file))?;
    Ok(Checked {
        typed,
        owned: OwnedProgram::default(),
    })
}

fn type_stop(errs: &[TypeError], file: &str) -> Stop {
    Stop::Record(fibref::types_dump::error_records(errs, file))
}

fn check_stop(e: CheckError, file: &str) -> Stop {
    match e {
        CheckError::Type(errs) => type_stop(&errs, file),
        CheckError::Own(errs) => Stop::Record(fibref::own_dump::own_error_records(&errs, file)),
        CheckError::Prelude(m) => Stop::Record(record("Prelude", None, file, &m)),
        CheckError::Read(m) | CheckError::Expand(m) | CheckError::Internal(m) => Stop::Internal(m),
    }
}

/// The macro-time module of the macro `name` (its `ns/name` key or its bare
/// name, the first in key order), under the line `;; == macro KEY`, as
/// `fibc emit --macros` compiles it: the macro and a `main` as a program of
/// their own, as the first macro module of a run (`k` is 0). No record
/// when the file has no such macro: `error NoMacro`.
pub(crate) fn macro_text(x: &Expanded, name: &str, file: &str) -> Result<String, Stop> {
    let defs = x.ctx.macros_by_key();
    let Some(m) = defs.iter().find(|m| m.key == name || m.name == name) else {
        let message = format!("no macro {name}");
        return Err(Stop::Record(record("NoMacro", None, file, &message)));
    };
    let forms = macro_forms(m);
    let checked = check_forms(&forms, &x.prelude).map_err(|e| check_stop(e, file))?;
    let n = m.params.len() + usize::from(m.rest.is_some());
    let module = compile_macro(&checked, &m.name, n, 0)?;
    Ok(format!(";; == macro {}\n{}", m.key, module.text))
}
