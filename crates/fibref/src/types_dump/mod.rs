//! The types dump (spec/bootstrap.md §6): what `fibref types` prints and
//! the self-hosted type checker (`compiler/types.fib`, M6 step 3) must
//! print byte for byte. For each file named, the program is read, loaded
//! and expanded as `fibref expand` does it, then checked as the pipeline
//! does ([`crate::types::lower_modules`], [`crate::types::infer_lowered`]),
//! and the result is printed module by module: declarations, schemes, the
//! order the units were checked, and, on request, the resolved AST and the
//! typed tables. A program the checker rejects prints the errors of its
//! first failing step.
//!
//! ```text
//! == FILE
//! -- module NS FILE-OF-NS
//! type Point () struct 1:1 0..27
//!   field x : i64
//! fun f : (fn (i64) i64) params x
//! unit scc f
//! ```
//!
//! Nothing here iterates a `HashMap`: the dump reads the `Vec` tables of
//! [`Globals`] and looks names up one at a time, so it is the same text
//! on every run. Variable ids that depend on allocation order (`?ς7`) are
//! renumbered by first occurrence in each record ([`normalise`]).

mod args;
mod normal;
mod sections;
mod tables;
mod texts;

use std::collections::BTreeSet;
use std::thread;

pub use args::{flags, parse_args};
pub use normal::normalise;
pub(crate) use sections::impl_method_name;

use crate::dump::span_in;
use crate::eval::{MacroEvaluator, STACK_BYTES};
use crate::expand::{expand_prelude, expand_program, ExpandCtx, NoRunner, PRELUDE_NS};
use crate::expand_dump::{expand_record, load_record, read_record, record};
use crate::modules::{begin_spec, try_load_with, ModuleSpec, IMPLICIT_LIB};
use crate::roots::Roots;
use crate::syntax::{read_all, Form};
use crate::types::decls::{Globals, ModuleId};
use crate::types::{
    infer_lowered, lower_modules, prelude_forms, ErrorKind, TypeError, TypedProgram, CHECK_STACK,
};

/// How far the checker runs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Stage {
    /// Steps 1-3 of types §3.5 (`lower_modules`): every name resolved and
    /// every body lowered, nothing inferred.
    Lower,
    /// The whole checker.
    #[default]
    Infer,
}

/// What a module's section is made of, and the order it is printed in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Section {
    Type,
    Protocol,
    Instance,
    Fun,
    Def,
    Extern,
    Unit,
    Error,
    Ast,
    Tables,
}

impl Section {
    /// Every section, in the order of the dump.
    pub const ALL: [Section; 10] = [
        Section::Type,
        Section::Protocol,
        Section::Instance,
        Section::Fun,
        Section::Def,
        Section::Extern,
        Section::Unit,
        Section::Error,
        Section::Ast,
        Section::Tables,
    ];

    /// The word `--sections` knows it by.
    pub fn name(self) -> &'static str {
        match self {
            Section::Type => "type",
            Section::Protocol => "protocol",
            Section::Instance => "instance",
            Section::Fun => "fun",
            Section::Def => "def",
            Section::Extern => "extern",
            Section::Unit => "unit",
            Section::Error => "error",
            Section::Ast => "ast",
            Section::Tables => "tables",
        }
    }

    /// The section a `--sections` word names.
    pub fn from_name(word: &str) -> Option<Section> {
        Section::ALL.into_iter().find(|s| s.name() == word)
    }
}

/// How `types_files` checks and what it prints.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Options {
    /// `--stage`.
    pub stage: Stage,
    /// `--sections LIST`: print only these; `None` prints every section
    /// but `ast` and `tables`.
    pub sections: Option<BTreeSet<Section>>,
    /// `--library`: `main` is not required (`check_library`).
    pub library: bool,
    /// `--implicit`: print the sections of the implicit modules, of the
    /// modules read for them, and of `fib.prelude`.
    pub implicit: bool,
    /// `--implicit-lib LIST`: the implicit modules instead of
    /// [`IMPLICIT_LIB`].
    pub implicit_lib: Option<Vec<String>>,
    /// `--prelude`: each file is a library prelude, checked alone.
    pub prelude: bool,
    /// `--ast`: also print the `ast` section.
    pub ast: bool,
    /// `--tables`: also print the `tables` section (and `builtins`).
    pub tables: bool,
}

impl Options {
    /// Whether the dump prints section `s`.
    pub fn wants(&self, s: Section) -> bool {
        let listed = match &self.sections {
            Some(set) => set.contains(&s),
            None => !matches!(s, Section::Ast | Section::Tables),
        };
        listed || (s == Section::Ast && self.ast) || (s == Section::Tables && self.tables)
    }
}

/// One file's output, and whether it ends in an error record.
pub(crate) struct Dump {
    pub(crate) text: String,
    pub(crate) failed: bool,
}

impl Dump {
    pub(crate) fn ok(text: String) -> Dump {
        Dump {
            text,
            failed: false,
        }
    }

    pub(crate) fn failure(text: String) -> Dump {
        Dump { text, failed: true }
    }
}

/// A module that has a section: its id in the tables, its `ns`, its file.
pub(crate) struct Shown {
    pub id: ModuleId,
    pub ns: String,
    pub file: String,
}

/// The tables a section reads: the global tables, and the typed program
/// when inference ran.
pub(crate) struct View<'a> {
    pub g: &'a Globals,
    pub typed: Option<&'a TypedProgram>,
}

/// Appends one record (a line, or an error with a message of several) to
/// `out`, its variable ids normalised.
pub(crate) fn push(out: &mut String, text: &str) {
    out.push_str(&normalise(text));
    out.push('\n');
}

/// Appends one line as it is: the `ast` section prints names the program
/// wrote, which are not the checker's variables.
pub(crate) fn push_raw(out: &mut String, text: &str) {
    out.push_str(text);
    out.push('\n');
}

/// What `fibref types` prints for `files` and the status it exits with:
/// each file under a `== FILE` header, its dump or the line `unreadable`;
/// status 0 if every file was typed, 1 if one ended in an error record,
/// 2 if one was unreadable (the larger wins). With `--tables`, the
/// `builtins` table comes once, before the first file. Runs on a thread
/// with the stack both the evaluator and the checker need.
pub fn types_files(files: &[String], opts: &Options) -> (String, u8) {
    let head = if opts.wants(Section::Tables) {
        tables::builtins_text()
    } else {
        String::new()
    };
    files_dump("types", files, &head, |file, source| {
        if opts.prelude {
            dump_prelude(source, file, opts)
        } else {
            dump_program(source, file, opts)
        }
    })
}

/// The loop of `fibref types` and `fibref own` (spec/bootstrap.md §6.1,
/// §7.1): `head`, then each file under its `== FILE` header with what
/// `one(file, source)` makes of it, on a thread of the stack the
/// evaluator and the checker need. `what` names the command in the
/// messages of a panic and of a thread that cannot start.
pub(crate) fn files_dump<F>(what: &str, files: &[String], head: &str, one: F) -> (String, u8)
where
    F: Fn(&str, &str) -> Dump + Sync,
{
    thread::scope(|scope| {
        let worker = thread::Builder::new()
            .name(format!("fibref-{what}"))
            .stack_size(STACK_BYTES.max(CHECK_STACK))
            .spawn_scoped(scope, || files_here(files, head, &one));
        match worker.map(|h| h.join()) {
            Ok(Ok(done)) => done,
            Ok(Err(_)) => (format!("internal error: the {what} dump panicked\n"), 2),
            Err(e) => (format!("cannot start the {what} dump: {e}\n"), 2),
        }
    })
}

fn files_here(files: &[String], head: &str, one: &dyn Fn(&str, &str) -> Dump) -> (String, u8) {
    let mut text = head.to_string();
    let mut status = 0u8;
    for file in files {
        text.push_str(&format!("== {file}\n"));
        match std::fs::read_to_string(file) {
            Ok(source) => {
                let dump = one(file, &source);
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

/// A program read, loaded and expanded: its modules in dependency order
/// with the forms each expanded to, the prelude's forms, and each
/// module's file and whether it is implicit.
pub(crate) struct Expanded {
    pub modules: Vec<(ModuleSpec, Vec<Form>)>,
    pub prelude: Vec<Form>,
    pub shown: Vec<(String, bool)>,
}

/// The program whose main module is `source` (the file `file`) as
/// `fibref expand` sees it, or the dump of the record that stops it.
pub(crate) fn expand_modules(
    source: &str,
    file: &str,
    implicit_lib: &Option<Vec<String>>,
) -> Result<Expanded, Dump> {
    let mut ctx = ExpandCtx::new();
    let prelude =
        prelude_forms(&mut ctx).map_err(|m| Dump::failure(record("Prelude", None, file, &m)))?;
    let implicit: Vec<&str> = match implicit_lib {
        Some(list) => list.iter().map(String::as_str).collect(),
        None => IMPLICIT_LIB.to_vec(),
    };
    let loaded = try_load_with(source, file, &Roots::default(), &implicit)
        .map_err(|e| Dump::failure(load_record(&e, file)))?;
    let all: Vec<Form> = loaded.iter().flat_map(|l| l.forms.clone()).collect();
    let mut runner = MacroEvaluator::new(&all, prelude.clone());
    let mut modules: Vec<(ModuleSpec, Vec<Form>)> = Vec::new();
    let mut shown: Vec<(String, bool)> = Vec::new();
    for l in loaded {
        begin_spec(&mut ctx, &l.spec);
        match expand_program(l.forms, &mut ctx, &mut runner) {
            Ok(forms) => {
                ctx.end_module();
                shown.push((l.file, l.implicit));
                modules.push((l.spec, forms));
            }
            Err(e) => {
                let head = format!(
                    "-- module {} {}
",
                    l.spec.ns, l.file
                );
                return Err(Dump::failure(head + &expand_record(&e, &l.file)));
            }
        }
    }
    Ok(Expanded {
        modules,
        prelude,
        shown,
    })
}

/// The dump of the program whose main module is `source` (the file
/// `file`).
fn dump_program(source: &str, file: &str, opts: &Options) -> Dump {
    match expand_modules(source, file, &opts.implicit_lib) {
        Err(dump) => dump,
        Ok(x) => {
            let layout = layout_of(&x.modules, x.shown, opts.implicit);
            check_and_dump(&x.modules, &x.prelude, (&layout, file), opts)
        }
    }
}

/// The modules that have a section, in order: the prelude first with
/// `implicit`, then each program module (module `i` is `ModuleId(i +
/// 2)`, after the builtins and the prelude), the implicit ones only with
/// `implicit`. `shown` is each module's file and whether it is implicit.
pub(crate) fn layout_of(
    modules: &[(ModuleSpec, Vec<Form>)],
    shown: Vec<(String, bool)>,
    implicit: bool,
) -> Vec<Shown> {
    let mut layout = Vec::new();
    if implicit {
        layout.push(prelude_shown("lib/prelude.fib"));
    }
    for (i, ((file, is_implicit), (spec, _))) in shown.into_iter().zip(modules).enumerate() {
        if implicit || !is_implicit {
            let (id, ns) = (ModuleId(i as u32 + 2), spec.ns.clone());
            layout.push(Shown { id, ns, file });
        }
    }
    layout
}

/// The section of the module `fib.prelude`, read from `file`.
pub(crate) fn prelude_shown(file: &str) -> Shown {
    Shown {
        id: ModuleId::PRELUDE,
        ns: PRELUDE_NS.to_string(),
        file: file.to_string(),
    }
}

/// The forms `source` (the file `file`) makes as a library prelude,
/// which is what `prelude_forms` makes of `lib/prelude.fib`: the
/// expander's own prelude, then the forms of the file, expanded with no
/// runner in the module `fib.prelude`; or the dump of the record that
/// stops it.
pub(crate) fn expand_library(source: &str, file: &str) -> Result<Vec<Form>, Dump> {
    let lib = read_all(source, file).map_err(|e| Dump::failure(read_record(&e, file)))?;
    let mut ctx = ExpandCtx::new();
    let expanded = expand_prelude(&mut ctx).and_then(|mut forms| {
        forms.extend(expand_program(lib, &mut ctx, &mut NoRunner)?);
        Ok(forms)
    });
    expanded.map_err(|e| {
        let head = format!(
            "-- module {PRELUDE_NS} {file}
"
        );
        Dump::failure(head + &expand_record(&e, file))
    })
}

/// The dump of `source` (the file `file`) as a library prelude, checked
/// alone (no `main`).
fn dump_prelude(source: &str, file: &str, opts: &Options) -> Dump {
    let forms = match expand_library(source, file) {
        Ok(forms) => forms,
        Err(dump) => return dump,
    };
    let layout = [prelude_shown(file)];
    let library = Options {
        library: true,
        ..opts.clone()
    };
    check_and_dump(&[], &forms, (&layout, file), &library)
}

/// Checks the expanded `modules` against `prelude` as far as `opts` says
/// and prints the sections of the modules in `layout`, or the errors of
/// the first failing step (their positions relative to `main_file`).
fn check_and_dump(
    modules: &[(ModuleSpec, Vec<Form>)],
    prelude: &[Form],
    (layout, main_file): (&[Shown], &str),
    opts: &Options,
) -> Dump {
    let errors = |errs: &[TypeError]| error_dump(errs, main_file, opts);
    match lower_modules(modules, prelude) {
        Err(errs) => errors(&errs),
        Ok(l) if opts.stage == Stage::Lower => {
            let view = View {
                g: &l.globals,
                typed: None,
            };
            Dump::ok(sections_text(&view, layout, opts))
        }
        Ok(l) => match infer_lowered(l, !opts.library) {
            Err(errs) => errors(&errs),
            Ok(t) => {
                let view = View {
                    g: &t.globals,
                    typed: Some(&t),
                };
                Dump::ok(sections_text(&view, layout, opts))
            }
        },
    }
}

fn sections_text(view: &View, layout: &[Shown], opts: &Options) -> String {
    let mut out = String::new();
    for shown in layout {
        out.push_str(&format!("-- module {} {}\n", shown.ns, shown.file));
        sections::module(view, shown, opts, &mut out);
    }
    out
}

/// The `error` records of a failed step, in order.
fn error_dump(errs: &[TypeError], home: &str, opts: &Options) -> Dump {
    let text = if opts.wants(Section::Error) {
        error_records(errs, home)
    } else {
        String::new()
    };
    Dump::failure(text)
}

/// The `error` records of the type errors `errs`, their positions
/// relative to `home`.
pub(crate) fn error_records(errs: &[TypeError], home: &str) -> String {
    let mut out = String::new();
    for e in errs {
        let at = span_in(&e.pos, home);
        let line = format!("error {} {at}: {}", kind_name(e.kind), e.message);
        push(&mut out, &line);
    }
    out
}

/// The name of an error's variant; a new variant is a compile error here
/// until it is named.
pub fn kind_name(k: ErrorKind) -> &'static str {
    use ErrorKind as K;
    match k {
        K::Resolve => "Resolve",
        K::Unify => "Unify",
        K::Infinite => "Infinite",
        K::NoField => "NoField",
        K::FieldUnresolved => "FieldUnresolved",
        K::DerefUnresolved => "DerefUnresolved",
        K::NoInstance => "NoInstance",
        K::ImplContext => "ImplContext",
        K::Ambiguous => "Ambiguous",
        K::CellNotSend => "CellNotSend",
        K::ValueNotSend => "ValueNotSend",
        K::RigidColour => "RigidColour",
        K::AmpArgument => "AmpArgument",
        K::AmpParamValue => "AmpParamValue",
        K::AmpPosition => "AmpPosition",
        K::AmpFunctionValue => "AmpFunctionValue",
        K::NonExhaustive => "NonExhaustive",
        K::Redundant => "Redundant",
        K::AwaitOutsideAsync => "AwaitOutsideAsync",
        K::WeakScalar => "WeakScalar",
        K::WeakOption => "WeakOption",
        K::NotObject => "NotObject",
        K::ConstantCalled => "ConstantCalled",
        K::RecurOutsideLoop => "RecurOutsideLoop",
        K::RecurNotTail => "RecurNotTail",
        K::DefUnresolved => "DefUnresolved",
        K::DefHoldsCell => "DefHoldsCell",
        K::DefCycle => "DefCycle",
        K::Other => "Other",
    }
}

#[cfg(test)]
mod tests;
