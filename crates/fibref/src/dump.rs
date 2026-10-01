//! The reader dump (spec/bootstrap.md §2): the forms the reader produced
//! as flat text, every node with its position, and a read error the same
//! way. The self-hosted reader (M6, `compiler/`) prints the same text, so
//! comparing the two dumps compares the two readers on every form, every
//! position and every error message.
//!
//! One line per node, depth first, two spaces of indent per level:
//!
//! ```text
//! list 2 1:1 0..9
//!   sym "def" 1:2 1..4
//!   int 42 i64 1:6 5..7
//! ```
//!
//! An error is one line, `error Kind L:C S..E: message`, where `Kind` is
//! the variant's name and the message is its `Display`.
//!
//! The print mode ([`print_source`]) compares the printer instead of the
//! positions: one line per top-level form, the text `Display` gives it.

use std::fmt::Write;

use crate::eval::arith::float_text;
use crate::syntax::{read_all, Form, FormKind, Pos, ReadError, ReadErrorKind};
use crate::types::ty::Scalar;

/// The dump of reading `source` as `file`: its forms, or its error.
pub fn dump_source(source: &str, file: &str) -> String {
    match read_all(source, file) {
        Ok(forms) => dump_forms(&forms),
        Err(e) => dump_error(&e),
    }
}

/// The printed text of reading `source` as `file`: one line per top-level
/// form, its `Display` text (which has no line break: strings and
/// characters escape every one), or the one line of the read error, as in
/// [`dump_source`].
pub fn print_source(source: &str, file: &str) -> String {
    match read_all(source, file) {
        Ok(forms) => forms.iter().map(|f| format!("{f}\n")).collect(),
        Err(e) => dump_error(&e),
    }
}

/// The dump of a sequence of top-level forms.
pub fn dump_forms(forms: &[Form]) -> String {
    let mut out = String::new();
    for f in forms {
        dump_form(f, 0, None, &mut out);
    }
    out
}

/// The dump of a sequence of top-level forms of the file `home`, as the
/// expansion dump prints them (spec/bootstrap.md §5): the reader dump's
/// lines, except that a position in another file than `home` ends with
/// `@FILE` (see [`span_in`]).
pub fn dump_forms_in(forms: &[Form], home: &str) -> String {
    let mut out = String::new();
    for f in forms {
        dump_form(f, 0, Some(home), &mut out);
    }
    out
}

/// The dump of a read error: one line.
pub fn dump_error(e: &ReadError) -> String {
    format!(
        "error {} {}: {}\n",
        kind_name(&e.kind),
        span(&e.pos),
        e.kind
    )
}

fn span(p: &Pos) -> String {
    format!("{}:{} {}..{}", p.line, p.col, p.start, p.end)
}

/// A position as the expansion dump prints it: [`span`], and `@FILE`
/// after it when the position is in another file than `home`, the file of
/// the module being dumped. A form a macro of another module built from
/// its own template, or a built-in definition (`<builtin>`), is the case.
pub(crate) fn span_in(p: &Pos, home: &str) -> String {
    let at = span(p);
    if &*p.file == home {
        at
    } else {
        format!("{at}@{}", p.file)
    }
}

/// One node of the dump and what it holds, at `depth`: the reader dump
/// with no `home`, the expansion dump with one.
pub(crate) fn dump_form(f: &Form, depth: usize, home: Option<&str>, out: &mut String) {
    let indent = "  ".repeat(depth);
    let at = home.map_or_else(|| span(&f.pos), |h| span_in(&f.pos, h));
    let seq = |tag: &str, items: &[Form], out: &mut String| {
        let _ = writeln!(out, "{indent}{tag} {} {at}", items.len());
        for item in items {
            dump_form(item, depth + 1, home, out);
        }
    };
    match &f.kind {
        FormKind::Sym(s) => {
            let _ = writeln!(out, "{indent}sym {} {at}", quote(s));
        }
        FormKind::Kw(s) => {
            let _ = writeln!(out, "{indent}kw {} {at}", quote(s));
        }
        FormKind::Int { v, width } => {
            let _ = writeln!(out, "{indent}int {v} {} {at}", width.suffix());
        }
        FormKind::Flt { v, width } => {
            let scalar = match width {
                crate::syntax::FltWidth::F32 => Scalar::F32,
                crate::syntax::FltWidth::F64 => Scalar::F64,
            };
            let _ = writeln!(
                out,
                "{indent}flt {} {} {at}",
                float_text(*v, scalar),
                width.suffix()
            );
        }
        FormKind::Str(s) => {
            let _ = writeln!(out, "{indent}str {} {at}", quote(s));
        }
        FormKind::Chr(c) => {
            let _ = writeln!(out, "{indent}chr U+{:04X} {at}", u32::from(*c));
        }
        FormKind::Bool(b) => {
            let _ = writeln!(out, "{indent}bool {b} {at}");
        }
        FormKind::Nil => {
            let _ = writeln!(out, "{indent}nil {at}");
        }
        FormKind::List(items) => seq("list", items, out),
        FormKind::Vec(items) => seq("vec", items, out),
        FormKind::Map(items) => seq("map", items, out),
    }
}

/// `"`, `\` and the control characters escaped, every other character
/// as it is: the text is unambiguous and needs no printer to produce.
pub(crate) fn quote(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c if u32::from(c) < 0x20 || u32::from(c) == 0x7F => {
                let _ = write!(out, "\\x{:02X}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The name of an error's variant.
pub fn kind_name(k: &ReadErrorKind) -> &'static str {
    use ReadErrorKind as K;
    match k {
        K::UnterminatedString => "UnterminatedString",
        K::BadEscape { .. } => "BadEscape",
        K::BadUnicodeEscape { .. } => "BadUnicodeEscape",
        K::BadCharLiteral(_) => "BadCharLiteral",
        K::InvalidCharacter(_) => "InvalidCharacter",
        K::InvalidNumber { .. } => "InvalidNumber",
        K::IntegerOutOfRange { .. } => "IntegerOutOfRange",
        K::FloatOutOfRange { .. } => "FloatOutOfRange",
        K::InvalidSymbol(_) => "InvalidSymbol",
        K::InvalidKeyword(_) => "InvalidKeyword",
        K::UnknownDispatch(_) => "UnknownDispatch",
        K::PrefixWithoutForm(_) => "PrefixWithoutForm",
        K::DiscardWithoutForm => "DiscardWithoutForm",
        K::InOutNotSymbol => "InOutNotSymbol",
        K::Unclosed { .. } => "Unclosed",
        K::UnexpectedClose { .. } => "UnexpectedClose",
        K::MismatchedClose { .. } => "MismatchedClose",
        K::OddMapEntries { .. } => "OddMapEntries",
        K::TooDeep { .. } => "TooDeep",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_list_with_atoms() {
        let d = dump_source("(def 42 2.5f32 \"a\\n\")", "t");
        assert_eq!(
            d,
            "list 4 1:1 0..21\n  sym \"def\" 1:2 1..4\n  int 42 i64 1:6 5..7\n  flt 2.5 f32 1:9 8..14\n  str \"a\\n\" 1:16 15..20\n"
        );
    }

    #[test]
    fn prefix_forms_dump_as_lists() {
        let d = dump_source("'x &y", "t");
        assert_eq!(
            d,
            "list 2 1:1 0..2\n  sym \"quote\" 1:1 0..1\n  sym \"x\" 1:2 1..2\nlist 2 1:4 3..5\n  sym \"&\" 1:4 3..4\n  sym \"y\" 1:5 4..5\n"
        );
    }

    #[test]
    fn chars_bools_nil_keywords_collections() {
        let d = dump_source("[\\a true nil :k/n {1 2}]", "t");
        assert!(d.contains("chr U+0061 1:2 1..3"), "{d}");
        assert!(d.contains("bool true 1:5 4..8"), "{d}");
        assert!(d.contains("nil 1:10 9..12"), "{d}");
        assert!(d.contains("kw \"k/n\" 1:14 13..17"), "{d}");
        assert!(d.contains("map 2 1:19 18..23"), "{d}");
    }

    #[test]
    fn an_error_is_one_line_with_its_kind_and_message() {
        let d = dump_source("(a", "t");
        assert_eq!(d, "error Unclosed 1:1 0..1: unclosed (\n");
        let d = dump_source("300i8", "t");
        assert_eq!(
            d,
            "error IntegerOutOfRange 1:1 0..5: integer literal 300i8 does not fit i8 (-128 to 127)\n"
        );
    }

    #[test]
    fn print_source_is_one_line_per_form() {
        let p = print_source("'x [1 2.50f32 \"a\\tb\"]\n{\\a nil}", "t");
        assert_eq!(p, "(quote x)\n[1 2.5f32 \"a\\tb\"]\n{\\a nil}\n");
        assert_eq!(print_source("", "t"), "");
    }

    #[test]
    fn print_source_of_an_error_is_the_dump_line() {
        assert_eq!(print_source("(a", "t"), dump_source("(a", "t"));
        assert_eq!(
            print_source("(a", "t"),
            "error Unclosed 1:1 0..1: unclosed (\n"
        );
    }

    #[test]
    fn print_source_keeps_a_form_with_awkward_text_on_one_line() {
        // A string with a line break and a forbidden character prints escaped.
        let p = print_source("\"a\\nb\\u{A0}\\0\" 1e16 0.00001", "t");
        assert_eq!(p, "\"a\\nb\\u{A0}\\0\"\n1e16\n1e-5\n");
    }

    #[test]
    fn a_position_in_another_file_than_home_ends_with_that_file() {
        let forms = crate::syntax::read_all("(a [b])", "other.fib").expect("reads");
        let same = dump_forms_in(&forms, "other.fib");
        assert_eq!(same, dump_forms(&forms));
        assert_eq!(
            dump_forms_in(&forms, "home.fib"),
            "list 2 1:1 0..7@other.fib\n  sym \"a\" 1:2 1..2@other.fib\n  \
             vec 1 1:4 3..6@other.fib\n    sym \"b\" 1:5 4..5@other.fib\n"
        );
    }

    #[test]
    fn quoting_escapes_quotes_backslashes_and_controls_only() {
        assert_eq!(
            quote("a\"b\\c\n\u{1}\u{7F}é😀"),
            "\"a\\\"b\\\\c\\n\\x01\\x7Fé😀\""
        );
    }

    #[test]
    fn every_error_variant_has_a_name() {
        // A new variant without a name is a compile error in kind_name;
        // this pins that the names are the variants' own.
        assert_eq!(
            kind_name(&ReadErrorKind::DiscardWithoutForm),
            "DiscardWithoutForm"
        );
        assert_eq!(kind_name(&ReadErrorKind::TooDeep { limit: 1 }), "TooDeep");
    }
}
