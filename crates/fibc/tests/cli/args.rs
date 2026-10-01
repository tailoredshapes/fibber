//! `(args)` and the bytes of the command line (syntax §4.3): a word that
//! is not UTF-8 reaches the program with each invalid sequence replaced
//! by U+FFFD as `String::from_utf8_lossy` replaces it (one for each
//! maximal prefix of a sequence), in a built executable, whose own runtime
//! makes the string (`fib.str-from-lossy`, rt/str.lir), and in `fibc run`,
//! whose command-line reader does. Before, a built executable made a `str`
//! of the bytes as they were, which is not UTF-8, and `fibc run` panicked
//! in `std::env::args`.
//!
//! `read-file` and `fib.utf8-valid` share the rule that says where a
//! sequence ends, so the same bytes are put in files and the compiled
//! `read-file` is compared with `from_utf8`.

use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;
use std::process::Command;

use super::io_support::{
    bytes_lines, first_difference, os_words, sh, words, TempDir, BYTES_PROGRAM,
};
use super::{bounded, build, fibc};

/// Prints `some` or `nil` for each path it is given: whether the compiled
/// `read-file` found a file of UTF-8 text.
const READER: &str = r#"
(defun main () -> i64
  (let ((ps (args)))
    (do (dotimes (i (count ps))
          (println (match (read-file (nth ps i)) ((some _) "some") (nil "nil"))))
        0)))
"#;

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[test]
fn a_built_executable_reads_arguments_that_are_not_utf8_as_from_utf8_lossy_makes_them() {
    let dir = TempDir::new("exe-args");
    let src = dir.file("bytes.fib", BYTES_PROGRAM);
    let exe = dir.path().join("bytes");
    build(&src, &exe);
    let words = words();
    let argv: Vec<OsString> = std::iter::once(exe.into_os_string())
        .chain(os_words(&words))
        .collect();
    let out = bounded::output(&mut sh("", "", &argv));
    let stdout = text(&out.stdout);
    if let Some(d) = first_difference(&words, &stdout, &bytes_lines(&words)) {
        panic!("{d}\n{}", text(&out.stderr));
    }
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert!(out.stderr.is_empty(), "{}", text(&out.stderr));
}

#[test]
fn fibc_run_reads_arguments_that_are_not_utf8_as_from_utf8_lossy_makes_them() {
    let dir = TempDir::new("run-args");
    let src = dir.file("bytes.fib", BYTES_PROGRAM);
    let words = words();
    let out = bounded::output(fibc().arg("run").arg(&src).arg("--").args(os_words(&words)));
    let stdout = text(&out.stdout);
    // The result of main, 0, is the last line after the program's own.
    let lines = stdout.strip_suffix("0\n").unwrap_or(&stdout);
    if let Some(d) = first_difference(&words, lines, &bytes_lines(&words)) {
        panic!("{d}\n{}", text(&out.stderr));
    }
    assert!(
        stdout.ends_with("\n0\n"),
        "the result 0 is not printed last"
    );
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(out.stderr.is_empty(), "{}", text(&out.stderr));
}

/// `read-file` is `some` for exactly the byte strings that are UTF-8,
/// and those include text with a NUL in it (a file may hold one though a
/// word may not).
#[test]
fn the_compiled_read_file_agrees_with_from_utf8_on_every_word() {
    let dir = TempDir::new("read-file");
    let exe = dir.path().join("reader");
    build(&dir.file("reader.fib", READER), &exe);
    let mut inputs = words();
    inputs.extend([
        b"a\0b".to_vec(),
        b"\0".to_vec(),
        b"\xe2\x82\0".to_vec(),
        b"\xc3\0\xa9".to_vec(),
    ]);
    let paths: Vec<OsString> = inputs
        .iter()
        .enumerate()
        .map(|(i, bytes)| {
            let path = dir.path().join(format!("w{i:04}"));
            std::fs::write(&path, bytes).expect("a scratch file can be written");
            path.into_os_string()
        })
        .collect();
    let out = bounded::output(Command::new(&exe).args(&paths));
    let got = text(&out.stdout);
    let want: String = inputs
        .iter()
        .map(|b| {
            if std::str::from_utf8(b).is_ok() {
                "some\n"
            } else {
                "nil\n"
            }
        })
        .collect();
    let (got, want): (Vec<&str>, Vec<&str>) = (got.lines().collect(), want.lines().collect());
    assert_eq!(got.len(), want.len(), "{}", text(&out.stderr));
    for (i, (g, w)) in got.iter().zip(&want).enumerate() {
        assert_eq!(g, w, "word {i} {:02x?}", inputs[i]);
    }
}

/// The names before `--` are the tool's own: one that is not UTF-8 is
/// refused with a message, not changed into another name (U+FFFD) and not
/// a panic.
#[test]
fn a_name_before_the_dashes_that_is_not_utf8_is_refused() {
    let bad = OsString::from_vec(b"a\xffb.fib".to_vec());
    for command in ["run", "build"] {
        let out = bounded::output(fibc().arg(command).arg(&bad));
        let stderr = text(&out.stderr);
        assert_eq!(out.status.code(), Some(2), "{command}: {stderr}");
        assert!(
            stderr.contains("an argument before `--` is not UTF-8: a\u{fffd}b.fib"),
            "{command}: {stderr}"
        );
        assert!(!stderr.contains("panicked"), "{command}: {stderr}");
    }
}
