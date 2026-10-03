//! Library roots (spec/syntax.md §5, spec/compiler.md §1; stdlib design
//! §7 E7), **Proposed**, through the command line of `fibref`: a module
//! is found beside the main file, then under each `-I DIR` in order, then
//! under each directory of `FIB_LIB` in order, then in the library the
//! executable carries; the first place that has it wins and the others are
//! not read; `(:export-from m)` makes a facade of a module.

#![cfg(unix)]

mod io_support;

use std::path::Path;
use std::process::{Command, Output};

use io_support::TempDir;

const FIBREF: &str = env!("CARGO_BIN_EXE_fibref");

/// A module `ns` of one function `which`, answering `n`.
fn module(ns: &str, n: i64) -> String {
    format!("(ns {ns})\n(defun which () -> i64 {n})\n")
}

/// Writes `files` (path, text) under `dir`, making their directories.
fn tree(dir: &Path, files: &[(&str, &str)]) {
    for (path, text) in files {
        let file = dir.join(path);
        std::fs::create_dir_all(file.parent().expect("a parent")).expect("dirs");
        std::fs::write(file, text).expect("write");
    }
}

fn run(args: &[&Path], fib_lib: Option<&str>) -> Output {
    let mut command = Command::new(FIBREF);
    command.arg("run").args(args).env_remove("FIB_LIB");
    if let Some(list) = fib_lib {
        command.env("FIB_LIB", list);
    }
    command.output().expect("fibref runs")
}

fn said(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// The result `fibref run` printed, or the whole output.
fn result(out: &Output) -> String {
    let text = said(out);
    match text.lines().find_map(|l| l.strip_prefix("result: ")) {
        Some(n) => n.to_string(),
        None => text,
    }
}

const MAIN: &str = "(ns main (:require [pick :as p]))\n(defun main () -> i64 (p/which))\n";

#[test]
fn a_module_under_an_i_root_is_found_and_without_the_flag_it_is_not() {
    let d = TempDir::new("roots-i");
    tree(d.path(), &[("lib/pick.fib", &module("pick", 7))]);
    let main = d.file("main.fib", MAIN);
    let without = run(&[&main], None);
    assert!(
        said(&without).contains("module pick is not at"),
        "{}",
        said(&without)
    );
    assert_eq!(without.status.code(), Some(1));
    let lib = d.path().join("lib");
    for words in [
        vec!["-I".as_ref(), lib.as_path(), &main],
        vec![&main, "-I".as_ref(), &lib],
    ] {
        assert_eq!(result(&run(&words, None)), "7");
    }
}

#[test]
fn the_order_is_the_main_directory_then_each_i_then_each_fib_lib_directory() {
    let d = TempDir::new("roots-order");
    tree(
        d.path(),
        &[
            ("i1/pick.fib", &module("pick", 1)),
            ("i2/pick.fib", &module("pick", 2)),
            ("v1/pick.fib", &module("pick", 3)),
            ("v2/pick.fib", &module("pick", 4)),
            ("v2/only.fib", &module("only", 5)),
        ],
    );
    let main = d.file("main.fib", MAIN);
    let (i1, i2) = (d.path().join("i1"), d.path().join("i2"));
    let fib_lib = format!(
        "{}:{}",
        d.path().join("v1").display(),
        d.path().join("v2").display()
    );
    // Every -I before every FIB_LIB directory, in the order given.
    assert_eq!(
        result(&run(
            &["-I".as_ref(), &i1, "-I".as_ref(), &i2, &main],
            Some(&fib_lib)
        )),
        "1"
    );
    assert_eq!(
        result(&run(
            &["-I".as_ref(), &i2, "-I".as_ref(), &i1, &main],
            Some(&fib_lib)
        )),
        "2"
    );
    assert_eq!(result(&run(&[&main], Some(&fib_lib))), "3");
    // A module in the main file's directory comes before all of them.
    d.file("pick.fib", &module("pick", 9));
    assert_eq!(
        result(&run(&["-I".as_ref(), &i1, &main], Some(&fib_lib))),
        "9"
    );
    // A directory of the list that is not there, or empty, is skipped.
    let list = format!(
        "{}::{}",
        d.path().join("none").display(),
        d.path().join("v2").display()
    );
    let only = d.file(
        "only-main.fib",
        "(ns main (:require [only :as o]))\n(defun main () -> i64 (o/which))\n",
    );
    assert_eq!(result(&run(&[&only], Some(&list))), "5");
}

#[test]
fn a_module_found_nowhere_names_the_file_it_was_expected_at_and_the_other_places() {
    let d = TempDir::new("roots-missing");
    let main = d.file("main.fib", MAIN);
    let other = d.path().join("other");
    let out = run(&["-I".as_ref(), &other, &main], None);
    let text = said(&out);
    assert!(
        text.contains(&format!(
            "module pick is not at {}/pick.fib",
            d.path().display()
        )),
        "{text}"
    );
    assert!(
        text.contains(&format!("nor at {}/pick.fib", other.display())),
        "{text}"
    );
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn explain_takes_the_roots_too() {
    let d = TempDir::new("roots-explain");
    tree(d.path(), &[("lib/pick.fib", &module("pick", 7))]);
    let main = d.file("main.fib", MAIN);
    let lib = d.path().join("lib");
    let out = Command::new(FIBREF)
        .arg("explain")
        .arg("-I")
        .arg(&lib)
        .arg(&main)
        .env_remove("FIB_LIB")
        .output()
        .expect("fibref runs");
    assert!(out.status.success(), "{}", said(&out));
    assert!(said(&out).contains("(which)"), "{}", said(&out));
}

#[test]
fn a_command_that_runs_no_program_refuses_the_flag_instead_of_ignoring_it() {
    for words in [["cases", "-I", "x"], ["read", "-I", "x"]] {
        let out = Command::new(FIBREF)
            .args(words)
            .output()
            .expect("fibref runs");
        assert_eq!(out.status.code(), Some(2), "{words:?}: {}", said(&out));
        assert!(
            said(&out).contains("-I belongs to `run`, `explain`, `expand`, `types` and `own`"),
            "{}",
            said(&out)
        );
    }
}

#[test]
fn the_flag_after_the_dashes_belongs_to_the_program() {
    let d = TempDir::new("roots-dashes");
    let main = d.file("main.fib", "(defun main () -> i64 (count (args)))\n");
    let out = Command::new(FIBREF)
        .arg("run")
        .arg(&main)
        .args(["--", "-I", "x"])
        .output()
        .expect("fibref runs");
    assert_eq!(result(&out), "2", "{}", said(&out));
}

/// A facade module that re-exports two others, one of which defines a
/// function, a struct, a protocol and a macro.
fn facade_tree(d: &Path) {
    tree(
        d,
        &[
            (
                "lib/parts/a.fib",
                "(ns parts.a)\n(defstruct Pt (x: i64 y: i64))\n\
                 (defprotocol Sized (size (self) -> i64))\n\
                 (impl Sized Pt (size (self) (+ (. self x) (. self y))))\n\
                 (defun fa (n: i64) -> i64 (+ n 100))\n\
                 (defun hidden :private () -> i64 5)\n\
                 (defmacro twice (x) `(+ ~x ~x))\n",
            ),
            (
                "lib/parts/b.fib",
                "(ns parts.b)\n(defun fb (n: i64) -> i64 (+ n 200))\n",
            ),
            (
                "lib/facade.fib",
                "(ns facade (:export-from parts.a parts.b))\n",
            ),
        ],
    );
}

fn run_main(d: &TempDir, text: &str) -> String {
    let main = d.file("main.fib", text);
    let lib = d.path().join("lib");
    result(&run(&["-I".as_ref(), &lib, &main], None))
}

#[test]
fn a_facade_exports_what_its_modules_do_and_not_what_they_keep_private() {
    let d = TempDir::new("roots-facade");
    facade_tree(d.path());
    let all = "(ns main (:use facade))\n\
               (defun main () -> i64 (+ (fa 1) (+ (fb 2) (+ (twice 3) (size (Pt 1 2))))))\n";
    assert_eq!(run_main(&d, all), "312");
    let private = "(ns main (:use facade))\n(defun main () -> i64 (hidden))\n";
    assert!(run_main(&d, private).contains("unbound name hidden"));
    let qualified = "(ns main (:require [facade :as f]))\n\
                     (defun main () -> i64 (+ (f/fa 1) (f/twice 2)))\n";
    assert_eq!(run_main(&d, qualified), "105");
}

#[test]
fn two_modules_a_facade_re_exports_that_disagree_on_a_name_make_it_an_error() {
    let d = TempDir::new("roots-facade-clash");
    tree(
        d.path(),
        &[
            ("lib/x.fib", "(ns x)\n(defun peek () -> i64 1)\n"),
            ("lib/y.fib", "(ns y)\n(defun peek () -> i64 2)\n"),
            ("lib/f.fib", "(ns f (:export-from x y))\n"),
            ("lib/g.fib", "(ns g (:export-from x))\n"),
        ],
    );
    let said = run_main(&d, "(ns main (:use f))\n(defun main () -> i64 (peek))\n");
    assert!(
        said.contains("peek is exported by both x and y; write x/peek or y/peek"),
        "{said}"
    );
    // The same definition through two facades is one name, not two.
    let twice = run_main(&d, "(ns main (:use g x))\n(defun main () -> i64 (peek))\n");
    assert_eq!(twice, "1");
}

#[test]
fn a_facade_that_re_exports_itself_is_a_cycle() {
    let d = TempDir::new("roots-facade-cycle");
    tree(
        d.path(),
        &[
            ("lib/p.fib", "(ns p (:export-from q))\n"),
            ("lib/q.fib", "(ns q (:export-from p))\n"),
        ],
    );
    let said = run_main(&d, "(ns main (:use p))\n(defun main () -> i64 1)\n");
    assert!(
        said.contains("modules require each other in a cycle: main -> p -> q -> p"),
        "{said}"
    );
}

/// An evaluator that does not take roots fails a case whose header names
/// them, instead of running it without and blaming the program.
#[test]
fn an_evaluator_that_ignores_roots_fails_a_case_that_has_them() {
    use fibref::cases::{run_case, PendingEvaluator, Status};
    let d = TempDir::new("roots-evaluator");
    let header = ";; spec: syntax §5\n;; expect: accept\n;; result: 1\n;; audit: clean\n";
    let with = d.file(
        "with.fib",
        &format!("{header};; roots: lib\n(defun main () -> i64 1)\n"),
    );
    let without = d.file(
        "without.fib",
        &format!("{header}(defun main () -> i64 1)\n"),
    );
    match run_case(&with, &PendingEvaluator).status {
        Status::Fail(why) => assert!(why.contains("does not take the library roots"), "{why}"),
        other => panic!("{other:?}"),
    }
    assert!(matches!(
        run_case(&without, &PendingEvaluator).status,
        Status::Pending(_)
    ));
}
