//! `fibc build FILE -o OUT -L DIR -l LIB` (compiler.md §1): the program
//! names a foreign function with `extern`, the flags say where it lives,
//! and the executable finds the library when it runs without
//! `LD_LIBRARY_PATH`, because each `-L` is also an rpath.
//!
//! The library is two tiny C files compiled with `cc -shared`:
//! `libfibtest.so` (`fibtest_triple`) and `libfibtwo.so` (`fibtwo_add`).
//! The program also calls `labs` from libc, which needs no flag.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use super::io_support::TempDir;
use super::{bounded, fibc};

const TRIPLE_C: &str = "long fibtest_triple(long n) { return 3 * n; }\n";
const ADD_C: &str = "long fibtwo_add(long a, long b) { return a + b; }\n";

/// 4 from libc's `labs`, 15 from the library: 19.
const ONE_LIBRARY: &str = "(extern labs (i64) -> i64)
(extern fibtest_triple (i64) -> i64)
(defun main () -> i64 (unsafe (+ (labs -4) (fibtest_triple 5))))
";

/// Both libraries and libc: 4 + 15 + 7.
const TWO_LIBRARIES: &str = "(extern labs (i64) -> i64)
(extern fibtest_triple (i64) -> i64)
(extern fibtwo_add (i64 i64) -> i64)
(defun main () -> i64
  (unsafe (+ (labs -4) (+ (fibtest_triple 5) (fibtwo_add 3 4)))))
";

/// libc alone, as every program before the flags existed.
const LIBC_ONLY: &str = "(extern labs (i64) -> i64)
(defun main () -> i64 (unsafe (labs -4)))
";

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// `libNAME.so` built from `source` in a fresh directory `dir/NAME-dir`;
/// the directory.
fn library(dir: &TempDir, name: &str, source: &str) -> PathBuf {
    let lib_dir = dir.path().join(format!("{name}-dir"));
    std::fs::create_dir_all(&lib_dir).expect("a library directory");
    let c = dir.file(&format!("{name}.c"), source);
    let built = bounded::output_within(
        Command::new("cc")
            .args(["-shared", "-fPIC", "-o"])
            .arg(lib_dir.join(format!("lib{name}.so")))
            .arg(&c),
        bounded::COMPILE,
    );
    assert!(built.status.success(), "{}", text(&built.stderr));
    lib_dir
}

/// `fibc build SRC -o EXE ARGS..` run in `cwd`.
fn build(cwd: &Path, src: &Path, exe: &Path, args: &[&str]) -> Output {
    bounded::output_within(
        fibc()
            .current_dir(cwd)
            .arg("build")
            .arg(src)
            .arg("-o")
            .arg(exe)
            .args(args),
        bounded::COMPILE,
    )
}

/// The executable, with `LD_LIBRARY_PATH` removed so that only what the
/// link recorded can find a library.
fn run(exe: &Path, cwd: &Path) -> Output {
    bounded::output(
        Command::new(exe)
            .current_dir(cwd)
            .env_remove("LD_LIBRARY_PATH"),
    )
}

#[test]
fn an_executable_finds_the_library_it_was_linked_against_without_ld_library_path() {
    let dir = TempDir::new("link-one");
    let lib = library(&dir, "fibtest", TRIPLE_C);
    let src = dir.file("one.fib", ONE_LIBRARY);
    let exe = dir.path().join("one");
    let built = build(
        dir.path(),
        &src,
        &exe,
        &["-L", lib.to_str().unwrap(), "-l", "fibtest"],
    );
    assert!(built.status.success(), "{}", text(&built.stderr));
    let out = run(&exe, dir.path());
    assert_eq!(out.status.code(), Some(19), "{}", text(&out.stderr));
    assert_eq!(text(&out.stdout), "");
}

/// The rpath is what finds it: the same executable, with the library
/// moved, does not start, and starts again when `LD_LIBRARY_PATH` says
/// where the library went.
#[test]
fn the_rpath_is_what_finds_the_library() {
    let dir = TempDir::new("link-moved");
    let lib = library(&dir, "fibtest", TRIPLE_C);
    let src = dir.file("one.fib", ONE_LIBRARY);
    let exe = dir.path().join("one");
    let flags = ["-L", lib.to_str().unwrap(), "-l", "fibtest"];
    let built = build(dir.path(), &src, &exe, &flags);
    assert!(built.status.success(), "{}", text(&built.stderr));
    assert_eq!(run(&exe, dir.path()).status.code(), Some(19));

    let moved = dir.path().join("moved");
    std::fs::rename(&lib, &moved).expect("the library directory moves");
    let lost = run(&exe, dir.path());
    assert_ne!(lost.status.code(), Some(19));
    // The loader's words differ between libcs; it names the library.
    assert!(
        text(&lost.stderr).contains("libfibtest.so"),
        "{}",
        text(&lost.stderr)
    );
    let found = bounded::output(Command::new(&exe).env("LD_LIBRARY_PATH", &moved));
    assert_eq!(found.status.code(), Some(19), "{}", text(&found.stderr));
}

/// `-L lib` names a directory relative to where `fibc` ran; the
/// executable must find it from anywhere, so the rpath is absolute.
#[test]
fn a_relative_directory_becomes_an_absolute_rpath() {
    let dir = TempDir::new("link-relative");
    library(&dir, "fibtest", TRIPLE_C);
    let src = dir.file("one.fib", ONE_LIBRARY);
    let exe = dir.path().join("one");
    let built = build(
        dir.path(),
        &src,
        &exe,
        &["-L", "fibtest-dir", "-l", "fibtest"],
    );
    assert!(built.status.success(), "{}", text(&built.stderr));
    let elsewhere = run(&exe, Path::new("/"));
    assert_eq!(
        elsewhere.status.code(),
        Some(19),
        "{}",
        text(&elsewhere.stderr)
    );
}

#[test]
fn several_directories_and_libraries_and_the_glued_spellings() {
    let dir = TempDir::new("link-two");
    let one = library(&dir, "fibtest", TRIPLE_C);
    let two = library(&dir, "fibtwo", ADD_C);
    let src = dir.file("two.fib", TWO_LIBRARIES);
    let exe = dir.path().join("two");
    let (l1, l2) = (
        format!("-L{}", one.display()),
        format!("-L{}", two.display()),
    );
    let built = build(
        dir.path(),
        &src,
        &exe,
        &[&l1, "-lfibtest", &l2, "-l", "fibtwo"],
    );
    assert!(built.status.success(), "{}", text(&built.stderr));
    let out = run(&exe, dir.path());
    assert_eq!(out.status.code(), Some(26), "{}", text(&out.stderr));
}

/// A program that names only libc builds with no flag, as before.
#[test]
fn libc_needs_no_flag() {
    let dir = TempDir::new("link-libc");
    let src = dir.file("libc.fib", LIBC_ONLY);
    let exe = dir.path().join("libc");
    let built = build(dir.path(), &src, &exe, &[]);
    assert!(built.status.success(), "{}", text(&built.stderr));
    assert_eq!(run(&exe, dir.path()).status.code(), Some(4));
}

/// The flags are what makes the link succeed: without them the linker
/// names the symbol it cannot find; with `-l` and no `-L` it cannot find
/// the library.
#[test]
fn without_the_flags_the_link_fails_and_names_what_is_missing() {
    let dir = TempDir::new("link-missing");
    let lib = library(&dir, "fibtest", TRIPLE_C);
    let src = dir.file("one.fib", ONE_LIBRARY);
    let exe = dir.path().join("one");

    let bare = build(dir.path(), &src, &exe, &[]);
    assert_eq!(bare.status.code(), Some(5), "{}", text(&bare.stderr));
    // GNU ld says "undefined reference to `sym'", lld "undefined symbol:
    // sym": the test asks for the word and the name.
    let said = text(&bare.stderr);
    assert!(
        said.contains("undefined") && said.contains("fibtest_triple"),
        "{said}"
    );
    assert!(!exe.exists(), "no executable is left behind");

    let no_dir = build(dir.path(), &src, &exe, &["-l", "fibtest"]);
    assert_eq!(no_dir.status.code(), Some(5), "{}", text(&no_dir.stderr));
    assert!(
        text(&no_dir.stderr).contains("-lfibtest"),
        "{}",
        text(&no_dir.stderr)
    );

    // And the library is there: the same line with the directory links.
    let both = build(
        dir.path(),
        &src,
        &exe,
        &["-L", lib.to_str().unwrap(), "-l", "fibtest"],
    );
    assert!(both.status.success(), "{}", text(&both.stderr));
}

#[test]
fn a_directory_that_does_not_exist_is_refused_before_anything_is_compiled() {
    let dir = TempDir::new("link-nodir");
    let src = dir.file("libc.fib", LIBC_ONLY);
    let exe = dir.path().join("libc");
    let out = build(dir.path(), &src, &exe, &["-L", "no-such-dir", "-l", "x"]);
    assert_eq!(out.status.code(), Some(2), "{}", text(&out.stderr));
    assert!(
        text(&out.stderr).starts_with("fibc: -L no-such-dir: "),
        "{}",
        text(&out.stderr)
    );
    assert!(!exe.exists());
}

#[test]
fn a_directory_an_rpath_cannot_hold_is_refused() {
    let dir = TempDir::new("link-colon");
    let odd = dir.path().join("a:b");
    std::fs::create_dir_all(&odd).expect("a directory");
    let src = dir.file("libc.fib", LIBC_ONLY);
    let exe = dir.path().join("libc");
    let out = build(dir.path(), &src, &exe, &["-L", odd.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(2), "{}", text(&out.stderr));
    assert!(
        text(&out.stderr).contains("an rpath cannot hold"),
        "{}",
        text(&out.stderr)
    );
}

#[test]
fn the_usage_text_says_what_build_takes() {
    let out = bounded::output(fibc().args(["build", "x.fib"]));
    assert_eq!(out.status.code(), Some(2));
    assert!(
        text(&out.stderr).contains("build <file> -o <out> [-O N] [-L dir].. [-l lib].."),
        "{}",
        text(&out.stderr)
    );
}
