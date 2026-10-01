//! The command line (compiler.md §1): `fibc run FILE -- a b` hands the
//! arguments to `(args)` and prints the result after the program's own
//! output; a built executable returns `main`'s result as its exit
//! status and prints nothing of its own.
//!
//! More families of tests ask what the program's surroundings do at
//! their edges (syntax §4.3, §4.5), of `fibc run` and of a built
//! executable, with the questions and inputs that `fibref run` is asked
//! in `crates/fibref/tests/run_io.rs`, and what `fibc build` does with
//! the libraries a program names:
//!
//! - `link.rs`: `-L DIR` and `-l LIB` link a program against a shared
//!   library built with `cc`, and the executable finds it without
//!   `LD_LIBRARY_PATH` (each `-L` is also an rpath, an absolute one);
//! - `roots.rs`: `-I DIR` and `FIB_LIB` name the library roots a module is
//!   found under after the main file's directory, in that order, on `run`
//!   and `build`, and the rule-6 harness gives the child the roots of the
//!   case's header and not `FIB_LIB`;
//! - `opt.rs`: `fibc build` compiles at `-O 2` unless `-O N` says another
//!   level, `fibc run` at `-O 0` (the default is the same bytes as `-O 2`
//!   and not `-O 0`'s, which are more than twice the size);
//! - `args.rs`: an argument that is not UTF-8 reaches `(args)` as
//!   `String::from_utf8_lossy` makes it (seeded byte strings of every kind
//!   of invalid sequence), and `read-file` and `str-from-bytes` agree with
//!   `from_utf8` on the same bytes;
//! - `writes.rs`: `println` and `eprintln` finish a write that is cut
//!   short and trap `println: write failed` when one fails (a full
//!   device), and a normal run writes every byte.

#![cfg(unix)]

#[path = "../../lair/tests/common/bounded.rs"]
mod bounded;
#[path = "../../fibref/tests/io_support/mod.rs"]
mod io_support;

#[path = "cli/args.rs"]
mod args;
#[path = "cli/link.rs"]
mod link;
#[path = "cli/opt.rs"]
mod opt;
#[path = "cli/roots.rs"]
mod roots;
#[path = "cli/writes.rs"]
mod writes;

use std::path::{Path, PathBuf};
use std::process::Command;

fn fibc() -> Command {
    Command::new(env!("CARGO_BIN_EXE_fibc"))
}

/// `fibc build SRC -o EXE`, which must succeed.
fn build(src: &Path, exe: &Path) {
    build_with(src, exe, &[]);
}

/// `fibc build SRC -o EXE` and then `flags`, which must succeed.
fn build_with(src: &Path, exe: &Path, flags: &[&str]) {
    let built = bounded::output_within(
        fibc().arg("build").arg(src).arg("-o").arg(exe).args(flags),
        bounded::COMPILE,
    );
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
}

fn case(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../cases/ownership")
        .join(name)
}

#[test]
fn run_hands_the_arguments_after_the_dashes_to_args() {
    let out = bounded::output(
        fibc()
            .args(["run"])
            .arg(case("186-args-and-println.fib"))
            .args(["--", "a", "b"]),
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{stdout}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(stdout, "hello from fibber\n2\n");
}

#[test]
fn a_built_executable_returns_mains_result_and_prints_nothing_of_its_own() {
    let dir = io_support::TempDir::new("cli-exe");
    let exe = dir.path().join("args");
    build(&case("186-args-and-println.fib"), &exe);
    let out = bounded::output(Command::new(&exe).args(["x", "y", "z"]));
    assert_eq!(
        out.status.code(),
        Some(3),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout), "hello from fibber\n");
}

/// A failed allocation is the trap "out of memory" (SIGABRT after the
/// message), not a write through a null pointer (a segmentation fault,
/// which is what malloc returning NULL used to be): the runtime's own
/// (`array`) and the program's (`alloc`, whose `calloc` was not checked;
/// case 195 is the same for the interpreter).
///
/// The `alloc` programs are built at `-O 0`. A pointer that is only
/// allocated, perhaps stored through and freed never leaves the function,
/// and at `-O 2` (the default of `build`, stdlib design §7 C6) LLVM takes
/// the allocation to succeed and removes it with its null check, so that
/// `(alloc 1000000000000000)` is no trap and the program returns 7 (or 5,
/// with a store and a load of it): an observable difference from the
/// interpreter and from `fibc run` that this test does not pin.
#[test]
fn an_allocation_that_fails_traps_with_out_of_memory() {
    use std::os::unix::process::ExitStatusExt;
    let dir = io_support::TempDir::new("cli-oom");
    // 8e15 bytes: more than any address space, so malloc fails.
    for (name, program, level) in [
        (
            "array",
            "(defun main () -> i64 (array-len (array 1000000000000000 0)))\n",
            "2",
        ),
        (
            "alloc",
            "(defun main () -> i64 (unsafe (let ((p (alloc 1000000000000000))) (do (free p) 7))))\n",
            "0",
        ),
        (
            "alloc-negative",
            "(defun main () -> i64 (unsafe (let ((p (alloc -1))) (do (free p) 7))))\n",
            "0",
        ),
    ] {
        let src = dir.file(&format!("{name}.fib"), program);
        let exe = dir.path().join(name);
        build_with(&src, &exe, &["-O", level]);
        let out = bounded::output(&mut Command::new(&exe));
        assert_eq!(out.status.signal(), Some(6), "{name}: {:?}", out.status);
        assert_eq!(
            String::from_utf8_lossy(&out.stderr),
            "trap: out of memory\n",
            "{name}"
        );
    }
}
