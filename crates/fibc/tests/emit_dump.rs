//! `fibc emit-dump` as the binary runs it (spec/bootstrap.md §8): the
//! statuses, the refusal of bad words, and that `fibc emit` prints what the
//! sections of the dump put together are. The sections and options are
//! tested in `src/emit_dump/`.

use std::path::PathBuf;
use std::process::{Command, Output};

fn fibc(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_fibc"))
        .args(args)
        .output()
        .expect("fibc runs")
}

fn case() -> String {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../cases/ownership");
    dir.join("01-return-part-of-argument.fib")
        .to_string_lossy()
        .into_owned()
}

fn text(out: &Output) -> String {
    String::from_utf8(out.stdout.clone()).expect("UTF-8")
}

#[test]
fn the_dump_with_its_header_lines_left_out_is_what_emit_prints() {
    let file = case();
    let emitted = fibc(&["emit", &file]);
    let dumped = fibc(&["emit-dump", &file]);
    assert_eq!(emitted.status.code(), Some(0));
    assert_eq!(dumped.status.code(), Some(0));
    let dump = text(&dumped);
    assert!(dump.starts_with(&format!("== {file}\n;; == section runtime\n")));
    let bare: String = dump
        .split_inclusive('\n')
        .filter(|l| !l.starts_with(";; == section ") && !l.starts_with("== "))
        .collect();
    assert!(bare == text(&emitted), "emit-dump is not the text of emit");
}

#[test]
fn bad_words_refuse_with_usage_on_standard_error_and_status_2() {
    let file = case();
    for bad in [
        vec!["emit-dump"],
        vec!["emit-dump", "--bogus", &file],
        vec!["emit-dump", "--sections", "nope", &file],
        vec!["emit-dump", "--layout", "--macro", "m", &file],
    ] {
        let out = fibc(&bad);
        assert_eq!(out.status.code(), Some(2), "{bad:?}");
        assert!(out.stdout.is_empty(), "{bad:?}");
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(err.starts_with("usage: fibc"), "{bad:?}: {err}");
    }
}

#[test]
fn an_unreadable_file_is_status_2_and_a_rejected_one_status_1() {
    let out = fibc(&["emit-dump", "--sections", "main", "/nonexistent/e0.fib"]);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(text(&out), "== /nonexistent/e0.fib\nunreadable\n");
    let reject = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../cases/ownership/105-reject-macro-float-literal-bogus-width.fib");
    let file = reject.to_string_lossy().into_owned();
    let out = fibc(&["emit-dump", &file]);
    assert_eq!(out.status.code(), Some(1));
    let dump = text(&out);
    assert!(dump.starts_with(&format!("== {file}\n")), "{dump}");
    assert!(dump.contains("\nerror "), "{dump}");
    assert!(!dump.contains(";; =="), "{dump}");
}
