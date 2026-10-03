//! What the fusion rewrite reads off the expanded forms before it rewrites
//! any: the names a top-level form binds anywhere (**B**), the names a
//! module defines at top level, and whether a form calls a terminal
//! consumer at all. Every scan is iterative (an explicit stack), so the
//! nesting of a program costs heap, not native stack, and every scan
//! over-approximates: a name that is bound, defined or called where it
//! need not be only keeps a call from being fused, never the other way.

use std::collections::HashSet;

use crate::syntax::{Form, FormKind};

use super::super::build::head_name;
use super::super::private::{marker_index, type_name};
use super::tables::{base_head, terminal};

/// Adds every symbol inside `form` to `out`; a name written `x:` is added
/// as `x:` and as `x`.
fn add_symbols(form: &Form, out: &mut HashSet<String>) {
    let mut stack = vec![form];
    while let Some(f) = stack.pop() {
        match &f.kind {
            FormKind::Sym(s) => {
                out.insert(s.clone());
                if let Some(bare) = s.strip_suffix(':').filter(|b| !b.is_empty()) {
                    out.insert(bare.to_string());
                }
            }
            FormKind::List(items) | FormKind::Vec(items) | FormKind::Map(items) => {
                stack.extend(items.iter());
            }
            _ => {}
        }
    }
}

/// The names `items` (a list whose head is `head`) binds in its own
/// positions: a `fn`'s name and parameters, the patterns (and the typed
/// names) of `let`, `loop` and `match`, a `defun`'s name and parameters, the parameters of the
/// methods of an `impl` or a `defprotocol`.
fn binds(head: &str, items: &[Form], out: &mut HashSet<String>) {
    match head {
        "fn" => {
            let named = items.get(1).is_some_and(|f| f.as_sym().is_some());
            let params = if named { 2 } else { 1 };
            if named {
                add_symbols(&items[1], out);
            }
            items.get(params).inspect(|p| add_symbols(p, out));
        }
        "let" | "loop" => {
            let pairs = items.get(1).and_then(Form::as_list).unwrap_or(&[]);
            for pair in pairs {
                // `(pattern expr)` binds the pattern; `(name: type expr)`
                // binds the name and says the type: never the expression.
                let parts = pair.as_list().unwrap_or(&[]);
                let bound = if parts.len() == 3 { 2 } else { 1 };
                parts.iter().take(bound).for_each(|p| add_symbols(p, out));
            }
        }
        "match" => {
            for clause in items.iter().skip(2) {
                let first = clause.as_list().and_then(<[Form]>::first);
                first.inspect(|p| add_symbols(p, out));
            }
        }
        "defun" => {
            items.get(1).inspect(|n| add_symbols(n, out));
            let params = items.iter().skip(2).find(|f| f.as_list().is_some());
            params.inspect(|p| add_symbols(p, out));
        }
        "impl" | "defprotocol" => {
            for method in items.iter().skip(2).filter_map(Form::as_list) {
                method
                    .get(1)
                    .filter(|p| p.as_list().is_some())
                    .inspect(|p| add_symbols(p, out));
            }
        }
        _ => {}
    }
}

/// **B** for one top-level form: every symbol it binds anywhere.
pub(super) fn bound_names(form: &Form) -> HashSet<String> {
    let mut out = HashSet::new();
    let mut stack = vec![form];
    while let Some(f) = stack.pop() {
        match &f.kind {
            FormKind::List(items) => {
                let head = head_name(f).unwrap_or("");
                if head == "quote" {
                    continue;
                }
                binds(head, items, &mut out);
                stack.extend(items.iter());
            }
            FormKind::Vec(items) | FormKind::Map(items) => stack.extend(items.iter()),
            _ => {}
        }
    }
    out
}

/// Whether `form` has a call of a terminal consumer anywhere (a prefilter:
/// a form without one has nothing to fuse, and is not walked again).
pub(super) fn calls_a_terminal(form: &Form) -> bool {
    let mut stack = vec![form];
    while let Some(f) = stack.pop() {
        match &f.kind {
            FormKind::List(items) => {
                let argc = items.len().saturating_sub(1);
                let name = head_name(f).map(|h| base_head(h, argc).1);
                if name.is_some_and(|n| terminal(n, argc).is_some()) {
                    return true;
                }
                stack.extend(items.iter());
            }
            FormKind::Vec(items) | FormKind::Map(items) => stack.extend(items.iter()),
            _ => {}
        }
    }
    false
}

/// The names a module defines at top level, and which of them are public.
#[derive(Default)]
pub(crate) struct Defined {
    pub all: HashSet<String>,
    pub public: HashSet<String>,
}

impl Defined {
    fn add(&mut self, name: &str, private: bool) {
        self.all.insert(name.to_string());
        if !private {
            self.public.insert(name.to_string());
        }
    }
}

/// The name of a variant of a `defenum` form: a bare symbol or `(Name f: T ..)`.
fn variant_name(f: &Form) -> Option<&str> {
    match &f.kind {
        FormKind::Sym(s) => Some(s.as_str()),
        FormKind::List(parts) => parts.first().and_then(Form::as_sym),
        _ => None,
    }
}

/// The name of a method of a `defprotocol` form: a list `(name (self ..) ..)`.
fn method_name(f: &Form) -> Option<&str> {
    let parts = f.as_list()?;
    parts.get(1)?.as_list()?;
    parts.first()?.as_sym()
}

/// The names one top-level form defines (`defn` and `defn-`, which are
/// macros over `defun`, as well as `defun`: a module's own definitions are
/// also read off its unexpanded forms, `own`).
pub(crate) fn names_of(form: &Form) -> Vec<&str> {
    let items = form.as_list().unwrap_or(&[]);
    let name = |i: usize| items.get(i).and_then(Form::as_sym);
    match head_name(form) {
        Some("defun" | "extern" | "defn" | "defn-") => name(1).into_iter().collect(),
        Some("def") => name(1)
            .map(|n| n.strip_suffix(':').unwrap_or(n))
            .into_iter()
            .collect(),
        Some("defstruct") => type_name(form).into_iter().collect(),
        Some("defenum") => {
            let mut all: Vec<&str> = type_name(form).into_iter().collect();
            all.extend(items.iter().skip(2).filter_map(variant_name));
            all
        }
        Some("defprotocol") => items.iter().skip(2).filter_map(method_name).collect(),
        _ => Vec::new(),
    }
}

/// The names the top-level forms define: functions, constants, externs,
/// the methods of protocols, structs, enums and their variants.
pub(crate) fn defined_names(forms: &[Form]) -> Defined {
    let mut out = Defined::default();
    for form in forms {
        let private = marker_index(form).is_some();
        names_of(form).into_iter().for_each(|n| out.add(n, private));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::read_all;

    fn forms(src: &str) -> Vec<Form> {
        read_all(src, "t.fib").unwrap_or_else(|e| panic!("{src:?}: {e}"))
    }

    fn bound(src: &str) -> Vec<String> {
        let mut names: Vec<String> = bound_names(&forms(src)[0]).into_iter().collect();
        names.sort();
        names
    }

    #[test]
    fn every_binder_of_a_form_is_bound() {
        assert_eq!(
            bound(
                "(defun f (a: i64 b) (let ((x 1) (y: i64 2)) (fn g (z) (match x ((some w) w)))))"
            ),
            ["a", "a:", "b", "f", "g", "i64", "some", "w", "x", "y", "y:", "z"]
        );
        assert_eq!(bound("(loop ((i 0)) (recur i))"), ["i"]);
    }

    #[test]
    fn the_expression_of_a_binding_binds_nothing() {
        assert_eq!(bound("(let ((s (map f (filter p v)))) s)"), ["s"]);
        assert_eq!(bound("(let ((n: i64 (count v))) n)"), ["i64", "n", "n:"]);
        assert_eq!(bound("(loop ((acc (take 2 v))) acc)"), ["acc"]);
    }

    #[test]
    fn what_is_only_called_or_quoted_is_not_bound() {
        assert!(bound("(defun f () (count (map g v)))").contains(&"f".to_string()));
        assert!(!bound("(defun f () (count (map g v)))").contains(&"count".to_string()));
        assert_eq!(
            bound("(quote (let ((count 1)) count))"),
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_terminal_call_is_seen_by_name_and_arity() {
        let call = |s: &str| calls_a_terminal(&forms(s)[0]);
        assert!(call("(defun f () (g (count v)))"));
        assert!(call("(defun f () (fib.seq/reduce a b c))"));
        assert!(!call("(defun f () (reduce a c))"));
        assert!(call("(defun f () (g (sort$1 v)))"));
        assert!(!call("(defun f () (first v))"));
    }

    #[test]
    fn a_module_defines_what_its_top_level_forms_name() {
        let d = defined_names(&forms(
            "(defun f () 1) (defun g :private () 2) (def c: i64 3) (defstruct S (a: i64)) \
             (defenum E A (B x: i64)) (defprotocol P (m (self) -> i64)) (extern e (i64) -> i64)",
        ));
        let mut all: Vec<&str> = d.all.iter().map(String::as_str).collect();
        all.sort_unstable();
        assert_eq!(all, ["A", "B", "E", "S", "c", "e", "f", "g", "m"]);
        assert!(d.all.contains("g") && !d.public.contains("g"));
        assert!(d.public.contains("f") && d.public.contains("m"));
    }
}
