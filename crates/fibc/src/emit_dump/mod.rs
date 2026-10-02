//! The emitter dump (spec/bootstrap.md §8): what `fibc emit-dump` prints
//! and the self-hosted emitter (`compiler/emit.fib`, M6 step 5) must print
//! byte for byte. For each file named, the program is read, loaded,
//! expanded, typed and checked as `fibc emit` does it, and the lIR module
//! it would print is printed in its sections: the runtime, the `extern`
//! declarations, the object structs and the type table, the static data,
//! the keyword helpers, the values of the `def`s, the quoted constants,
//! the functions, and `main`. `--layout` prints the layout of every ground
//! type the checker knew, with no body emitted and no ownership plan, and
//! `--macro NAME` one macro-time module, so that names, layout,
//! monomorphisation and objects can each be judged alone.
//!
//! ```text
//! == FILE
//! ;; == section runtime
//! (defstruct fib.typerec (ptr ptr ptr i64 ptr))
//! ;; == section fns
//! (define internal tailcc (f.sq i64) ((i64 p0)) ..
//! ```
//!
//! `fibc emit` is unchanged: the sections are the pieces of its text
//! ([`crate::compile::Parts`]), and with every section and the header
//! lines left out they are that text.

mod args;
mod front;
mod layout;
mod sections;

#[cfg(test)]
mod tests;

use std::collections::BTreeSet;
use std::thread;

pub use args::{flags, parse_args};

use fibref::eval::STACK_BYTES;
use fibref::roots::Roots;
use fibref::types::CHECK_STACK;

use crate::compile::compile_module;
use front::Stop;

/// The parts of a module text, in the order of the dump, each under a line
/// `;; == section NAME`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Section {
    Runtime,
    Externs,
    Types,
    Statics,
    Keywords,
    Defs,
    Quotes,
    Fns,
    Main,
}

impl Section {
    /// Every section, in the order of the dump.
    pub const ALL: [Section; 9] = [
        Section::Runtime,
        Section::Externs,
        Section::Types,
        Section::Statics,
        Section::Keywords,
        Section::Defs,
        Section::Quotes,
        Section::Fns,
        Section::Main,
    ];

    /// The word `--sections` knows it by.
    pub fn name(self) -> &'static str {
        match self {
            Section::Runtime => "runtime",
            Section::Externs => "externs",
            Section::Types => "types",
            Section::Statics => "statics",
            Section::Keywords => "keywords",
            Section::Defs => "defs",
            Section::Quotes => "quotes",
            Section::Fns => "fns",
            Section::Main => "main",
        }
    }

    /// The section a `--sections` word names.
    pub fn from_name(word: &str) -> Option<Section> {
        Section::ALL.into_iter().find(|s| s.name() == word)
    }
}

/// What `emit_files` prints.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Options {
    /// `--sections LIST`: print only these; `None` prints every section
    /// (only `fns`, with a `--fn` prefix).
    pub sections: Option<BTreeSet<Section>>,
    /// `--fn PREFIX`: of the functions, only those whose name starts with
    /// PREFIX.
    pub fn_prefix: Option<String>,
    /// `--layout`: the layout table, no sections.
    pub layout: bool,
    /// `--macro NAME`: the macro-time module of the macro NAME, no
    /// sections.
    pub macro_name: Option<String>,
}

impl Options {
    /// Whether the dump prints section `s`.
    pub fn wants(&self, s: Section) -> bool {
        match (&self.sections, &self.fn_prefix) {
            (Some(set), _) => set.contains(&s),
            (None, Some(_)) => s == Section::Fns,
            (None, None) => true,
        }
    }
}

/// What one file prints, and the status it adds: 0 sections, 1 an error
/// record, 2 an internal error, 3 `unsupported`.
struct One {
    text: String,
    status: u8,
}

impl One {
    fn ok(text: String) -> One {
        One { text, status: 0 }
    }
}

impl From<Stop> for One {
    fn from(stop: Stop) -> One {
        match stop {
            Stop::Record(text) => One { text, status: 1 },
            Stop::Internal(m) => One {
                text: format!("internal error: {m}\n"),
                status: 2,
            },
            // The first word of the message and nothing else: what a
            // program the compiler cannot lower yet is told by is its
            // status, the messages being for people (§8.4).
            Stop::Unsupported(m) => One {
                text: match m.split_whitespace().next() {
                    Some(word) => format!("unsupported {word}\n"),
                    None => "unsupported\n".to_string(),
                },
                status: 3,
            },
        }
    }
}

/// What `fibc emit-dump` prints for `files` and the status it exits with:
/// each file under a `== FILE` header, its dump or the line `unreadable`;
/// status 0 if every file printed its sections, 1 if one ended in an error
/// record, 2 if one was unreadable or failed inside the tool, 3 if one was
/// `unsupported` (the larger wins). Runs on a thread with the stack the
/// expander and the checker need.
pub fn emit_files(files: &[String], roots: &Roots, opts: &Options) -> (String, u8) {
    thread::scope(|scope| {
        let worker = thread::Builder::new()
            .name("fibc-emit-dump".into())
            .stack_size(STACK_BYTES.max(CHECK_STACK))
            .spawn_scoped(scope, || files_here(files, roots, opts));
        match worker.map(|h| h.join()) {
            Ok(Ok(done)) => done,
            Ok(Err(_)) => ("internal error: the emit dump panicked\n".to_string(), 2),
            Err(e) => (format!("cannot start the emit dump: {e}\n"), 2),
        }
    })
}

fn files_here(files: &[String], roots: &Roots, opts: &Options) -> (String, u8) {
    let mut text = String::new();
    let mut status = 0u8;
    for file in files {
        text.push_str(&format!("== {file}\n"));
        match std::fs::read_to_string(file) {
            Ok(source) => {
                let one = dump_file(&source, file, roots, opts);
                status = status.max(one.status);
                text.push_str(&one.text);
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
fn dump_file(source: &str, file: &str, roots: &Roots, opts: &Options) -> One {
    match dump_text(source, file, roots, opts) {
        Ok(text) => One::ok(text),
        Err(stop) => stop.into(),
    }
}

fn dump_text(source: &str, file: &str, roots: &Roots, opts: &Options) -> Result<String, Stop> {
    let expanded = front::expand(source, file, roots)?;
    if let Some(name) = &opts.macro_name {
        return front::macro_text(&expanded, name, file);
    }
    if opts.layout {
        return Ok(layout::text(&front::typed(&expanded, file)?));
    }
    let checked = front::checked(&expanded, file)?;
    let module = compile_module(&checked, false)?;
    Ok(sections::text(&module, opts))
}
