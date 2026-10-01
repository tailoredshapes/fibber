//! The command line (compiler.md §1): `fibc run FILE -- a b` hands the
//! arguments to `(args)` and prints the result after the program's own
//! output; a built executable returns `main`'s result as its exit
//! status and prints nothing of its own.

use std::path::{Path, PathBuf};
use std::process::Command;

fn fibc() -> Command {
    Command::new(env!("CARGO_BIN_EXE_fibc"))
}

fn case(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../cases/ownership")
        .join(name)
}

#[test]
fn run_hands_the_arguments_after_the_dashes_to_args() {
    let out = fibc()
        .args(["run"])
        .arg(case("186-args-and-println.fib"))
        .args(["--", "a", "b"])
        .output()
        .expect("fibc runs");
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
    let dir = std::env::temp_dir().join(format!("fibc-cli-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a temp dir");
    let exe = dir.join("args");
    let built = fibc()
        .args(["build"])
        .arg(case("186-args-and-println.fib"))
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("fibc builds");
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let out = Command::new(&exe)
        .args(["x", "y", "z"])
        .output()
        .expect("the executable runs");
    assert_eq!(
        out.status.code(),
        Some(3),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout), "hello from fibber\n");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A failed allocation is the trap "out of memory" (SIGABRT after the
/// message), not a write through a null pointer (a segmentation fault,
/// which is what malloc returning NULL used to be).
#[test]
fn an_allocation_that_fails_traps_with_out_of_memory() {
    use std::os::unix::process::ExitStatusExt;
    let dir = std::env::temp_dir().join(format!("fibc-cli-oom-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a temp dir");
    let src = dir.join("oom.fib");
    // 8e15 bytes: more than any address space, so malloc fails.
    std::fs::write(
        &src,
        "(defun main () -> i64 (array-len (array 1000000000000000 0)))\n",
    )
    .expect("the program is written");
    let exe = dir.join("oom");
    let built = fibc()
        .args(["build"])
        .arg(&src)
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("fibc builds");
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let out = Command::new(&exe).output().expect("the executable runs");
    assert_eq!(out.status.signal(), Some(6), "{:?}", out.status);
    assert_eq!(
        String::from_utf8_lossy(&out.stderr),
        "trap: out of memory\n"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
