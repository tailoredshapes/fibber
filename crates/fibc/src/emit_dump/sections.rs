//! The sections of a module text (spec/bootstrap.md §8.3): the pieces
//! [`crate::compile::assemble_parts`] puts together, each under its line.

use crate::compile::Module;

use super::{Options, Section};

/// The line that starts a section; with these lines left out, the sections
/// of an unrestricted dump are the module text `fibc emit` prints.
pub(crate) const HEADER: &str = ";; == section ";

/// The sections of `module` that `opts` asks for, in the order of the dump,
/// each under `;; == section NAME`; the `fns` section holds the functions
/// whose name starts with `--fn`'s prefix, or all of them.
pub(crate) fn text(module: &Module, opts: &Options) -> String {
    let mut out = String::new();
    for s in Section::ALL {
        if !opts.wants(s) {
            continue;
        }
        out.push_str(HEADER);
        out.push_str(s.name());
        out.push('\n');
        let p = &module.parts;
        match s {
            Section::Runtime => out.push_str(&p.runtime),
            Section::Externs => out.push_str(&p.externs),
            Section::Types => out.push_str(&p.types),
            Section::Statics => out.push_str(&p.statics),
            Section::Keywords => out.push_str(&p.keywords),
            Section::Defs => out.push_str(&p.defs),
            Section::Quotes => out.push_str(&p.quotes),
            Section::Fns => push_fns(&mut out, &p.fns, opts.fn_prefix.as_deref()),
            Section::Main => out.push_str(&module.main),
        }
    }
    out
}

fn push_fns(out: &mut String, fns: &[String], prefix: Option<&str>) {
    for f in fns {
        if prefix.is_none_or(|p| fn_name(f).is_some_and(|n| n.starts_with(p))) {
            out.push_str(f);
        }
    }
}

/// The name of the function a `(define ..)` text defines: the word after
/// the first parenthesis that follows the modifiers, as in `(define
/// internal tailcc (f.sq i64) ..)`.
pub(crate) fn fn_name(text: &str) -> Option<&str> {
    let rest = text.strip_prefix("(define ")?;
    let head = &rest[rest.find('(')? + 1..];
    let end = head.find(|c: char| c.is_whitespace() || c == ')')?;
    Some(&head[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_function_is_named_by_the_head_after_its_modifiers() {
        assert_eq!(
            fn_name("(define internal tailcc (f.sq i64) ((i64 p0))\n  (block entry))\n"),
            Some("f.sq")
        );
        assert_eq!(fn_name("(define (main i32) ((i32 argc)))"), Some("main"));
        assert_eq!(
            fn_name("(define internal (kw.show ptr) ((i64 k))"),
            Some("kw.show")
        );
        assert_eq!(fn_name("(declare printf i32 (ptr ...))"), None);
        assert_eq!(fn_name("(define internal"), None);
    }
}
