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

use fibc::compile::compile_macro;
use fibc::macros::macro_forms;
use fibref::cases::list_cases;
use fibref::expand::ExpandCtx;
use fibref::own::check_forms;
use fibref::types::prelude_forms;

use super::recording::{
    compare, demo_output, expand, expected_output, expected_repeated, Scenario,
};

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
        `(let ((,g ,(Int (count (struct-fields name)) :i64))) (do ,@(struct-fields name) ,g))
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

/// Floats: fibber has no bit cast from `f64`, so a float is made into its
/// bits through the C library (`sscanf` of the shortest text that reads
/// back) and read out with `call-f64`.
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

/// fibber's `to-object` assumes the keyword ids of the widths: i8 i16
/// i32 i64 f32 f64 are 0 to 5 in every module (`compile_macro` interns
/// them first), and the text the runner made is the text `compile_macro`
/// makes.
#[test]
fn the_width_ids_fibber_assumes_are_the_ones_the_module_has() {
    let (runs, texts) = expand(REFLECTION.source, Some(REFLECTION.wanted));
    let run = &runs[0];
    let mut ctx = ExpandCtx::new();
    let prelude = prelude_forms(&mut ctx).expect("the prelude expands");
    let checked = check_forms(&macro_forms(&run.def), &prelude).expect("the macro checks");
    let n = run.def.params.len() + usize::from(run.def.rest.is_some());
    let k = run.k.expect("the macro has a module");
    let module = compile_macro(&checked, &run.def.name, n, k).expect("the macro compiles");
    assert_eq!(module.text, texts[k].1);
    assert_eq!(module.widths, [0, 1, 2, 3, 4, 5]);
}

/// The calls of every `defmacro` of `cases/ownership` are run from fibber
/// and compared with the Rust runner's, as the real programs of the
/// project are. What is left out: a macro whose module cannot be made (a
/// case that is refused before the macro runs).
///
/// Case 105 is the one known difference: the module's keyword table is
/// not in its text, so fibber names the width `:f16` by its id, `#6`,
/// where the Rust runner names the keyword (spec/bootstrap.md §4).
const KEYWORD_GAP_CASE: &str = "105-reject-macro-float-literal-bogus-width.fib";

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
            let got = demo_output(name, run, &texts[k].1, 1);
            let mut want = expected_output(run);
            if name == KEYWORD_GAP_CASE {
                want = want.replace(":f16", ":#6");
            }
            assert_eq!(got, want, "{name}: {}", run.call);
            compared.push(format!("{name}: {}", run.call));
        }
    }
    println!("compared {} runs; skipped {skipped:?}", compared.len());
    assert!(compared.len() >= 11, "only {compared:?}");
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
