//! A module's own definitions beat the prelude macros (tranche 1 R14, the
//! owner's rule of 2026-10-01: Clojure's resolution, a name is the
//! module's own, then what its `:use`s export, then the implicit library,
//! then the prelude).
//!
//! The expander finds a prelude macro by its bare name, so a program's
//! `(defun str ..)` was shadowed in head position by the macro `str`. A
//! prelude macro now declines in a module that defines the name itself:
//! as a top-level `defun`, `defn`, `defn-`, `def`, `extern`, `defstruct`
//! (its constructor), `defenum` (its variants) or a method of a
//! `defprotocol`; or that sees it exported unqualified by a `:use`d module
//! of the program (the library's modules are not counted: their functions
//! `update`, `reduce`, `str` are the twins the macros are designed to sit
//! beside, syntax §4.4). The call is then an ordinary call of the
//! program's function.
//!
//! The definitions are read off the module's unexpanded forms before the
//! first one is expanded, so a function defined below its use hides the
//! macro there too (a top-level `do` is looked through), and off each
//! form a macro produced as it is produced. A local binding (`let`, a
//! parameter) is not looked at: it does not hide a macro today and is not
//! a definition of the module.

use std::collections::HashSet;

use crate::syntax::Form;

use super::build::head_name;
use super::fuse::names_of;

/// The names the top-level forms define, looking through a top-level
/// `do`; the work list is explicit, so a deeply nested `do` costs no
/// native stack.
pub(crate) fn defined_by(forms: &[Form]) -> HashSet<String> {
    let mut out = HashSet::new();
    let mut stack: Vec<&Form> = forms.iter().rev().collect();
    while let Some(form) = stack.pop() {
        if head_name(form) == Some("do") {
            stack.extend(form.as_list().unwrap_or(&[]).iter().skip(1).rev());
        } else {
            out.extend(names_of(form).into_iter().map(str::to_string));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::read_all;

    fn names(src: &str) -> Vec<String> {
        let forms = read_all(src, "t.fib").unwrap_or_else(|e| panic!("{src:?}: {e}"));
        let mut names: Vec<String> = defined_by(&forms).into_iter().collect();
        names.sort();
        names
    }

    #[test]
    fn every_kind_of_definition_names_what_it_defines() {
        assert_eq!(
            names(
                "(defun a () 1) (defn b [x] x) (defn- c [x] x) (def d: i64 1) (extern e (i64) -> i64) \
                 (defstruct S (x: i64)) (defenum E V (W y: i64)) (defprotocol P (m (self) -> i64))"
            ),
            ["E", "S", "V", "W", "a", "b", "c", "d", "e", "m"]
        );
    }

    #[test]
    fn a_top_level_do_is_looked_through_and_an_expression_defines_nothing() {
        assert_eq!(
            names("(do (defun a () 1) (do (defun b () 2))) (f (defun c () 3)) (let ((d 1)) d)"),
            ["a", "b"]
        );
    }
}
