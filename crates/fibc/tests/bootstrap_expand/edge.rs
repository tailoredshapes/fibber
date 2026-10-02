//! Fixed inputs, one for each way expansion can fail and for the rewrites
//! that the cases show only in passing (spec/bootstrap.md §5). Each is
//! a program of one file, or of a few when the error is about modules,
//! named after what it provokes.

use crate::gen::{Input, Kind};

/// A program of one file.
fn one(name: &str, text: &str) -> Input {
    program(name, &[("main.fib", text)])
}

fn program(name: &str, files: &[(&str, &str)]) -> Input {
    Input {
        kind: Kind::Edge,
        name: format!("edge-{name}"),
        files: files
            .iter()
            .map(|(n, t)| (n.to_string(), t.to_string()))
            .collect(),
    }
}

/// The edge inputs, in a fixed order.
pub fn edge_inputs() -> Vec<Input> {
    let mut all = errors();
    all.extend(macro_errors());
    all.extend(rewrites());
    all.extend(modules());
    all.extend(deep());
    all
}

/// Inputs that nest deeply, in the source or by what a macro makes of
/// them: an expander that recurses on the native stack, as the Rust one
/// does not (it walks with an explicit stack), would fail on them.
fn deep() -> Vec<Input> {
    let nest = |open: &str, close: &str, n: usize, core: &str| {
        format!(
            "(defun f (x) -> i64 {}{core}{})\n",
            open.repeat(n),
            close.repeat(n)
        )
    };
    let operands = |n: usize| vec!["x"; n].join(" ");
    vec![
        one("deep-do-900", &nest("(do ", ")", 900, "x")),
        one("deep-let-400", &nest("(let ((a x)) ", ")", 400, "a")),
        one("deep-match-300", &nest("(match x (_ ", "))", 300, "1")),
        one("deep-quasiquote-150", &nest("`(a ", ")", 150, ",x")),
        one(
            "deep-and-700",
            &format!("(defun f (x) -> i64 (and {}))\n", operands(700)),
        ),
        one(
            "too-deep-and-2100",
            &format!("(defun f (x) -> i64 (and {}))\n", operands(2100)),
        ),
        one(
            "wide-vector-1000",
            &format!("(defun f (x) -> i64 (g [{}]))\n", operands(1000)),
        ),
    ]
}

/// Errors of the core forms and the prelude macros.
fn errors() -> Vec<Input> {
    vec![
        one("malformed-defun", "(defun)\n"),
        one("expression-at-top-level", "(defun f () -> i64 1)\n(f)\n"),
        one("definition-in-expression", "(defun f () -> i64 (defun g () -> i64 1))\n"),
        one("ns-not-first", "(defun f () -> i64 1)\n(ns late)\n"),
        one("nil-called", "(defun f () -> i64 (nil 1))\n"),
        one("nil-symbol-called", "(defun f () -> i64 ((nil)))\n"),
        one("brace-in-pattern", "(defun f (x) -> i64 (match x ({1 2} 1) (_ 0)))\n"),
        one("in-out-outside-argument", "(defun f () -> i64 (& x))\n"),
        one("in-out-as-argument", "(defun f (x: i64) -> i64 (g (& x) (& nil)))\n"),
        one("unquote-outside", "(defun f () -> i64 (unquote x))\n"),
        one("splice-outside", "(defun f () -> i64 `,@x)\n"),
        one("splice-inside-list", "(defun f (xs) -> i64 `(a ,@xs b))\n"),
        one("quasi-levels", "(defun f (x) -> i64 `(a `(b ,(c ,x) ,@(d ,@x))))\n"),
        one("thread-step", "(defun f (x) -> i64 (-> x 1))\n"),
        one("thread-first-last", "(defun f (x) -> i64 (+ (-> x (g 1) h) (->> x (g 1) h)))\n"),
        one("derive-protocol", "(defstruct P (a: i64))\n(derive Foo P)\n"),
        one("derive-target", "(derive Eq Nope)\n"),
        one("derive-all", "(defstruct P (a: i64 b: str))\n(defenum E (A x: i64) (B))\n(derive Eq P)(derive Ord P)(derive Hash P)(derive Show P)\n(derive Eq E)(derive Ord E)(derive Hash E)(derive Show E)\n"),
        one("derive-generic", "(defstruct (Pair a b) (fst: a snd: b))\n(derive Eq Pair)(derive Show Pair)\n"),
        one("macro-odd-map", "(defmacro m () (Map [(Int 1 :i64) (Int 2 :i64) (Int 3 :i64)]))\n(defun f () -> i64 (g (m)))\n"),
        one("arity-prelude-macro", "(defun f () -> i64 (dbg 1 2))\n"),
        one("cond-else-not-last", "(defun f (x) -> i64 (cond :else 1 x 2))\n"),
        one("let-binding", "(defun f () -> i64 (let (a) 1))\n"),
        one("limit-deep-and", "(defun f (x) -> i64 (and x x x x x x x x x x x x x x x x))\n"),
    ]
}

/// Errors and rewrites of user macros: the first block is run by a
/// runner (stage 2b), the rest fail before one is needed.
fn macro_errors() -> Vec<Input> {
    vec![
        one("macro-arity", "(defmacro m (a) a)\n(defun main () -> i64 (m 1 2))\n"),
        one("macro-arity-rest", "(defmacro m (a ... r) a)\n(defun main () -> i64 (m))\n"),
        one("macro-names-core-form", "(defmacro let (x) x)\n"),
        one("macro-names-quasiquote", "(defmacro quasiquote (x) x)\n"),
        one("macro-bad-params", "(defmacro m (a ... ) a)\n"),
        one("macro-phase", "(defun helper (x: i64) -> i64 x)\n(defmacro m (x) (Int (helper 1) :i64))\n(defun main () -> i64 (m 1))\n"),
        one("macro-failed", "(defmacro m () (trap \"no\"))\n(defun main () -> i64 (m))\n"),
        one("macro-bad-literal", "(defmacro bad () (Int 300 :i8))\n(defun main () -> i64 (bad))\n"),
        one("macro-not-a-struct", "(defmacro nf (n) (Int (count (struct-fields n)) :i64))\n(defun main () -> i64 (nf Nope))\n"),
        one("macro-not-an-enum", "(defmacro nv (n) (Int (count (enum-variants n)) :i64))\n(defun main () -> i64 (nv Nope))\n"),
        one("macro-bad-reflection", "(defmacro nf (n) (Int (count (struct-fields (Int 1 :i64))) :i64))\n(defun main () -> i64 (nf P))\n"),
        one("macro-splices-definitions", "(defmacro defrecord (name fields)\n  `(do (defstruct ,name ,fields) (derive Eq ,name)))\n(defrecord Point (x: i64 y: i64))\n(defun main () -> i64 (if (= (Point 1 2) (Point 1 2)) 1 0))\n"),
        one("macro-shadows-prelude", "(defmacro when (c ... b) `(if ,c (do ,@b) 0))\n(defun main () -> i64 (when true 1 2))\n"),
        one("macro-gensyms", "(defmacro keep (x) (let ((g (gensym \"k\"))) `(let ((,g ,x)) ,g)))\n(defun main () -> i64 (+ (keep 1) (keep 2)))\n"),
        one("macro-private-marker", "(defmacro hid :private (x) x)\n(defstruct S :private (a: i64))\n(defun main () -> i64 (hid 1))\n"),
    ]
}

/// Rewrites a case shows once: literal collections, `nil`, a spliced
/// `do`, markers, patterns, every core form at least once.
fn rewrites() -> Vec<Input> {
    vec![
        one("collections", "(defun f () -> i64 (g [] [1 [2]] {} {1 [2] 3 {}} '[a {b c}]))\n"),
        one("nil-everywhere", "(defun f (x) -> i64 (match x (nil 1) ((some nil) 2) ([nil a] 3) (_ nil)))\n"),
        one("top-level-do", "(do (defstruct A (a: i64)) (do (derive Eq A) (defun f () -> i64 1)))\n"),
        one("private-markers", "(def k :private i64 1)\n(def j: :private i64 2)\n(defun g :private () -> i64 1)\n(extern puts :private (ptr) -> i32)\n"),
        one("core-forms", "(defun f (x: i64) -> i64 (let ((a 1) (b: i64 2)) (loop ((i 0)) (if (< i x) (recur (+ i 1)) (do (async (g)) (await (h)) (unsafe (i)) (. a b) (fn (y) y) ((var f) 1) (quote (a b)) (set-field! r f 1) (trunc i32 x) (dyn Eq x) a)))))\n"),
        one("protocol-and-impl", "(defprotocol Shape (area (self) -> i64) (name (self) -> str \"shape\"))\n(defstruct Sq (s: i64))\n(impl Shape Sq (area (self) (* (. self s) (. self s))))\n"),
        one("extern-and-def", "(extern strtod :private (ptr ptr) -> f64)\n(def n: i64 (when true 1))\n"),
        one("for-each-and-range", "(defun f () -> i64 (do (for-each (range 0 3) (fn (i) (g i))) (for-each xs (fn (x) x)) (for-each (range 3) (fn (i: i64) i)) (range 2) (range 1 3)))\n"),
        one("loops-and-lets", "(defun f (o) -> i64 (do (while (g) (h)) (dotimes (i 3) (g i)) (plet ((a (g)) (b: i64 (h))) (+ a b)) (if-let (x o) x 0) (when-let ([a b] o) a) (doto (g) (h 1) (k))))\n"),
        one("bom", "\u{FEFF}(defun f () -> i64 (when true 1))\n"),
        one("empty-file", ""),
        one("only-comments", ";; nothing here\n; at all\n"),
        one("multibyte-positions", "(defun f () -> str (when \"é😀\" \"ü\"))\n"),
        one("crlf", "(defun f () -> i64\r\n  (when true\r\n    1))\r\n"),
    ]
}

/// Programs of several modules: the errors of loading and of macro
/// lookup, the driver's order.
fn modules() -> Vec<Input> {
    let util = "(ns util)\n(defmacro twice (x) `(do ,x ,x))\n(defmacro hid :private (x) x)\n(defstruct PS :private (a: i64))\n(defstruct S (a: i64))\n";
    let more = "(ns more)\n(defmacro twice (x) `(do ,x ,x ,x))\n";
    vec![
        program("ambiguous-macro", &[
            ("main.fib", "(ns main (:use util more))\n(defun main () -> i64 (twice 1))\n"),
            ("util.fib", util),
            ("more.fib", more),
        ]),
        program("qualified-macros", &[
            ("main.fib", "(ns main (:require [util :as u] [more :as m]))\n(defun main () -> i64 (+ (u/twice 1) (m/twice 1) (util/twice 1) (hid 1) (u/hid 1)))\n"),
            ("util.fib", util),
            ("more.fib", more),
        ]),
        program("hidden-types", &[
            ("main.fib", "(ns main (:use util))\n(derive Eq PS)\n"),
            ("util.fib", util),
        ]),
        program("visible-type-of-a-module", &[
            ("main.fib", "(ns main (:use util))\n(derive Eq S)\n(derive Show S)\n"),
            ("util.fib", util),
        ]),
        program("missing-module", &[("main.fib", "(ns main (:use nowhere))\n")]),
        program("cycle", &[
            ("main.fib", "(ns main (:use util))\n"),
            ("util.fib", "(ns util (:use main))\n"),
        ]),
        program("ns-mismatch", &[
            ("main.fib", "(ns main (:use util))\n"),
            ("util.fib", "(ns other)\n"),
        ]),
        program("bad-ns-clause", &[("main.fib", "(ns main (:bogus x))\n")]),
        program("dependency-does-not-read", &[
            ("main.fib", "(ns main (:use util))\n"),
            ("util.fib", "(ns util\n(defun f (\n"),
        ]),
        program("diamond", &[
            ("main.fib", "(ns main (:use left right))\n(defun main () -> i64 (twice 1))\n"),
            ("left.fib", "(ns left (:use shared))\n"),
            ("right.fib", "(ns right (:use shared))\n"),
            ("shared.fib", "(ns shared)\n(defmacro twice (x) `(do ,x ,x))\n"),
        ]),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use fibref::syntax::read_all;

    #[test]
    fn the_names_are_unique_and_the_fixed_inputs_are_many() {
        let all = edge_inputs();
        let names: std::collections::HashSet<&str> = all.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names.len(), all.len());
        assert!(all.len() >= 60, "{}", all.len());
        assert!(all
            .iter()
            .all(|i| i.kind == Kind::Edge && i.files[0].0 == "main.fib"));
    }

    #[test]
    fn all_but_the_ones_meant_not_to_read_read() {
        let unread = ["edge-dependency-does-not-read"];
        for input in edge_inputs() {
            for (name, text) in &input.files {
                let reads = read_all(text, name).is_ok();
                assert_eq!(
                    reads,
                    !unread.contains(&input.name.as_str()) || name == "main.fib",
                    "{} {name}",
                    input.name
                );
            }
        }
    }
}
