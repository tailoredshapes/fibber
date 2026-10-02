//! The ownership dump (spec/bootstrap.md §7): what `fibref own` prints and
//! the self-hosted ownership checker (`compiler/own.fib`, M6 step 4) must
//! print byte for byte. For each file named, the program is read, loaded,
//! expanded and typed as `fibref types` does it, then run through the
//! ownership pass as [`crate::own`] does ([`syntactic::check`] before
//! inference, [`analyse_observed`] after), and the decisions are printed
//! module by module: every body with its modes, bindings, calls and
//! operations, the facts each unit settled on, the summaries, the
//! functions whose value is taken, and, on request, the text of `fibref
//! explain`. A program that is rejected prints the errors of its first
//! failing step.
//!
//! ```text
//! == FILE
//! -- module NS FILE-OF-NS
//! body fun f
//!   param B3 x borrowed escapes=0 declared-borrow=0
//!   expr E9 borrowed b:3
//! summary f (0,0)
//! ```
//!
//! Nothing here iterates a `HashMap` unsorted: every table the pass keeps
//! by id is printed in order of its id, so it is the same text on every
//! run. Ownership prints no types, so no variable is renumbered.

mod args;
mod sections;
mod texts;

#[cfg(test)]
mod tests;

use std::collections::BTreeSet;

pub use args::{flags, parse_args};
pub use texts::kind_name;

use crate::dump::span_in;
use crate::own::syntactic;
use crate::own::{analyse_observed, OwnError};
use crate::syntax::Form;
use crate::types::{infer_lowered, lower_modules, TypeError};
use crate::types_dump::{
    error_records, expand_library, expand_modules, files_dump, layout_of, prelude_shown, push,
    Dump, Shown,
};

use crate::modules::ModuleSpec;
use sections::Seen;

/// What a module's section is made of, and the order it is printed in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Section {
    Body,
    Facts,
    Summary,
    Taken,
    Error,
    Explain,
}

impl Section {
    /// Every section, in the order of the dump.
    pub const ALL: [Section; 6] = [
        Section::Body,
        Section::Facts,
        Section::Summary,
        Section::Taken,
        Section::Error,
        Section::Explain,
    ];

    /// The word `--sections` knows it by.
    pub fn name(self) -> &'static str {
        match self {
            Section::Body => "body",
            Section::Facts => "facts",
            Section::Summary => "summary",
            Section::Taken => "taken",
            Section::Error => "error",
            Section::Explain => "explain",
        }
    }

    /// The section a `--sections` word names.
    pub fn from_name(word: &str) -> Option<Section> {
        Section::ALL.into_iter().find(|s| s.name() == word)
    }
}

/// How `own_files` checks and what it prints.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Options {
    /// `--sections LIST`: print only these; `None` prints every section
    /// but `explain`.
    pub sections: Option<BTreeSet<Section>>,
    /// `--library`: `main` is not required (`check_library`).
    pub library: bool,
    /// `--implicit`: print the sections of the implicit modules, of the
    /// modules read for them, and of `fib.prelude`.
    pub implicit: bool,
    /// `--implicit-lib LIST`: the implicit modules instead of
    /// `IMPLICIT_LIB`.
    pub implicit_lib: Option<Vec<String>>,
    /// `--prelude`: each file is a library prelude, checked alone.
    pub prelude: bool,
}

impl Options {
    /// Whether the dump prints section `s`.
    pub fn wants(&self, s: Section) -> bool {
        match &self.sections {
            Some(set) => set.contains(&s),
            None => s != Section::Explain,
        }
    }
}

/// What `fibref own` prints for `files` and the status it exits with: each
/// file under a `== FILE` header, its dump or the line `unreadable`;
/// status 0 if every file was accepted, 1 if one ended in an error
/// record, 2 if one was unreadable (the larger wins). Runs on a thread
/// with the stack the evaluator and the checker need.
pub fn own_files(files: &[String], opts: &Options) -> (String, u8) {
    files_dump("own", files, "", |file, source| {
        if opts.prelude {
            dump_prelude(source, file, opts)
        } else {
            dump_program(source, file, opts)
        }
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

/// Checks `modules` against `prelude` as `own::check_modules` does and
/// prints the sections of the modules in `layout`, or the errors of the
/// first failing step (their positions relative to `main_file`).
fn check_and_dump(
    modules: &[(ModuleSpec, Vec<Form>)],
    prelude: &[Form],
    (layout, main_file): (&[Shown], &str),
    opts: &Options,
) -> Dump {
    let lowered = match lower_modules(modules, prelude) {
        Ok(l) => l,
        Err(errs) => return type_errors(&errs, main_file, opts),
    };
    let amp = syntactic::check(&lowered.globals);
    if !amp.is_empty() {
        return own_errors(&amp, main_file, opts);
    }
    let typed = match infer_lowered(lowered, !opts.library) {
        Ok(t) => t,
        Err(errs) => return type_errors(&errs, main_file, opts),
    };
    let mut facts = Vec::new();
    let result = analyse_observed(&typed, &mut |keys, f| {
        facts.push((keys.to_vec(), f.clone()))
    });
    match result {
        Err(errs) => own_errors(&errs, main_file, opts),
        Ok(owned) => {
            let seen = Seen {
                typed: &typed,
                owned: &owned,
                facts: &facts,
            };
            Dump::ok(sections_text(&seen, layout, opts))
        }
    }
}

fn sections_text(seen: &Seen, layout: &[Shown], opts: &Options) -> String {
    let mut out = String::new();
    for shown in layout {
        out.push_str(&format!("-- module {} {}\n", shown.ns, shown.file));
        sections::module(seen, shown, opts, &mut out);
    }
    out
}

/// The `error` records of a type error of the failed step.
fn type_errors(errs: &[TypeError], home: &str, opts: &Options) -> Dump {
    let text = match opts.wants(Section::Error) {
        true => error_records(errs, home),
        false => String::new(),
    };
    Dump::failure(text)
}

/// The `error` records of the ownership errors of the failed step.
fn own_errors(errs: &[OwnError], home: &str, opts: &Options) -> Dump {
    let mut out = String::new();
    if opts.wants(Section::Error) {
        for e in errs {
            let at = span_in(&e.pos, home);
            push(
                &mut out,
                &format!("error {} {at}: {}", kind_name(e.kind), e.message),
            );
        }
    }
    Dump::failure(out)
}
