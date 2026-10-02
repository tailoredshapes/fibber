//! Real macro-time modules run from fibber (spec/compiler.md §6, §9).
//!
//! For each scenario a program with a `defmacro` is expanded by the Rust
//! `JitRunner`, which makes the macro-time module (`fibc`'s own, the
//! text `fibc emit --macros` would print) and runs the macro: that is
//! the oracle. The module's text goes to a file and `jit-demo macro`,
//! a fibber program, runs the same macro on the same call: it reads the
//! call with the stage-2 reader, builds `Form` objects with the module's
//! own constructors through `call-i64`, runs the entry on a mailbox's
//! worker, answers `gensym` and the reflection calls itself, reads the
//! result back and prints it with the stage-2 printer. The two texts
//! must be equal, and so must the gensym counter and the error of an
//! expansion that fails.

use std::path::Path;

use fibc::macros::abi::WIDTHS;
use fibc::macros::module::Fns;
use fibref::cases::list_cases;
use lair::{Jit, JitOptions};

use super::recording::{
    compare, demo_output, demo_run, expand, expected_output, expected_repeated, Scenario,
};
use super::support::stdout_text;

const GENSYM: Scenario = Scenario {
    name: "gensym",
    source: "(defmacro with-temp (e body)
  (let ((g (gensym \"tmp\")))
    `(let ((,g ,e)) ,body)))
(defun main () -> i64 (do (with-temp 5 (+ 1 2)) (with-temp \"s\" (with-temp 1 2))))",
    wanted: "with-temp",
    calls: 3,
};

#[test]
fn a_macro_that_calls_gensym_is_answered_from_fibber() {
    compare(&GENSYM);
}

const REFLECTION: Scenario = Scenario {
    name: "reflection",
    source: "(defstruct P (a: i64 b: str))
(defmacro describe (name)
  (let ((g (gensym \"d\")))
    (if (struct? name)
        `(let ((,g ,(Int (vec-count (struct-fields name)) :i64))) (do ,@(struct-fields name) ,g))
        `(quote ,name))))
(defun main () -> i64 (do (describe P) (describe Q) 0))",
    wanted: "describe",
    calls: 2,
};

#[test]
fn a_macro_that_reflects_is_answered_from_fibber() {
    compare(&REFLECTION);
}

#[test]
fn a_reflection_error_ends_the_expansion_with_the_message_rust_gives() {
    for (name, call) in [
        ("not-a-struct", "(fields-of Q)"),
        ("not-a-symbol", "(fields-of 5)"),
    ] {
        let source = format!(
            "(defmacro fields-of (name) `(quote ,(Vec (struct-fields name))))
(defun main () -> i64 (do {call} 0))"
        );
        let source: &'static str = Box::leak(source.into_boxed_str());
        compare(&Scenario {
            name,
            source,
            wanted: "fields-of",
            calls: 1,
        });
    }
}

const REST: Scenario = Scenario {
    name: "rest",
    source: "(defmacro wrap-all (tag ... bodies)
  (let ((g (gensym \"w\")))
    `(let ((,g ,tag)) (do ,@bodies ,g))))
(defun f () -> i64 1)
(defun main () -> i64 (do (wrap-all 7 (f) (f) (f)) (wrap-all 8)))",
    wanted: "wrap-all",
    calls: 2,
};

#[test]
fn a_macro_with_a_rest_parameter_gets_its_arguments_as_a_vec() {
    compare(&REST);
}

/// Every kind of form passes through a macro that returns its argument:
/// the objects built from fibber are read back by the module's own
/// accessors, and the text is the same.
const EVERY_KIND: Scenario = Scenario {
    name: "every-kind",
    source: "(defmacro ident (x) x)
(defun main () -> i64
  (do (ident [1 2i32 -3i8 30000i16 -9223372036854775808 \"s\\n\\\"q\\\" é\" \\a \\newline \\u{1F600}
              true false nil :kw {1 2} (a (b c)) sym [] () {} + a/b])
      (ident ())
      0))",
    wanted: "ident",
    calls: 2,
};

#[test]
fn every_kind_of_form_goes_in_and_comes_out_of_a_module() {
    compare(&EVERY_KIND);
}

/// Floats: a float is made into its bits with `f64->bits` (every bit of
/// the f64, whatever the form's width) and read out with `call-f64`.
const FLOATS: Scenario = Scenario {
    name: "floats",
    source: "(defmacro ident (x) x)
(defun main () -> i64
  (do (ident [2.5 -0.0 0.0 1e300 5e-324 1.7976931348623157e308 2.5f32 0.1f32 0.1 -3.4028235e38f32
              16777217.0 123456789.125 1e-7 1e16 1e15])
      0))",
    wanted: "ident",
    calls: 1,
};

#[test]
fn floats_go_in_and_come_out_of_a_module_bit_for_bit() {
    compare(&FLOATS);
}

/// A macro that builds each kind of form with the constructors.
const CONSTRUCT: Scenario = Scenario {
    name: "construct",
    source: "(defmacro build ()
  (List [(Sym \"foo\") (Kw \"bar\") (Str \"x\\ty\") (Chr \\a) (Bool true) Nil (Int 7 :i32)
         (Vec [(Int 1 :i8) (Flt 2.5 :f32) (Int -9 :i64)]) (Map [(Sym \"k\") (Int 2 :i16)])
         (Flt 0.1 :f64) (List [])]))
(defun main () -> i64 (do (build) 0))",
    wanted: "build",
    calls: 1,
};

#[test]
fn a_macro_that_builds_forms_returns_them_to_fibber() {
    compare(&CONSTRUCT);
}

/// A macro that builds a form whose width is `:f16`, a keyword the module
/// interns after the six widths: case 105 of `cases/ownership`, which the
/// expansion refuses where the form is turned back into code.
const BOGUS_WIDTH: Scenario = Scenario {
    name: "bogus-width",
    source: "(defmacro half () (List [(Sym \"fptosi\") (Sym \"i64\") (Flt 2.5 :f16)]))
(defun main () -> i64 (half))",
    wanted: "half",
    calls: 1,
};

/// The module's own text exports the table of its keywords, which the
/// compiler here and fibber read: `fibm.kw-count.K` and `fibm.kw.K`.
/// Every width and the `:f16` the macro mentions are in it once, by
/// name, and an id that is none has no name (null).
#[test]
fn the_module_exports_the_table_of_its_keywords() {
    type P = *const u8;
    let (runs, texts) = expand(BOGUS_WIDTH.source, Some(BOGUS_WIDTH.wanted));
    let k = runs[0].k.expect("the macro has a module");
    let mut jit = Jit::new(JitOptions::default()).expect("a JIT");
    jit.add_source("keywords", &texts[k].1)
        .expect("the module is added");
    let n = |s: &str| format!("fibm.{s}.{k}");
    // SAFETY: abi.rs defines these as (fn i64 ()), (fn ptr (i64)),
    // (fn i64 (ptr)) and (fn ptr (ptr)), and the JIT outlives the calls.
    let (count, kw, len, ptr): (
        extern "C" fn() -> i64,
        extern "C" fn(i64) -> P,
        extern "C" fn(P) -> i64,
        extern "C" fn(P) -> P,
    ) = unsafe {
        (
            jit.function(&n("kw-count")).expect("kw-count"),
            jit.function(&n("kw")).expect("kw"),
            jit.function(&n("str-len")).expect("str-len"),
            jit.function(&n("str-ptr")).expect("str-ptr"),
        )
    };
    let name = |id: i64| {
        let s = kw(id);
        // SAFETY: a str object holds its length in bytes at its bytes.
        (!s.is_null()).then(|| unsafe {
            let bytes = std::slice::from_raw_parts(ptr(s), len(s) as usize);
            String::from_utf8(bytes.to_vec()).expect("a keyword is utf-8")
        })
    };
    let table: Vec<String> = (0..count()).map(|id| name(id).expect("named")).collect();
    for want in WIDTHS.iter().copied().chain(["f16"]) {
        assert_eq!(
            table.iter().filter(|k| *k == want).count(),
            1,
            "{want} in {table:?}"
        );
    }
    assert_eq!(table.len(), WIDTHS.len() + 1, "the six widths and :f16");
    for id in [-1, count(), count() + 1, i64::MAX, i64::MIN] {
        assert_eq!(name(id), None, "keyword {id} of {}", count());
    }
    println!("the table of {}: {table:?}", runs[0].def.name);
}

/// The text of a module with the count its `fibm.kw-count.K` returns
/// replaced by `n`.
fn with_count(text: &str, k: usize, n: usize) -> String {
    let head = format!("(define (fibm.kw-count.{k} i64) () (block entry (ret (i64 ");
    let start = text.find(&head).expect("the module has a count") + head.len();
    let end = start + text[start..].find(')').expect("the count ends");
    format!("{}{n}{}", &text[..start], &text[end..])
}

/// A table that lacks a width (the module made by a compiler that did not
/// intern them), or has an id with no name, makes a module neither the
/// Rust runner nor fibber will use: each says so when it looks the module
/// up, and neither builds a form that would need the width.
#[test]
fn a_module_whose_table_lacks_a_width_or_a_name_is_refused_when_it_is_looked_up() {
    let (runs, texts) = expand(BOGUS_WIDTH.source, Some(BOGUS_WIDTH.wanted));
    let run = &runs[0];
    let k = run.k.expect("the macro has a module");
    let text = &texts[k].1;
    // The table is the six widths, in the order of WIDTHS, then :f16.
    let cases = [
        (4, "the keyword table has no :f32"),
        (100, "the keyword table has no name for keyword 7 of 100"),
    ];
    for (count, message) in cases {
        let broken = with_count(text, k, count);
        assert_ne!(&broken, text);
        let mut jit = Jit::new(JitOptions::default()).expect("a JIT");
        jit.add_source("broken", &broken).expect("it is added");
        match Fns::lookup(&mut jit, k) {
            Err(u) => assert_eq!(u.0, format!("macro module: {message}")),
            Ok(_) => panic!("the Rust runner accepted a table of {count}"),
        }
        let (_, out) = demo_run("broken-table", run, &broken, 1);
        assert_eq!(
            (out.status.code(), stdout_text(&out)),
            (Some(1), format!("-- the macro module\n{message}\n")),
            "fibber with a table of {count}"
        );
    }
}

/// A width the module interned past the six (`:f16`), one that no table
/// has (`:i128`) and a width of the other family are named by the
/// error from the module's table, as the Rust runner names them.
#[test]
fn a_form_of_a_width_that_is_none_is_named_from_the_table_as_rust_names_it() {
    for (name, body) in [
        ("flt-f16", "(Flt 2.5 :f16)"),
        ("int-i128", "(Int 7 :i128)"),
        ("int-f32", "(Int 7 :f32)"),
        ("flt-i8", "(Flt 1.5 :i8)"),
    ] {
        let source = format!(
            "(defmacro bad () (List [(Sym \"quote\") {body}]))\n(defun main () -> i64 (do (bad) 0))"
        );
        let source: &'static str = Box::leak(source.into_boxed_str());
        let scenario = Scenario {
            name,
            source,
            wanted: "bad",
            calls: 1,
        };
        compare(&scenario);
        let (runs, _) = expand(source, Some("bad"));
        let want = expected_output(&runs[0]);
        let kind = if body.starts_with("(Int") {
            "an Int"
        } else {
            "a Flt"
        };
        assert!(
            want.contains(&format!("trap: {kind} form of width :")) && !want.contains("#"),
            "{name}: {want}"
        );
    }
}

/// The calls of every `defmacro` of `cases/ownership` are run from fibber
/// and compared with the Rust runner's, as the real programs of the
/// project are, case 105 included: its `:f16` is named by the module's
/// table in both. What is left out: a macro whose module cannot be made
/// (a case that is refused before the macro runs), and one that fails
/// by `trap`.
#[test]
fn the_macros_of_the_cases_run_from_fibber_as_the_rust_runner_runs_them() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../cases/ownership");
    let (mut compared, mut skipped) = (Vec::new(), Vec::new());
    for path in list_cases(&dir).expect("cases/ownership is readable") {
        let source = std::fs::read_to_string(&path).expect("a case is readable");
        if !source.contains("defmacro") {
            continue;
        }
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let (runs, texts) = expand(&source, None);
        for run in &runs {
            let Some(k) = run.k else {
                skipped.push(format!("{name}: {} has no module", run.def.name));
                continue;
            };
            let want = expected_output(run);
            // A macro that fails by `trap` (cases 246, 247): the Rust
            // runner rejects the program through its hook
            // (`fibm.set-trap-hook`); the fibber runner sets none yet, so
            // its module still aborts the process, as the Rust runner's
            // did before the hook.
            if want.contains(": trap: ") {
                skipped.push(format!("{name}: {} fails by trap", run.def.name));
                continue;
            }
            let got = demo_output(name, run, &texts[k].1, 1);
            assert_eq!(got, want, "{name}: {}", run.call);
            compared.push(format!("{name}: {}", run.call));
        }
    }
    println!("compared {} runs; skipped {skipped:?}", compared.len());
    assert!(compared.len() >= 11, "only {compared:?}");
    assert!(
        compared.iter().any(|c| c.starts_with("105-")),
        "case 105 is among them: {compared:?}"
    );
}

/// One module, initialised once, runs the same call again and again, a
/// new mailbox each time, and the gensym counter is passed on from each
/// run to the next: the second run's `#tmp.N` is the first's `N + 1`.
#[test]
fn a_module_runs_the_same_macro_again_with_the_counter_passed_on() {
    for scenario in [&GENSYM, &REFLECTION, &REST] {
        let (runs, texts) = expand(scenario.source, Some(scenario.wanted));
        let run = &runs[0];
        let k = run.k.expect("the macro has a module");
        let got = demo_output(scenario.name, run, &texts[k].1, 3);
        assert_eq!(
            got,
            expected_repeated(run, 3),
            "{}: {}",
            scenario.name,
            run.call
        );
        assert!(got.matches("expansion: ").count() == 3, "{got}");
    }
}

/// A macro that runs a task: the module's runtime must be initialised
/// (`fibm.init`, which sets up its run queue) before the macro runs.
const TASKS: Scenario = Scenario {
    name: "tasks",
    source: "(defmacro threaded (x)
  (let ((t (spawn (fn () 40))))
    (if (= (join t) 40) x `(quote bad))))
(defun main () -> i64 (do (threaded 5) 0))",
    wanted: "threaded",
    calls: 1,
};

#[test]
fn a_macro_that_spawns_a_task_needs_the_module_initialised() {
    compare(&TASKS);
}
