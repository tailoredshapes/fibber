//! Library roots (spec/compiler.md §1, syntax §5), **Proposed**, through
//! `fibc`: `-I DIR` and `FIB_LIB` on `run` and `build`, the same order as
//! `fibref`'s (`crates/fibref/tests/roots.rs`), an executable that carries
//! the library modules it was built from, and the rule-6 harness giving
//! the child process the roots of the case's header and nothing else.

use std::path::Path;
use std::process::Command;

use super::io_support::TempDir;
use super::{bounded, build, fibc};

/// A module `ns` of one function `which`, answering `n`.
fn module(ns: &str, n: i64) -> String {
    format!("(ns {ns})\n(defun which () -> i64 {n})\n")
}

fn tree(dir: &Path, files: &[(&str, &str)]) {
    for (path, text) in files {
        let file = dir.join(path);
        std::fs::create_dir_all(file.parent().expect("a parent")).expect("dirs");
        std::fs::write(file, text).expect("write");
    }
}

const MAIN: &str = "(ns main (:require [pick :as p]))\n(defun main () -> i64 (p/which))\n";

/// What `fibc run` printed last, or its whole output.
fn run(args: &[&Path], fib_lib: Option<&str>) -> String {
    let mut command = fibc();
    command.arg("run").args(args).env_remove("FIB_LIB");
    if let Some(list) = fib_lib {
        command.env("FIB_LIB", list);
    }
    let out = bounded::output(&mut command);
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    match out.status.success() {
        true => stdout.lines().last().unwrap_or("").to_string(),
        false => format!("{stdout}{}", String::from_utf8_lossy(&out.stderr)),
    }
}

#[test]
fn run_finds_a_module_under_a_root_by_the_flag_and_by_the_variable() {
    let d = TempDir::new("fibc-roots-run");
    tree(d.path(), &[("lib/pick.fib", &module("pick", 7))]);
    let main = d.file("main.fib", MAIN);
    let lib = d.path().join("lib");
    assert!(run(&[&main], None).contains("module pick is not at"));
    assert_eq!(run(&["-I".as_ref(), &lib, &main], None), "7");
    assert_eq!(run(&[&main], Some(lib.to_str().expect("utf-8"))), "7");
}

#[test]
fn the_order_is_the_main_directory_then_each_i_then_each_fib_lib_directory() {
    let d = TempDir::new("fibc-roots-order");
    tree(
        d.path(),
        &[
            ("i1/pick.fib", &module("pick", 1)),
            ("i2/pick.fib", &module("pick", 2)),
            ("v1/pick.fib", &module("pick", 3)),
        ],
    );
    let main = d.file("main.fib", MAIN);
    let (i1, i2) = (d.path().join("i1"), d.path().join("i2"));
    let v1 = d.path().join("v1");
    let v1 = v1.to_str().expect("utf-8");
    assert_eq!(
        run(&["-I".as_ref(), &i1, "-I".as_ref(), &i2, &main], Some(v1)),
        "1"
    );
    assert_eq!(
        run(&["-I".as_ref(), &i2, "-I".as_ref(), &i1, &main], Some(v1)),
        "2"
    );
    assert_eq!(run(&[&main], Some(v1)), "3");
    d.file("pick.fib", &module("pick", 9));
    assert_eq!(run(&["-I".as_ref(), &i1, &main], Some(v1)), "9");
}

#[test]
fn build_takes_the_roots_and_the_executable_does_not_need_them_again() {
    let d = TempDir::new("fibc-roots-build");
    tree(d.path(), &[("lib/pick.fib", &module("pick", 7))]);
    let main = d.file("main.fib", MAIN);
    let exe = d.path().join("prog");
    let lib = d.path().join("lib");
    let built = bounded::output_within(
        fibc()
            .arg("build")
            .arg(&main)
            .arg("-o")
            .arg(&exe)
            .arg("-I")
            .arg(&lib),
        bounded::COMPILE,
    );
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    std::fs::remove_dir_all(&lib).expect("the roots are gone");
    let out = bounded::output(&mut Command::new(&exe));
    assert_eq!(
        out.status.code(),
        Some(7),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    // And without the flag the build is a rejection, not a link error.
    let other = d.path().join("other");
    let refused = bounded::output_within(
        fibc().arg("build").arg(&main).arg("-o").arg(&other),
        bounded::COMPILE,
    );
    assert_eq!(refused.status.code(), Some(3));
    build(
        &d.file("one.fib", "(defun main () -> i64 1)\n"),
        &d.path().join("one"),
    );
}

#[test]
fn a_command_that_reads_no_program_refuses_the_flag_instead_of_ignoring_it() {
    let out = bounded::output(fibc().args(["cases", "-I", "x"]));
    assert_eq!(
        out.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stderr).contains("-I belongs to a command"));
}

/// The harness runs a case with the roots of its header and no others:
/// `FIB_LIB` in the environment of `fibc cases` reaches neither the
/// interpreter side nor the child. The case rejects a module that `FIB_LIB`
/// would supply; were the child to read it, the two sides would disagree.
#[test]
fn the_harness_gives_the_child_the_roots_of_the_header_and_not_fib_lib() {
    let d = TempDir::new("fibc-roots-harness");
    tree(
        d.path(),
        &[
            ("env/pick.fib", &module("pick", 1)),
            (
                "cases/a/main.fib",
                &case_text("reject", "module pick is not at", ""),
            ),
            ("cases/b/lib/pick.fib", &module("pick", 2)),
            (
                "cases/b/main.fib",
                &case_text("accept", "", ";; roots: lib\n"),
            ),
        ],
    );
    let env_root = d.path().join("env");
    let out = bounded::output_within(
        fibc()
            .arg("cases")
            .arg(d.path().join("cases"))
            .env("FIB_LIB", &env_root),
        bounded::COMPILE,
    );
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(
        out.status.success(),
        "{text}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(text.contains("2 cases: 2 pass, 0 fail"), "{text}");
}

fn case_text(expect: &str, error: &str, roots: &str) -> String {
    let verdict = match expect {
        "reject" => format!(";; expect: reject\n;; error:  {error}\n"),
        _ => ";; expect: accept\n;; result: 2\n;; audit:  clean\n".to_string(),
    };
    format!(
        ";; spec:   syntax §5\n{verdict}{roots}\
         (ns main (:require [pick :as p]))\n(defun main () -> i64 (p/which))\n"
    )
}
