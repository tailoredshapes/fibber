//! Library roots (spec/syntax.md §5) for the dumps of spec/bootstrap.md:
//! `fibref expand`, `types` and `own` find a module as `fibref run` does,
//! beside the file, then under each `-I DIR`, then under each directory of
//! `FIB_LIB`, then in the library the executable carries. They used to read
//! the embedded library only, so a change under lib/ made the Rust oracle
//! drift from the tool written in fibber without a sign.

#![cfg(unix)]

mod io_support;

use std::path::Path;
use std::process::{Command, Output};

use io_support::TempDir;

const FIBREF: &str = env!("CARGO_BIN_EXE_fibref");

const MAIN: &str = "(ns main (:require [pick :as p]))\n(defun main () -> i64 (p/which))\n";
const PICK: &str = "(ns pick)\n(defun which () -> i64 7)\n";

fn said(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// `fibref COMMAND ARGS..` with `FIB_LIB` as given (removed when none).
fn dump(command: &str, args: &[&Path], fib_lib: Option<&Path>) -> Output {
    let mut c = Command::new(FIBREF);
    c.arg(command).args(args).env_remove("FIB_LIB");
    if let Some(dir) = fib_lib {
        c.env("FIB_LIB", dir);
    }
    c.output().expect("fibref runs")
}

/// A directory with `lib/pick.fib` (found by no default) and `main.fib`.
fn project(name: &str) -> (TempDir, std::path::PathBuf, std::path::PathBuf) {
    let d = TempDir::new(name);
    let lib = d.path().join("lib");
    std::fs::create_dir_all(&lib).expect("dirs");
    std::fs::write(lib.join("pick.fib"), PICK).expect("write");
    let main = d.file("main.fib", MAIN);
    (d, lib, main)
}

fn check_tool(command: &str) {
    let (_d, lib, main) = project(&format!("roots-dumps-{command}"));
    let without = dump(command, &[&main], None);
    assert_eq!(
        without.status.code(),
        Some(1),
        "{command}: {}",
        said(&without)
    );
    assert!(
        said(&without).contains("module pick is not at"),
        "{command}: {}",
        said(&without)
    );
    // `-I` before the file, and after it, as `run` takes it.
    for words in [
        vec!["-I".as_ref(), lib.as_path(), &main],
        vec![&main, "-I".as_ref(), &lib],
    ] {
        let out = dump(command, &words, None);
        assert_eq!(out.status.code(), Some(0), "{command} -I: {}", said(&out));
        assert!(said(&out).contains("pick"), "{command} -I: {}", said(&out));
    }
    // The same through the environment.
    let out = dump(command, &[&main], Some(&lib));
    assert_eq!(
        out.status.code(),
        Some(0),
        "{command} FIB_LIB: {}",
        said(&out)
    );
    assert!(
        said(&out).contains("pick"),
        "{command} FIB_LIB: {}",
        said(&out)
    );
}

#[test]
fn expand_finds_a_module_under_i_and_fib_lib() {
    check_tool("expand");
}

#[test]
fn types_finds_a_module_under_i_and_fib_lib() {
    check_tool("types");
}

#[test]
fn own_finds_a_module_under_i_and_fib_lib() {
    check_tool("own");
}
