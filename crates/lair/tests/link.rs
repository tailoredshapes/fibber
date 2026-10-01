//! Library directories of `aot::Options` (spec/lir.md §11) and of
//! `lair build -L DIR -l LIB` (§12): a program names a function with
//! `declare`, the options say which library has it and where, and the
//! executable finds the library when it runs without `LD_LIBRARY_PATH`,
//! because each directory is also an absolute rpath.
//!
//! The libraries are tiny C files compiled with `cc -shared`:
//! `liblairtest.so` (`lairtest_triple`) and `liblairtwo.so`
//! (`lairtwo_add`).

#![cfg(unix)]

mod common;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use common::Scratch;
use lair::aot::{build_executable, Options};
use lair::Error;

const TRIPLE_C: &str = "int lairtest_triple(int n) { return 3 * n; }\n";
const ADD_C: &str = "int lairtwo_add(int a, int b) { return a + b; }\n";

/// 3 * 14: 42.
const ONE_LIBRARY: &str = "(declare lairtest_triple i32 (i32))
(define (main i32) ()
  (block entry (ret (call @lairtest_triple (i32 14)))))";

/// 3 * 14 + 5 + 9: 56.
const TWO_LIBRARIES: &str = "(declare lairtest_triple i32 (i32))
(declare lairtwo_add i32 (i32 i32))
(define (main i32) ()
  (block entry
    (ret (call @lairtwo_add (call @lairtest_triple (i32 14)) (i32 14)))))";

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// `libNAME.so` built from `source` in a fresh directory `dir/NAME-dir`;
/// the directory.
fn library(dir: &Path, name: &str, source: &str) -> PathBuf {
    let lib_dir = dir.join(format!("{name}-dir"));
    std::fs::create_dir_all(&lib_dir).expect("a library directory");
    let c = dir.join(format!("{name}.c"));
    std::fs::write(&c, source).expect("the C file is written");
    let built = Command::new("cc")
        .args(["-shared", "-fPIC", "-o"])
        .arg(lib_dir.join(format!("lib{name}.so")))
        .arg(&c)
        .output()
        .expect("cc runs");
    assert!(built.status.success(), "{}", text(&built.stderr));
    lib_dir
}

fn options(dirs: &[&Path], libs: &[&str]) -> Options {
    Options {
        lib_dirs: dirs
            .iter()
            .map(|d| d.to_str().expect("utf-8").into())
            .collect(),
        libs: libs.iter().map(|l| l.to_string()).collect(),
        ..Options::default()
    }
}

fn build(program: &str, exe: &Path, opts: &Options) -> Result<(), Error> {
    let module = lair::for_executable(program).expect("the program is valid lIR");
    build_executable(&module, "link-test", exe, opts)
}

/// The executable, with `LD_LIBRARY_PATH` removed so that only what the
/// link recorded can find a library.
fn run(exe: &Path, cwd: &Path) -> Output {
    Command::new(exe)
        .current_dir(cwd)
        .env_remove("LD_LIBRARY_PATH")
        .output()
        .expect("the executable runs")
}

/// `lair ARGS..` run in `cwd`.
fn lair(cwd: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lair"))
        .current_dir(cwd)
        .args(args)
        .output()
        .expect("lair runs")
}

#[test]
fn an_executable_finds_the_library_it_was_linked_against_without_ld_library_path() {
    let scratch = Scratch::new("link-one");
    let lib = library(scratch.path(), "lairtest", TRIPLE_C);
    let exe = scratch.path().join("one");
    build(ONE_LIBRARY, &exe, &options(&[&lib], &["lairtest"])).expect("links");
    let out = run(&exe, scratch.path());
    assert_eq!(out.status.code(), Some(42), "{}", text(&out.stderr));
}

/// The rpath is what finds it: the same executable, with the library
/// moved, does not start, and starts again when `LD_LIBRARY_PATH` says
/// where the library went.
#[test]
fn the_rpath_is_what_finds_the_library() {
    let scratch = Scratch::new("link-moved");
    let lib = library(scratch.path(), "lairtest", TRIPLE_C);
    let exe = scratch.path().join("one");
    build(ONE_LIBRARY, &exe, &options(&[&lib], &["lairtest"])).expect("links");
    assert_eq!(run(&exe, scratch.path()).status.code(), Some(42));

    let moved = scratch.path().join("moved");
    std::fs::rename(&lib, &moved).expect("the library directory moves");
    let lost = run(&exe, scratch.path());
    assert_ne!(lost.status.code(), Some(42));
    assert!(
        text(&lost.stderr).contains("liblairtest.so: cannot open shared object file"),
        "{}",
        text(&lost.stderr)
    );
    let found = Command::new(&exe)
        .env("LD_LIBRARY_PATH", &moved)
        .output()
        .expect("the executable runs");
    assert_eq!(found.status.code(), Some(42), "{}", text(&found.stderr));
}

/// The rpath is the canonical path, not the spelling given: a symbolic
/// link to the directory can go and the library is still found.
#[test]
fn a_directory_given_by_a_symbolic_link_is_recorded_by_its_real_path() {
    let scratch = Scratch::new("link-symlink");
    let lib = library(scratch.path(), "lairtest", TRIPLE_C);
    let link = scratch.path().join("shortcut");
    std::os::unix::fs::symlink(&lib, &link).expect("a symbolic link");
    let exe = scratch.path().join("one");
    build(ONE_LIBRARY, &exe, &options(&[&link], &["lairtest"])).expect("links");
    std::fs::remove_file(&link).expect("the link goes");
    let out = run(&exe, scratch.path());
    assert_eq!(out.status.code(), Some(42), "{}", text(&out.stderr));
}

#[test]
fn several_directories_and_libraries_are_all_found() {
    let scratch = Scratch::new("link-two");
    let one = library(scratch.path(), "lairtest", TRIPLE_C);
    let two = library(scratch.path(), "lairtwo", ADD_C);
    let exe = scratch.path().join("two");
    let opts = options(&[&one, &two], &["lairtest", "lairtwo"]);
    build(TWO_LIBRARIES, &exe, &opts).expect("links");
    let out = run(&exe, scratch.path());
    assert_eq!(out.status.code(), Some(56), "{}", text(&out.stderr));
}

/// `-L lib` names a directory relative to where `lair` ran; the
/// executable must find it from anywhere, so the rpath is absolute.
/// (A relative rpath would look for `/libs/..` from the root.)
#[test]
fn a_relative_directory_on_the_command_line_becomes_an_absolute_rpath() {
    let scratch = Scratch::new("link-relative");
    library(scratch.path(), "lairtest", TRIPLE_C);
    std::fs::write(scratch.path().join("one.lir"), ONE_LIBRARY).expect("the program");
    let built = lair(
        scratch.path(),
        &[
            "build",
            "one.lir",
            "-o",
            "one",
            "-L",
            "lairtest-dir",
            "-l",
            "lairtest",
        ],
    );
    assert!(built.status.success(), "{}", text(&built.stderr));
    let elsewhere = run(&scratch.path().join("one"), Path::new("/"));
    assert_eq!(
        elsewhere.status.code(),
        Some(42),
        "{}",
        text(&elsewhere.stderr)
    );
}

/// The linker's own words, and nothing left behind.
#[test]
fn a_library_the_linker_cannot_find_gives_the_linkers_message() {
    let scratch = Scratch::new("link-missing");
    let lib = library(scratch.path(), "lairtest", TRIPLE_C);
    let exe = scratch.path().join("one");

    let err = build(ONE_LIBRARY, &exe, &options(&[&lib], &["lairnosuch"])).expect_err("no lib");
    let msg = err.to_string();
    assert!(msg.starts_with("error: linker failed:"), "{msg}");
    assert!(msg.contains("-llairnosuch"), "{msg}");

    // The symbol is named and the library is not: the symbol is undefined.
    let err = build(ONE_LIBRARY, &exe, &options(&[&lib], &[])).expect_err("no -l");
    assert!(
        err.to_string()
            .contains("undefined reference to `lairtest_triple'"),
        "{err}"
    );

    // And the library is there: the same module with both links.
    build(ONE_LIBRARY, &exe, &options(&[&lib], &["lairtest"])).expect("links");
    let left: Vec<String> = std::fs::read_dir(scratch.path())
        .expect("the scratch directory")
        .map(|e| {
            e.expect("an entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .filter(|n| n.ends_with(".lair.o"))
        .collect();
    assert_eq!(left, Vec::<String>::new(), "no object file is left behind");
}

#[test]
fn a_directory_that_cannot_be_used_is_refused_and_nothing_is_written() {
    let scratch = Scratch::new("link-baddir");
    let exe = scratch.path().join("one");
    let file = scratch.path().join("a-file");
    std::fs::write(&file, "").expect("a file");
    let odd = scratch.path().join("a:b");
    std::fs::create_dir_all(&odd).expect("a directory");
    for (dir, reason) in [
        (scratch.path().join("no-such-dir"), "No such file"),
        (file, "not a directory"),
        (odd, "an rpath cannot hold"),
    ] {
        let err = build(ONE_LIBRARY, &exe, &options(&[&dir], &["lairtest"])).expect_err("refused");
        let msg = err.to_string();
        assert!(
            msg.starts_with(&format!("error: -L {}: ", dir.display())),
            "{msg}"
        );
        assert!(msg.contains(reason), "{msg}");
    }
    assert!(!exe.exists(), "no executable");
    let entries = std::fs::read_dir(scratch.path())
        .expect("the scratch")
        .count();
    assert_eq!(entries, 2, "no object file either: only a-file and a:b");
}

#[test]
fn the_command_line_names_a_directory_that_is_missing() {
    let scratch = Scratch::new("link-cli-baddir");
    std::fs::write(scratch.path().join("one.lir"), ONE_LIBRARY).expect("the program");
    let out = lair(
        scratch.path(),
        &["build", "one.lir", "-o", "one", "-L", "nowhere", "-l", "x"],
    );
    assert!(!out.status.success());
    assert!(
        text(&out.stderr).contains("one.lir: error: -L nowhere: "),
        "{}",
        text(&out.stderr)
    );
    assert!(!scratch.path().join("one").exists());
}
