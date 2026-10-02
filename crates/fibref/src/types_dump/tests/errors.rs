//! The error records of the dump, the determinism check and the
//! renumbering through the dump.

use std::collections::BTreeSet;
use std::sync::Arc;

use super::{dump, only, plain, with, EX};
use crate::syntax::Pos;
use crate::types::TypeError;
use crate::types_dump::{error_dump, push, push_raw, Options, Section, Stage};

/// Programs the checker rejects, one for each error kind a short program
/// reaches: the name, the program, and the record it prints.
const REJECTED: [(&str, &str, &str); 26] = [
    ("Resolve", "(defun main () -> i64 nope)", "Resolve 1:23 22..26: unbound name nope"),
    ("Unify", "(defun main () -> i64 (+ 1i32 2))", "Unify 1:31 30..31: cannot unify i64 with i32"),
    (
        "Infinite",
        "(defun main () -> i64 (let ((c (cell []))) (do (set! c [c]) 0)))",
        "Infinite 1:56 55..58: cannot construct the infinite type: (Vec (Cell (Vec a))) = (Vec a)",
    ),
    (
        "NoField",
        "(defstruct P (x: i64 y: i64)) (defun main () -> i64 (. (P 1 2) z))",
        "NoField 1:53 52..65: P has no field z",
    ),
    (
        "FieldUnresolved",
        "(defun getx (p) (. p x)) (defun main () -> i64 0)",
        "FieldUnresolved 1:17 16..23: cannot infer the struct type of p for field x; annotate it",
    ),
    (
        "DerefUnresolved",
        "(defun get (c) @c) (defun main () -> i64 0)",
        "DerefUnresolved 1:16 15..17: cannot infer whether c is a cell, an atom, a weak reference or a task",
    ),
    (
        "NoInstance",
        "(defun main () -> i64 @5)",
        "NoInstance 1:23 22..24: no implementation of Deref for i64",
    ),
    (
        "Ambiguous",
        "(defun f (x) (do (show (trap \"no\")) x)) (defun main () -> i64 (f 1))",
        "Ambiguous 1:19 18..22: ambiguous constraint Show a in f; add an annotation",
    ),
    (
        "AmpArgument",
        "(defun bump (&v) (set! v (+ @v 1))) (defun main () -> i64 (let ((x 5)) (do (bump &x) x)))",
        "AmpArgument 1:82 81..83: & argument must be a cell variable",
    ),
    (
        "AmpParamValue",
        "(defun leak (&v) v) (defun main () -> i64 0)",
        "AmpParamValue 1:18 17..18: & parameter v used as a value in leak",
    ),
    (
        "AmpPosition",
        "(defun bump (&v) (set! v (+ @v 1))) (defun main () -> i64 (let ((c (cell 0))) (do (bump c) @c)))",
        "AmpPosition 1:89 88..89: parameter v of bump is &; pass &x",
    ),
    (
        "AmpFunctionValue",
        "(defun bump (&v) (set! v (+ @v 1))) (defun main () -> i64 (let ((f bump)) 0))",
        "AmpFunctionValue 1:68 67..71: function with & parameters is not a value",
    ),
    (
        "NonExhaustive",
        "(defun main () -> i64 (match (some 1) ((some x) x)))",
        "NonExhaustive 1:23 22..51: non-exhaustive match: missing nil",
    ),
    (
        "Redundant",
        "(defun main () -> i64 (match true (true 1) (false 2) (_ 3)))",
        "Redundant 1:55 54..55: redundant match clause",
    ),
    (
        "AwaitOutsideAsync",
        "(defun main () -> i64 (await (async 1)))",
        "AwaitOutsideAsync 1:23 22..39: await outside async",
    ),
    (
        "WeakScalar",
        "(defun main () -> i64 (do (weak 1) 0))",
        "WeakScalar 1:28 27..31: weak requires an object type",
    ),
    (
        "WeakOption",
        "(defun main () -> i64 (do (weak nil) 0))",
        "WeakOption 1:28 27..31: weak of an Option is not allowed: (Option a) has no object of its own to observe; take the weak reference of the object inside it",
    ),
    (
        "NotObject",
        "(defprotocol Q (q (self) -> i64)) (impl Q i64 (q (self) 3)) (defun main () -> i64 (q (dyn Q 7)))",
        "NotObject 1:86 85..94: dyn requires an object type, not i64",
    ),
    (
        "ConstantCalled",
        "(defenum C Red Green) (defun main () -> i64 (Red))",
        "ConstantCalled 1:46 45..48: Red is a constant, not a function; write Red",
    ),
    (
        "RecurOutsideLoop",
        "(defun main () -> i64 (recur 1))",
        "RecurOutsideLoop 1:23 22..31: recur outside loop",
    ),
    (
        "RecurNotTail",
        "(defun main () -> i64 (loop ((i 0)) (+ 1 (recur i))))",
        "RecurNotTail 1:42 41..50: recur not in tail position",
    ),
    (
        "DefUnresolved",
        "(def g []) (defun main () -> i64 0)",
        "DefUnresolved 1:1 0..10: def g has an unresolved type; annotate it",
    ),
    (
        "DefNotConstant",
        "(defun f () -> i64 1) (def g (f)) (defun main () -> i64 g)",
        "DefNotConstant 1:30 29..32: def g: initialiser is not a constant expression",
    ),
    (
        "CellNotSend",
        "(defun main () -> i64 (let ((c (cell 1))) (do (spawn (fn () @c)) 0)))",
        "CellNotSend 1:54 53..63: cell cannot be shared between threads: closure capture c has type (Cell i64)",
    ),
    (
        "Other",
        "(defun f (x) x) (defun main () -> i64 (f 1 2))",
        "Other 1:39 38..45: f takes 1 argument(s), got 2",
    ),
    (
        "Other",
        "(defun f () -> i64 1)",
        "Other 0:0 0..0@<builtin>: the program has no main",
    ),
];

#[test]
fn each_error_kind_a_short_program_reaches_prints_its_record_and_ends_the_file() {
    let mut kinds = BTreeSet::new();
    for (kind, src, record) in REJECTED {
        let (text, status) = dump("rejected", src, &plain());
        assert_eq!(text, format!("== D/t.fib\nerror {record}\n"), "{src}");
        assert_eq!(status, 1, "{src}");
        kinds.insert(kind);
    }
    assert!(kinds.len() >= 15, "{kinds:?}");
    assert_eq!(kinds.len(), 25);
}

#[test]
fn a_failed_step_prints_every_error_of_its_units_in_order_and_the_sections_are_left_out() {
    let src = "(defun f () -> i64 \"a\") (defun g () -> bool 1) (defun main () -> i64 0)";
    let (text, status) = dump("many", src, &plain());
    assert_eq!(status, 1);
    let expected = "== D/t.fib
error Unify 1:20 19..22: cannot unify str with i64
error Unify 1:45 44..45: cannot unify i64 with bool
";
    assert_eq!(text, expected);
    // A section list without `error` hides the records, not the failure.
    let (text, status) = dump("many", src, &only(&[Section::Type]));
    assert_eq!((text.as_str(), status), ("== D/t.fib\n", 1));
}

#[test]
fn lowering_errors_stop_before_inference_at_either_stage() {
    let src = "(defun f () -> i64 1) (defun f () -> i64 2) (defun main () -> i64 \"x\")";
    let want = "== D/t.fib\nerror Resolve 1:23 22..43: f is already defined\n";
    let (infer, status) = dump("lowering", src, &plain());
    assert_eq!((infer.as_str(), status), (want, 1));
    let (lower, status) = dump("lowering", src, &with(|o| o.stage = Stage::Lower));
    assert_eq!((lower.as_str(), status), (want, 1));
    // A type error is not found at the lower stage.
    let (lower, status) = dump(
        "lowering",
        "(defun main () -> i64 \"x\")",
        &with(|o| o.stage = Stage::Lower),
    );
    assert_eq!(status, 0, "{lower}");
}

#[test]
fn error_records_and_scheme_texts_are_renumbered_by_first_occurrence() {
    let pos = Pos {
        file: Arc::from("f.fib"),
        line: 1,
        col: 2,
        start: 1,
        end: 5,
    };
    let e = TypeError::other(&pos, "colour ?ς9 is not ?ς4, nor ?ς9 or ?3 (?3 and ?8)");
    let dump = error_dump(&[e.clone(), e], "f.fib", &Options::default());
    let line = "error Other 1:2 1..5: colour ?ς1 is not ?ς2, nor ?ς1 or ?1 (?1 and ?2)\n";
    assert_eq!(
        dump.text,
        format!("{line}{line}"),
        "each record counts from one"
    );
    assert!(dump.failed);
    let mut out = String::new();
    push(&mut out, "(fn ?ς40 (a) a) ⊑ ?ς41");
    push_raw(&mut out, "local B3 ?ς40");
    assert_eq!(out, "(fn ?ς1 (a) a) ⊑ ?ς2\nlocal B3 ?ς40\n");
}

#[test]
fn the_dump_is_the_same_text_every_time_it_never_iterates_a_hash_map() {
    // Two runs in one process build their hash tables with different
    // seeds, so a dump that depended on iteration order would differ.
    let everything = with(|o| {
        o.ast = true;
        o.tables = true;
    });
    let lowered = with(|o| {
        o.stage = Stage::Lower;
        o.ast = true;
    });
    let mut programs: Vec<&str> = vec![EX];
    programs.extend(REJECTED.iter().map(|(_, src, _)| *src));
    for src in programs {
        for opts in [&plain(), &everything, &lowered] {
            let first = dump("twice", src, opts);
            assert_eq!(first, dump("twice", src, opts), "{src}");
        }
    }
    // The library and the prelude: every module of the implicit set.
    let all = Options {
        implicit: true,
        ast: true,
        tables: true,
        ..Options::default()
    };
    let first = dump("twice", EX, &all);
    assert_eq!(first.1, 0);
    assert!(first.0.len() > 100_000, "the library is in the dump");
    assert_eq!(first, dump("twice", EX, &all));
}
