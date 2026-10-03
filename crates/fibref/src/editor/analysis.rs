//! Running the front end on an editor buffer and keeping what it says,
//! whether the buffer is good or not: the diagnostics from the first
//! error, and a checked program to read names and types from. A buffer
//! that does not check still gets a program: the library and the
//! modules its `ns` form names, checked without the rest of the buffer.

use crate::expand::{ExpandCtx, NoRunner};
use crate::modules::{self, LoadError};
use crate::own::{check_modules_with, CheckError};
use crate::roots::Roots;
use crate::syntax::Pos;
use crate::types::{prelude_forms, TypedProgram};

/// One problem in a buffer, as a byte range of its text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diag {
    pub start: usize,
    pub end: usize,
    pub message: String,
}

/// What the front end made of a buffer.
pub struct Analysis {
    /// The buffer's program, or the library's and its imports' when the
    /// buffer itself does not check.
    pub typed: Option<TypedProgram>,
    /// Whether `typed` includes the buffer's own definitions.
    pub own: bool,
    /// The errors, empty for a buffer that checks.
    pub diags: Vec<Diag>,
}

/// Reads, expands, types and ownership-checks `source` (the text of the
/// file `path`) as the main module, with the library found through
/// `roots`; on any failure the diagnostics, and a program for the
/// library and the `ns` imports alone.
pub fn analyse(source: &str, path: &str, roots: &Roots) -> Analysis {
    analyse_at(source, path, roots, None)
}

/// [`analyse`] for a cursor at byte `cursor`: when the buffer does not
/// check, the program is built from the library, the `ns` form and the
/// top-level forms other than the one the cursor is in, then from the
/// type definitions among them, then from the `ns` form alone.
pub fn analyse_at(source: &str, path: &str, roots: &Roots, cursor: Option<usize>) -> Analysis {
    let main = source.contains("(defun main");
    match run(source, path, roots, main) {
        Ok(typed) => Analysis {
            typed: Some(typed),
            own: true,
            diags: Vec::new(),
        },
        Err(diags) => {
            let rungs = [
                mask(source, |c, i| {
                    !c.contains(cursor.unwrap_or(usize::MAX)) || i == 0
                }),
                mask(source, |c, i| {
                    i == 0 || (is_type_form(c, source) && !c.contains(cursor.unwrap_or(usize::MAX)))
                }),
                ns_form(source),
                String::new(),
            ];
            let typed = rungs.iter().find_map(|s| run(s, path, roots, false).ok());
            Analysis {
                typed,
                own: false,
                diags,
            }
        }
    }
}

/// A top-level form of a buffer, as a byte range.
#[derive(Clone, Copy)]
struct Chunk {
    start: usize,
    end: usize,
}

impl Chunk {
    /// Whether the cursor is in it (at its end counts: the form is
    /// still being typed).
    fn contains(&self, off: usize) -> bool {
        self.start < off && off <= self.end
    }
}

/// The top-level forms of `src`, found by delimiters alone, so that an
/// unclosed form is one that runs to the end.
fn chunks(src: &str) -> Vec<Chunk> {
    let mut d = Delims::default();
    let mut out = Vec::new();
    let mut start = None;
    for (i, c) in src.char_indices() {
        let idle = d.idle();
        d.step(c);
        if start.is_none() && idle && c == '(' {
            start = Some(i);
        }
        if let Some(s) = start.filter(|_| d.idle()) {
            out.push(Chunk {
                start: s,
                end: i + 1,
            });
            start = None;
        }
    }
    out.extend(start.map(|s| Chunk {
        start: s,
        end: src.len(),
    }));
    out
}

fn is_type_form(c: &Chunk, src: &str) -> bool {
    let text = &src[c.start..c.end];
    ["(defstruct", "(defenum", "(defprotocol", "(defrecord"]
        .iter()
        .any(|k| text.starts_with(k))
}

/// `src` with the top-level forms that `keep` rejects blanked out
/// (newlines kept, so that offsets and lines do not move). `keep` is
/// given the form and its index; the `ns` form is index 0 when first.
fn mask(src: &str, keep: impl Fn(&Chunk, usize) -> bool) -> String {
    let mut out = src.to_string();
    let all = chunks(src);
    let ns_first = all.first().is_some_and(|c| is_ns(&src[c.start..]));
    for (i, c) in all.iter().enumerate() {
        let is_ns_form = ns_first && i == 0;
        if !keep(c, usize::from(!is_ns_form)) {
            let blank: String = src[c.start..c.end]
                .chars()
                .map(|ch| {
                    if ch == '\n' {
                        "\n".to_string()
                    } else {
                        " ".repeat(ch.len_utf8())
                    }
                })
                .collect();
            out.replace_range(c.start..c.end, &blank);
        }
    }
    out
}

fn run(source: &str, path: &str, roots: &Roots, main: bool) -> Result<TypedProgram, Vec<Diag>> {
    let at = Located { source, path };
    let loaded = modules::try_load_in(source, path, roots).map_err(|e| vec![at.load(&e)])?;
    let mut ctx = ExpandCtx::new();
    let prelude = prelude_forms(&mut ctx).map_err(|m| vec![at.plain(m)])?;
    let expanded = modules::expand_all(loaded, &mut ctx, &mut NoRunner)
        .map_err(|e| vec![at.range(&e.pos, e.kind.to_string())])?;
    match check_modules_with(&expanded, &prelude, main) {
        Ok(checked) => Ok(checked.typed),
        Err(CheckError::Type(es)) => Err(es
            .iter()
            .map(|e| at.range(&e.pos, e.message.clone()))
            .collect()),
        Err(CheckError::Own(es)) => Err(es
            .iter()
            .map(|e| at.range(&e.pos, e.message.clone()))
            .collect()),
        Err(e) => Err(vec![at.plain(e.to_string())]),
    }
}

/// Turns the front end's positions into ranges of one buffer.
struct Located<'a> {
    source: &'a str,
    path: &'a str,
}

impl Located<'_> {
    fn plain(&self, message: String) -> Diag {
        Diag {
            start: 0,
            end: first_line_end(self.source),
            message,
        }
    }

    /// An error at `pos`; one in another file (a module the buffer
    /// requires) is shown at the top of this buffer with its place.
    fn range(&self, pos: &Pos, message: String) -> Diag {
        if &*pos.file != self.path {
            return self.plain(format!("{pos}: {message}"));
        }
        let end = pos.end.clamp(pos.start, self.source.len());
        Diag {
            start: pos.start.min(self.source.len()),
            end: end.max(pos.start.min(self.source.len())),
            message,
        }
    }

    fn load(&self, e: &LoadError) -> Diag {
        match e {
            LoadError::Read(r) => self.range(&r.pos, r.kind.to_string()),
            LoadError::Spec { pos, what } => self.range(pos, what.clone()),
            other => self.plain(other.to_string()),
        }
    }
}

fn first_line_end(source: &str) -> usize {
    source.find('\n').unwrap_or(source.len())
}

/// The text of the first top-level form when it is `(ns ..)`, else empty:
/// the part of a broken buffer that says what it imports. A form that
/// never closes is closed here.
pub fn ns_form(source: &str) -> String {
    let start = match first_form_start(source) {
        Some(i) if is_ns(&source[i..]) => i,
        _ => return String::new(),
    };
    let rest = &source[start..];
    let mut scan = Delims::default();
    for (i, c) in rest.char_indices() {
        scan.step(c);
        if scan.open.is_empty() {
            return rest[..i + c.len_utf8()].to_string();
        }
    }
    format!("{rest}{}", scan.closers())
}

fn is_ns(text: &str) -> bool {
    text.strip_prefix("(ns")
        .is_some_and(|r| r.starts_with(|c: char| c.is_whitespace() || c == ')'))
}

fn first_form_start(source: &str) -> Option<usize> {
    let mut off = 0;
    for line in source.split_inclusive('\n') {
        let t = line.trim_start();
        if !t.is_empty() && !t.starts_with(';') {
            return Some(off + line.len() - t.len());
        }
        off += line.len();
    }
    None
}

/// A scan of text for the delimiters still open at its end, aware of
/// strings, comments and character literals.
#[derive(Default)]
pub struct Delims {
    pub open: Vec<char>,
    in_string: bool,
    in_comment: bool,
    escaped: bool,
}

impl Delims {
    /// Reads one more character.
    pub fn step(&mut self, c: char) {
        if self.in_comment {
            self.in_comment = c != '\n';
        } else if self.escaped {
            self.escaped = false;
        } else if self.in_string {
            match c {
                '\\' => self.escaped = true,
                '"' => self.in_string = false,
                _ => {}
            }
        } else {
            self.code(c);
        }
    }

    fn code(&mut self, c: char) {
        match c {
            ';' => self.in_comment = true,
            '"' => self.in_string = true,
            '\\' => self.escaped = true,
            '(' | '[' | '{' => self.open.push(c),
            ')' | ']' | '}' => {
                self.open.pop();
            }
            _ => {}
        }
    }

    /// Whether nothing is open: not in a form, a string or a comment.
    pub fn idle(&self) -> bool {
        self.open.is_empty() && !self.in_string && !self.in_comment && !self.escaped
    }

    /// Whether the text ends inside a string.
    pub fn in_string(&self) -> bool {
        self.in_string
    }

    /// The text that closes what is open, innermost first.
    pub fn closers(&self) -> String {
        self.open
            .iter()
            .rev()
            .map(|c| match c {
                '(' => ')',
                '[' => ']',
                _ => '}',
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roots() -> Roots {
        Roots::default()
    }

    #[test]
    fn a_good_buffer_has_no_diagnostics_and_its_own_definitions() {
        let a = analyse(
            "(defun inc (x: i64) -> i64 (+ x 1))\n",
            "/nonexistent/a.fib",
            &roots(),
        );
        assert!(a.diags.is_empty(), "{:?}", a.diags);
        assert!(a.own && a.typed.is_some());
    }

    #[test]
    fn a_type_error_is_located_in_the_buffer_and_the_library_is_still_there() {
        let src = "(defun inc (x: i64) -> i64 (+ x \"a\"))\n";
        let a = analyse(src, "/nonexistent/a.fib", &roots());
        assert_eq!(a.diags.len(), 1, "{:?}", a.diags);
        assert!(a.diags[0].end <= src.len() && a.diags[0].start < a.diags[0].end);
        assert!(!a.own);
        assert!(a.typed.is_some());
    }

    #[test]
    fn an_unreadable_buffer_is_one_read_error_at_its_place() {
        let src = "(defun a () -> i64 1)\n(defun b (\n";
        let a = analyse(src, "/nonexistent/a.fib", &roots());
        assert_eq!(a.diags.len(), 1);
        assert!(a.diags[0].start >= src.find("(defun b").unwrap_or(0));
        assert!(a.typed.is_some());
    }

    #[test]
    fn the_ns_form_of_a_broken_buffer_is_closed_for_reading() {
        assert_eq!(
            ns_form("; c\n(ns a (:require [x :as y]))\n(defun"),
            "(ns a (:require [x :as y]))"
        );
        assert_eq!(ns_form("(ns a (:use \"b)\"\n"), "(ns a (:use \"b)\"\n))");
        assert_eq!(ns_form("(defun f)"), "");
    }
}
