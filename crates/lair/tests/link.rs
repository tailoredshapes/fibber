//! Library directories of `aot::Options` (spec/lir.md §11) and of
//! `lair build -L DIR -l LIB` (§12): a program names a function with
//! `declare`, the options say which library has it and where, and the
//! executable finds the library when it runs without `LD_LIBRARY_PATH`,
//! because each directory is also an absolute rpath.
//!
//! The libraries are tiny C files compiled with `cc -shared`:
//! `liblairtest.so` (`lairtest_triple`) and `liblairtwo.so`
//! (`lairtwo_add`). The last tests do the same through the C interface's
//! `lair_build_executable_with`.

#![cfg(unix)]

mod common;

use std::ffi::c_char;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::ptr;

use common::bounded;
use common::{s, take, Scratch};
use lair::aot::{build_executable, Options};
use lair::capi::lair_build_executable_with;
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
    bounded::output(
        Command::new(exe)
            .current_dir(cwd)
            .env_remove("LD_LIBRARY_PATH"),
    )
}

/// `lair ARGS..` run in `cwd`.
fn lair(cwd: &Path, args: &[&str]) -> Output {
    bounded::output_within(
        Command::new(env!("CARGO_BIN_EXE_lair"))
            .current_dir(cwd)
            .args(args),
        bounded::COMPILE,
    )
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
    // The loader's words differ between libcs; it names the library.
    assert!(
        text(&lost.stderr).contains("liblairtest.so"),
        "{}",
        text(&lost.stderr)
    );
    let found = bounded::output(Command::new(&exe).env("LD_LIBRARY_PATH", &moved));
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

    // The symbol is named and the library is not: the symbol is undefined
    // (GNU ld says "undefined reference to `sym'", lld "undefined symbol:
    // sym": the test asks for the word and the name).
    let err = build(ONE_LIBRARY, &exe, &options(&[&lib], &[])).expect_err("no -l");
    let msg = err.to_string();
    assert!(
        msg.contains("undefined") && msg.contains("lairtest_triple"),
        "{msg}"
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

// ---- the C interface: lair_build_executable_with (spec/compiler.md §9) ----

/// `lair_build_executable_with` as a C caller writes it: the error text,
/// or `None` for success.
fn build_through_c(program: &str, exe: &Path, libs: &[&str], dirs: &[&Path]) -> Option<String> {
    let strs = |v: &[&str]| -> (Vec<*const c_char>, Vec<usize>) {
        (
            v.iter().map(|x| x.as_ptr().cast()).collect(),
            v.iter().map(|x| x.len()).collect(),
        )
    };
    let dirs: Vec<&str> = dirs.iter().map(|d| d.to_str().expect("utf-8")).collect();
    let ((lp, ll), (dp, dl)) = (strs(libs), strs(&dirs));
    let (c, cl) = s(program);
    let (p, pl) = s(exe.to_str().expect("utf-8"));
    // SAFETY: every pointer is to bytes of the length beside it, alive
    // for the call.
    unsafe {
        take(lair_build_executable_with(
            c,
            cl,
            p,
            pl,
            0,
            lp.as_ptr(),
            ll.as_ptr(),
            libs.len(),
            dp.as_ptr(),
            dl.as_ptr(),
            dirs.len(),
        ))
    }
}

/// The directories of the C interface are written as the Rust options'
/// are: -L and an absolute rpath, so the executable runs from another
/// directory with no `LD_LIBRARY_PATH`; and the function without them
/// still cannot find a library outside the system's.
#[test]
fn the_c_interface_writes_an_rpath_for_each_directory_it_is_given() {
    let scratch = Scratch::new("link-capi");
    let one = library(scratch.path(), "lairtest", TRIPLE_C);
    let two = library(scratch.path(), "lairtwo", ADD_C);
    let exe = scratch.path().join("two");
    let got = build_through_c(TWO_LIBRARIES, &exe, &["lairtest", "lairtwo"], &[&one, &two]);
    assert_eq!(got, None);
    let out = run(&exe, Path::new("/"));
    assert_eq!(out.status.code(), Some(56), "{}", text(&out.stderr));

    std::fs::remove_file(&exe).expect("the executable goes");
    let without = build_through_c(ONE_LIBRARY, &exe, &["lairtest"], &[]).expect("no -L: refused");
    assert!(without.starts_with("error: linker failed:"), "{without}");
    assert!(!exe.exists());
}

#[test]
fn the_c_interface_refuses_a_directory_that_cannot_be_used_and_a_null_list() {
    let scratch = Scratch::new("link-capi-bad");
    let exe = scratch.path().join("one");
    let nowhere = scratch.path().join("no-such-dir");
    let got = build_through_c(ONE_LIBRARY, &exe, &["lairtest"], &[&nowhere]).expect("refused");
    assert!(
        got.starts_with(&format!("error: -L {}: ", nowhere.display())) && got.contains("No such"),
        "{got}"
    );
    assert!(!exe.exists());
    // SAFETY: the null pointers are the case; `src` and `path` are valid.
    let null_dirs = unsafe {
        let (c, cl) = s(ONE_LIBRARY);
        let (p, pl) = s(exe.to_str().expect("utf-8"));
        take(lair_build_executable_with(
            c,
            cl,
            p,
            pl,
            0,
            ptr::null(),
            ptr::null(),
            0,
            ptr::null(),
            ptr::null(),
            2,
        ))
    };
    assert_eq!(
        null_dirs.as_deref(),
        Some("dirs and dir_lens must not be null for n_dirs 2")
    );
}
