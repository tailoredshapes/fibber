//! The expansion dump (spec/bootstrap.md §5): what `fibref expand` prints
//! and the self-hosted expander (`compiler/expand.fib`, M6 step 2) must
//! print byte for byte. For each file named, the forms of its program
//! after expansion, module by module, in the line format of the reader
//! dump (`crate::dump`), or the first error, in the same record.
//!
//! ```text
//! == FILE
//! -- module NS FILE-OF-NS
//! list 3 1:1 0..21        (the expanded forms of the module, as the
//!   sym "def" 1:2 1..4     reader dump prints forms)
//! -- module NS2 FILE-OF-NS2
//! error Kind L:C S..E: message
//! ```
//!
//! The expansion is what the pipeline does (`eval::pipeline`): the
//! prelude is expanded first and is the context every module of the
//! program expands in, the modules follow in dependency order, the main
//! module last, and a user macro is run by the interpreter's evaluator
//! ([`RunnerKind::Evaluator`]) or, to judge an expander that has no
//! evaluator yet, fails as `NoRunner` makes it ([`RunnerKind::None`]).
//! The context a file starts from is fresh each time.

mod args;
mod context;

use std::fmt::Write;
use std::thread;

pub use args::{flags, parse_args};

use crate::dump::{dump_forms_in, kind_name, span_in};
use crate::eval::{MacroEvaluator, STACK_BYTES};
use crate::expand::{
    expand_prelude, expand_program, ExpandCtx, ExpandError, ExpandErrorKind, MacroRunner, NoRunner,
    PRELUDE_NS,
};
use crate::modules::{begin_spec, try_load, LoadError};
use crate::syntax::{read_all, Form, Pos, ReadError};
use crate::types::prelude_forms;

/// What runs a user macro.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RunnerKind {
    /// The interpreter's evaluator (`eval::MacroEvaluator`), as
    /// `fibref run` does.
    #[default]
    Evaluator,
    /// Nothing: a call of a user macro is `MacroNeedsEvaluator`.
    None,
}

/// Limits to expand under instead of the defaults (`expand::Limits`), set
/// once the prelude is expanded, so that a small one can reach an error
/// on a small input. `None` keeps the default.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LimitOverrides {
    pub steps: Option<usize>,
    pub depth: Option<usize>,
    pub forms: Option<usize>,
}

impl LimitOverrides {
    fn apply(&self, ctx: &mut ExpandCtx) {
        let l = &mut ctx.limits;
        l.max_steps = self.steps.unwrap_or(l.max_steps);
        l.max_depth = self.depth.unwrap_or(l.max_depth);
        l.max_forms = self.forms.unwrap_or(l.max_forms);
    }
}

/// How `expand_files` expands and what it prints.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Options {
    /// Each file is a library prelude (`--prelude`): see [`dump_prelude`].
    pub prelude: bool,
    /// After each module, the context it left (`--context`, [`context`]).
    pub context: bool,
    pub runner: RunnerKind,
    pub limits: LimitOverrides,
}

/// One file's output, and whether it ends in an error record.
struct Dump {
    text: String,
    failed: bool,
}

impl Dump {
    fn ok(text: String) -> Dump {
        Dump {
            text,
            failed: false,
        }
    }

    fn failure(text: String) -> Dump {
        Dump { text, failed: true }
    }
}

/// What `fibref expand` prints for `files` and the status it exits with:
/// each file under a `== FILE` header, its dump or the line `unreadable`
/// (it cannot be read as UTF-8 text); status 0 if every file expanded, 1
/// if one ended in an error record, 2 if one was unreadable (the larger
/// wins). Runs on a thread of [`STACK_BYTES`], which the evaluator needs.
pub fn expand_files(files: &[String], opts: &Options) -> (String, u8) {
    thread::scope(|scope| {
        let worker = thread::Builder::new()
            .name("fibref-expand".into())
            .stack_size(STACK_BYTES)
            .spawn_scoped(scope, || expand_here(files, opts));
        match worker.map(|h| h.join()) {
            Ok(Ok(done)) => done,
            Ok(Err(_)) => ("internal error: the expander panicked\n".to_string(), 2),
            Err(e) => (format!("cannot start the expander: {e}\n"), 2),
        }
    })
}

fn expand_here(files: &[String], opts: &Options) -> (String, u8) {
    let mut text = String::new();
    let mut status = 0u8;
    for file in files {
        let _ = writeln!(text, "== {file}");
        match std::fs::read_to_string(file) {
            Ok(source) => {
                let dump = if opts.prelude {
                    dump_prelude(&source, file, opts)
                } else {
                    dump_program(&source, file, opts)
                };
                status = status.max(u8::from(dump.failed));
                text.push_str(&dump.text);
            }
            Err(_) => {
                text.push_str("unreadable\n");
                status = 2;
            }
        }
    }
    (text, status)
}

/// The dump of the program whose main module is `source` (the file
/// `file`).
fn dump_program(source: &str, file: &str, opts: &Options) -> Dump {
    let mut ctx = ExpandCtx::new();
    let prelude = match prelude_forms(&mut ctx) {
        Ok(p) => p,
        Err(m) => return Dump::failure(record("Prelude", None, file, &m)),
    };
    opts.limits.apply(&mut ctx);
    let baseline = context::Baseline::of(&ctx);
    let loaded = match try_load(source, file) {
        Ok(l) => l,
        Err(e) => return Dump::failure(load_record(&e, file)),
    };
    let all: Vec<Form> = loaded.iter().flat_map(|l| l.forms.clone()).collect();
    let mut runner: Box<dyn MacroRunner> = match opts.runner {
        RunnerKind::Evaluator => Box::new(MacroEvaluator::new(&all, prelude)),
        RunnerKind::None => Box::new(NoRunner),
    };
    let mut out = String::new();
    for l in loaded {
        begin_spec(&mut ctx, &l.spec);
        let _ = writeln!(out, "-- module {} {}", l.spec.ns, l.file);
        match expand_program(l.forms, &mut ctx, runner.as_mut()) {
            Ok(forms) => out.push_str(&dump_forms_in(&forms, &l.file)),
            Err(e) => {
                out.push_str(&expand_record(&e, &l.file));
                return Dump::failure(out);
            }
        }
        if opts.context {
            context::dump_context(&ctx, &l.spec.ns, &l.file, &baseline, &mut out);
        }
        ctx.end_module();
    }
    Dump::ok(out)
}

/// The dump of `source` (the file `file`) as a library prelude, which is
/// what `types::prelude_forms` makes of `lib/prelude.fib`: expanded with
/// no runner in the module `fib.prelude`, after the expander's own
/// prelude (`expand::PRELUDE_SOURCE`), whose forms are in the same
/// section and end with `@<prelude>`. With `lib/prelude.fib` as the file
/// it is the expanded prelude every program expands in.
fn dump_prelude(source: &str, file: &str, opts: &Options) -> Dump {
    let lib = match read_all(source, file) {
        Ok(forms) => forms,
        Err(e) => return Dump::failure(read_record(&e, file)),
    };
    let mut ctx = ExpandCtx::new();
    let mut out = format!("-- module {PRELUDE_NS} {file}\n");
    let own = match expand_prelude(&mut ctx) {
        Ok(forms) => forms,
        Err(e) => return Dump::failure(out + &expand_record(&e, file)),
    };
    opts.limits.apply(&mut ctx);
    match expand_program(lib, &mut ctx, &mut NoRunner) {
        Ok(forms) => {
            out.push_str(&dump_forms_in(&own, file));
            out.push_str(&dump_forms_in(&forms, file));
        }
        Err(e) => return Dump::failure(out + &expand_record(&e, file)),
    }
    if opts.context {
        context::dump_context(
            &ctx,
            PRELUDE_NS,
            file,
            &context::Baseline::empty(),
            &mut out,
        );
    }
    Dump::ok(out)
}

/// `error KIND L:C S..E: MESSAGE`: the record of the reader dump. A
/// position in another file than `home` ends `@FILE`; an error with no
/// position (a module that cannot be found) is at `0:0 0..0`.
fn record(kind: &str, pos: Option<&Pos>, home: &str, message: &str) -> String {
    let at = pos.map_or_else(|| "0:0 0..0".to_string(), |p| span_in(p, home));
    format!("error {kind} {at}: {message}\n")
}

fn read_record(e: &ReadError, home: &str) -> String {
    record(kind_name(&e.kind), Some(&e.pos), home, &e.kind.to_string())
}

fn expand_record(e: &ExpandError, home: &str) -> String {
    let kind = expand_kind_name(&e.kind);
    record(kind, Some(&e.pos), home, &e.kind.to_string())
}

/// The record of a program that could not be loaded. A module that is not
/// at its file prints no operating system text: it is not the same
/// words in every language a tool is written in.
fn load_record(e: &LoadError, home: &str) -> String {
    match e {
        LoadError::Read(r) => read_record(r, home),
        LoadError::Spec { pos, what } => record("BadNs", Some(pos), home, what),
        LoadError::Missing { ns, file, .. } => {
            let message = format!("module {ns} is not at {file}");
            record("ModuleMissing", None, home, &message)
        }
        LoadError::Cycle { .. } => record("ModuleCycle", None, home, &e.to_string()),
        LoadError::Mismatch { .. } => record("ModuleMismatch", None, home, &e.to_string()),
    }
}

/// The name of an expansion error's variant.
pub fn expand_kind_name(k: &ExpandErrorKind) -> &'static str {
    use ExpandErrorKind as K;
    match k {
        K::MacroNeedsEvaluator { .. } => "MacroNeedsEvaluator",
        K::MacroPhase { .. } => "MacroPhase",
        K::MacroFailed { .. } => "MacroFailed",
        K::MacroArity { .. } => "MacroArity",
        K::AmbiguousMacro { .. } => "AmbiguousMacro",
        K::UnquoteOutsideQuasiquote { .. } => "UnquoteOutsideQuasiquote",
        K::SpliceOutsideList => "SpliceOutsideList",
        K::Malformed { .. } => "Malformed",
        K::MacroNamesCoreForm { .. } => "MacroNamesCoreForm",
        K::DeriveProtocol { .. } => "DeriveProtocol",
        K::DeriveTarget { .. } => "DeriveTarget",
        K::NotAStruct { .. } => "NotAStruct",
        K::NotAnEnum { .. } => "NotAnEnum",
        K::BadReflection { .. } => "BadReflection",
        K::ThreadStep => "ThreadStep",
        K::InOutOutsideArgument => "InOutOutsideArgument",
        K::NilCalled => "NilCalled",
        K::BraceInPattern => "BraceInPattern",
        K::ExpressionAtTopLevel => "ExpressionAtTopLevel",
        K::DefinitionInExpression { .. } => "DefinitionInExpression",
        K::NsNotFirst => "NsNotFirst",
        K::TooManySteps { .. } => "TooManySteps",
        K::TooDeep { .. } => "TooDeep",
        K::TooLarge { .. } => "TooLarge",
        K::BadLiteral { .. } => "BadLiteral",
    }
}

#[cfg(test)]
mod tests;
