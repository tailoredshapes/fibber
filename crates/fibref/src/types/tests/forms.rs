//! The typing rules of the remaining forms (types §2): macros over
//! `Form`, `quote`, `dyn`, conversions, `unsafe` and `extern`, `async`
//! and `await`, derived instances, named `fn`s, the prelude's I/O.

use crate::types::ErrorKind as K;

use super::{binding_type, fails, ok};

#[test]
fn macros_are_typed_as_functions_over_form() {
    let p = ok("(defmacro m (x ... rest) `(do ~@rest (+ ~x 1))) (defun main () -> i64 0)");
    let m = p
        .globals
        .funs
        .iter()
        .position(|f| f.name == "m" && f.is_macro)
        .expect("macro");
    let s = p.fun_schemes[m].as_ref().expect("typed");
    assert_eq!(p.show_scheme(s), "(fn :send (Form (Vec Form)) Form)");
    fails(
        "(defmacro bad (x) 1) (defun main () -> i64 0)",
        K::Unify,
        "cannot unify i64 with Form",
    );
    // A macro is not a value.
    fails(
        "(defmacro m (x) x) (defun main () -> i64 (do m 0))",
        K::Resolve,
        "unbound name m",
    );
}

#[test]
fn quoted_forms_are_form_values() {
    let p = ok(
        "(defun items (f: Form) -> (Vec Form) (match f ((List xs) xs) (_ [])))
                (defun main () -> i64 (vec-count (items '(a b))))",
    );
    assert_eq!(
        p.show_fun("items").as_deref(),
        Some("(fn :send (Form) (Vec Form))")
    );
    ok("(defun main () -> i64 (match 'x ((Sym s) (str-len s)) (_ 0)))");
}

#[test]
fn dyn_values_dispatch_through_their_protocol() {
    let area = "(defprotocol Area (area (self) -> i64)) (defstruct Sq (n: i64))
                (impl Area Sq (area (self) (* (. self n) (. self n))))";
    let p = ok(&format!(
        "{area} (defun main () -> i64 (let ((d (dyn Area (Sq 3)))) (area d)))"
    ));
    assert_eq!(binding_type(&p, "d"), "(dyn Area)");
    fails(
        &format!("{area} (defun main () -> i64 (area (dyn Area \"s\")))"),
        K::NoInstance,
        "no implementation of Area for str",
    );
    // Object safety (§4.4).
    fails(
        "(defprotocol Same (same (self y: Self) -> bool)) (impl Same str (same (self y) (= self y)))
         (defun main () -> i64 (if (same (dyn Same \"a\") (dyn Same \"b\")) 1 0))",
        K::Other,
        "method same of Same is not callable through dyn",
    );
    // (dyn P) is not Send (§4.4).
    fails(
        &format!("{area} (defun main () -> i64 (let ((d (dyn Area (Sq 3)))) (join (spawn (fn () (area d))))))"),
        K::ValueNotSend,
        "value of type (dyn Area) cannot be shared between threads: closure capture d",
    );
}

#[test]
fn conversions_name_their_target_type() {
    let p = ok("(defun main () -> i64 (let ((a (trunc i8 300)) (b (sitofp f64 1)) (c (fptosi i64 1.5)) (d (fpext f64 1.0f32))) c))");
    assert_eq!(binding_type(&p, "a"), "i8");
    assert_eq!(binding_type(&p, "b"), "f64");
    fails(
        "(defun main () -> i64 (do (zext i64 1.5) 0))",
        K::NoInstance,
        "no implementation of Bits for f64",
    );
    fails(
        "(defun main () -> i64 (do (fptrunc f32 1) 0))",
        K::Unify,
        "cannot unify i64 with a float type",
    );
    let p = ok("(defun widen (x) (zext i64 x)) (defun main () -> i64 (widen 1i8))");
    assert_eq!(
        p.show_fun("widen").as_deref(),
        Some("∀a. (Bits a) ⇒ (fn :send (a) i64)")
    );
}

#[test]
fn float_bit_casts_are_functions_of_fixed_widths() {
    let p = ok("(defun main () -> i64 (let ((a (f64->bits 1.0)) (b (bits->f64 1)) (c (f32->bits 1.0f32)) (d (bits->f32 1i32))) a))");
    assert_eq!(binding_type(&p, "a"), "i64");
    assert_eq!(binding_type(&p, "b"), "f64");
    assert_eq!(binding_type(&p, "c"), "i32");
    assert_eq!(binding_type(&p, "d"), "f32");
    // Numbers never widen: each cast takes exactly its own width (a literal argument
    // adopts a float type, stdlib §7 L19; a variable never does).
    fails(
        "(defun main () -> i64 (let ((n 1)) (do (f64->bits n) 0)))",
        K::Unify,
        "cannot unify i64 with f64",
    );
    fails(
        "(defun main () -> i64 (do (f32->bits 1.0) 0))",
        K::Unify,
        "cannot unify f64 with f32",
    );
    fails(
        "(defun main () -> i64 (do (bits->f64 1i32) 0))",
        K::Unify,
        "cannot unify i32 with i64",
    );
    fails(
        "(defun main () -> i64 (do (bits->f32 1) 0))",
        K::Unify,
        "cannot unify i64 with i32",
    );
}

#[test]
fn unsafe_operations_only_inside_unsafe() {
    ok("(extern puts (ptr) -> i32) (defun main () -> i64 (do (unsafe (puts (alloc 1))) 0))");
    fails(
        "(extern puts (ptr) -> i32) (defun main () -> i64 (do (puts (alloc 1)) 0))",
        K::Other,
        "puts may only be used inside unsafe",
    );
    fails(
        "(defun main () -> i64 (do (alloc 1) 0))",
        K::Other,
        "alloc may only be used inside unsafe",
    );
    fails(
        "(extern f (str) -> i32) (defun main () -> i64 0)",
        K::Other,
        "extern positions are scalars, ptr or unit",
    );
}

#[test]
fn async_and_await() {
    let p = ok("(defun measure (s) (async (await (yield)) (str-len s))) (defun main () -> i64 (block-on (measure \"x\")))");
    assert_eq!(
        p.show_fun("measure").as_deref(),
        Some("(fn :send (str) (Task i64))")
    );
    // §2.8: an async's captures must be Send.
    fails(
        "(defun main () -> i64 (let ((c (cell 1))) (block-on (async @c))))",
        K::CellNotSend,
        "cell cannot be shared between threads: async capture c has type (Cell i64)",
    );
    fails(
        "(defun main () -> i64 (await 1))",
        K::AwaitOutsideAsync,
        "await outside async",
    );
}

#[test]
fn derived_instances_resolve_through_their_contexts() {
    ok("(defstruct Pair (a b)) (derive Eq Pair) (derive Ord Pair) (derive Hash Pair) (derive Show Pair)
        (defun main () -> i64 (if (< (Pair 1 \"x\") (Pair 1 \"y\")) (hash (Pair 1 \"x\")) 0))");
    ok("(defenum Shape (Circle r: f64) (Rect w: f64 h: f64)) (derive Eq Shape) (derive Show Shape)
        (defun main () -> i64 (if (= (Circle 1.0) (Rect 1.0 2.0)) 0 (str-len (show (Circle 1.0)))))");
    ok("(defun main () -> i64 (if (= (list (some 1)) (list (some 1))) 1 0))");
    fails(
        "(defstruct Pair (a b)) (derive Eq Pair) (defun main () -> i64 (if (= (Pair (cell 1) 2) (Pair (cell 1) 2)) 1 0))",
        K::NoInstance,
        "no implementation of Eq for (Cell i64)",
    );
}

#[test]
fn named_fn_calls_itself_monomorphically() {
    let p = ok("(defun main () -> i64 (let ((f (fn go (n: i64) -> i64 (if (= n 0) 0 (go (- n 1)))))) (f 5)))");
    assert_eq!(binding_type(&p, "go"), "(fn :send (i64) i64)");
}

#[test]
fn the_prelude_io_and_dbg() {
    let p = ok("(defun main () -> i64 (do (eprintln \"hi\") (dbg (+ 1 2))))");
    assert_eq!(
        p.show_fun("eprintln").as_deref(),
        Some("(fn :send (str) unit)")
    );
    fails(
        "(defun main () -> i64 (do (dbg (cell 1)) 0))",
        K::NoInstance,
        "no implementation of Show for (Cell i64)",
    );
}

#[test]
fn errors_are_reported_in_evaluation_order() {
    // The deferred Num constraint of the first step is retried before
    // the second step is typed (§3.5: the first form that cannot be
    // typed).
    let e = fails(
        "(defun main () -> i64 (do (+ \"a\" \"b\") (if 1 2 3)))",
        K::NoInstance,
        "no implementation of Num for str",
    );
    assert_eq!(e.pos.col, 28, "the position of the method +");
}

#[test]
fn deref_of_a_task_is_its_result() {
    // types §2.9: the fourth built-in `Deref` instance, `(Deref (Task a) a)`,
    // whose `deref` is `join`.
    let p = ok("(defun get (t: (Task str)) -> str @t)
                (defun fun (t: (Task i64)) -> i64 (deref t))
                (defun main () -> i64 (str-len (get (spawn (fn () \"ab\")))))");
    assert_eq!(
        p.show_fun("get").as_deref(),
        Some("(fn :send ((Task str)) str)")
    );
    assert_eq!(
        p.show_fun("fun").as_deref(),
        Some("(fn :send ((Task i64)) i64)")
    );
    // A task of unresolved type is fixed by the `join` that comes first
    // (§3.4); a bare `@t` is the deferred constraint, unresolved at the end.
    ok("(defun get (t) (do (join t) @t)) (defun main () -> i64 (get (spawn (fn () 1))))");
    fails(
        "(defun get (t) @t) (defun main () -> i64 0)",
        K::DerefUnresolved,
        "cannot infer whether t is a cell, an atom, a weak reference or a task",
    );
    // The result type is the task's, so a mismatch is the usual one.
    fails(
        "(defun main () -> str @(spawn (fn () 1)))",
        K::Unify,
        "cannot unify i64 with str",
    );
}

#[test]
fn a_user_deref_of_a_task_overlaps_the_builtin_instance() {
    // Before the instance existed the program below was accepted and `@`
    // ignored it; now it is `overlapping instances` as for `Cell` (§2.9).
    fails(
        "(impl (Deref a) (Task a) (deref (self) (trap \"mine\")))
         (defun main () -> i64 @(spawn (fn () 1)))",
        K::Other,
        "overlapping instances: Deref for (Task a) is already implemented",
    );
}
