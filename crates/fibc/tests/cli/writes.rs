//! `println` and `eprintln` (syntax §4.5) when the system does not do
//! what they ask, in a built executable and in `fibc run`: a write that
//! fails (standard output or standard error on `/dev/full`) is the trap
//! `println: write failed` (`eprintln: ...`), a write that is cut short is
//! finished by the loop of the prelude's `write-str` and the one after it
//! that fails is the same trap, and a normal run writes every byte.
//! Before, the status of `write` was dropped: a full device was a
//! success, with the exit status of `main`, and the bytes a cut-short
//! write left behind were lost without a word.
//!
//! A trap writes `trap: MESSAGE` to standard error and aborts (SIGABRT),
//! so a message can be read only when standard error is not the device
//! that failed; in the other case the test reads what is absent: the
//! output `after` of the statement that follows the failed one.

use std::ffi::OsString;
use std::os::unix::process::ExitStatusExt;
use std::path::Path;
use std::process::{Output, Stdio};

use super::io_support::{
    case_192_stdout, chunking_shim, full_device, limited, long_stdout_program, sh,
    stderr_long_stderr, TempDir, CASE_192, CASE_192_RESULT, STDERR_LONG_PROGRAM,
    STDERR_THEN_STDOUT, STDOUT_THEN_STDERR,
};

use super::{bounded, build};

const SIGABRT: i32 = 6;

/// How a program is run: built to an executable first, or by `fibc run`.
#[derive(Clone, Copy, Debug)]
enum Way {
    Exe,
    FibcRun,
}

const WAYS: [Way; 2] = [Way::Exe, Way::FibcRun];

impl Way {
    /// The command line that runs the program in `src`, building it into
    /// `dir` first when it is an executable.
    fn argv(self, dir: &TempDir, src: &Path) -> Vec<OsString> {
        match self {
            Way::Exe => {
                let exe = dir.path().join("program");
                build(src, &exe);
                vec![exe.into_os_string()]
            }
            Way::FibcRun => vec![
                OsString::from(env!("CARGO_BIN_EXE_fibc")),
                OsString::from("run"),
                src.as_os_str().to_owned(),
            ],
        }
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// `source` run `way`, with standard output (`out_full`) or standard
/// error on `/dev/full` and the other a pipe, core dumps off.
fn run_on_a_full_device(way: Way, source: &str, out_full: bool) -> Output {
    let dir = TempDir::new(&format!("{way:?}-full-{out_full}"));
    let argv = way.argv(&dir, &dir.file("p.fib", source));
    let mut command = sh("", "", &argv);
    if out_full {
        command.stdout(full_device()).stderr(Stdio::piped());
    } else {
        command.stderr(full_device()).stdout(Stdio::piped());
    }
    // Standard output or error is the device, not a pipe, so the child is
    // started here and only waited for with a deadline.
    let child = command
        .stdin(Stdio::null())
        .spawn()
        .expect("the program starts");
    bounded::wait_with_output(child, bounded::RUN, "a program on a full device")
}

#[test]
fn println_on_a_full_device_is_a_trap() {
    for way in WAYS {
        let out = run_on_a_full_device(way, STDOUT_THEN_STDERR, true);
        let stderr = text(&out.stderr);
        assert_eq!(out.status.signal(), Some(SIGABRT), "{way:?}: {stderr}");
        assert_eq!(stderr, "trap: println: write failed\n", "{way:?}");
    }
}

/// The trap's own message goes to the failed standard error and is lost;
/// what shows is the abort, and that `println "after"` never ran.
#[test]
fn eprintln_on_a_full_device_is_a_trap() {
    for way in WAYS {
        let out = run_on_a_full_device(way, STDERR_THEN_STDOUT, false);
        assert_eq!(
            out.status.signal(),
            Some(SIGABRT),
            "{way:?}: {:?}",
            out.status
        );
        assert_eq!(text(&out.stdout), "", "{way:?}: the program went on");
    }
}

/// A write that crosses the limit of the file that is standard output is
/// cut short: 512 or 1024 bytes reach the file (the limit is one block,
/// whose size the shell decides), the loop writes the rest, and that
/// fails. A write that was dropped when short went on to print `after`.
#[test]
fn a_write_cut_short_is_continued_and_the_failure_after_it_is_a_trap() {
    for way in WAYS {
        let dir = TempDir::new(&format!("{way:?}-short"));
        let argv = way.argv(&dir, &dir.file("p.fib", &long_stdout_program()));
        let out_file = dir.path().join("stdout.txt");
        let out = bounded::output(limited(1, &argv).env("OUT", &out_file));
        let stderr = text(&out.stderr);
        assert_eq!(out.status.signal(), Some(SIGABRT), "{way:?}: {stderr}");
        assert_eq!(stderr, "trap: println: write failed\n", "{way:?}");
        let len = std::fs::metadata(&out_file).expect("the file exists").len();
        assert!(
            len == 512 || len == 1024,
            "{way:?}: {len} bytes in the file"
        );
    }
}

/// Case 192 (64 short lines and one of 81920 bytes on standard output)
/// and its standard-error twin, run `way`, with `preload`, a library for
/// `LD_PRELOAD`, when given: every byte written, none twice, and nothing
/// on the other stream. An executable's exit status is the result modulo
/// 256; `fibc run` prints the result after the program's own output.
fn assert_every_byte_is_written(way: Way, preload: Option<&Path>) {
    let dir = TempDir::new(&format!("{way:?}-bytes-{}", preload.is_some()));
    let case = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(CASE_192);
    let twin = dir.file("twin.fib", STDERR_LONG_PROGRAM);
    let run_with = |src: &Path| {
        let mut command = sh("", "", &way.argv(&dir, src));
        if let Some(library) = preload {
            command.env("LD_PRELOAD", library);
        }
        bounded::output(&mut command)
    };
    // An executable is built into the one place `argv` builds it, so the
    // second program replaces the first.
    let code = |r: i64| match way {
        Way::Exe => (r % 256) as i32,
        Way::FibcRun => 0,
    };
    let (out_tail, twin_tail) = match way {
        Way::Exe => (String::new(), "done\n".to_string()),
        Way::FibcRun => (format!("{CASE_192_RESULT}\n"), "done\n5\n".to_string()),
    };

    let out = run_with(&case);
    assert_eq!(out.status.code(), Some(code(CASE_192_RESULT)), "{way:?}");
    let want = case_192_stdout() + &out_tail;
    assert!(
        out.stdout == want.as_bytes(),
        "{way:?}: case 192's output differs"
    );
    assert_eq!(text(&out.stderr), "", "{way:?}");

    let out = run_with(&twin);
    assert_eq!(out.status.code(), Some(code(5)), "{way:?}");
    assert_eq!(text(&out.stdout), twin_tail, "{way:?}");
    assert!(
        out.stderr == stderr_long_stderr().as_bytes(),
        "{way:?}: the twin's standard error differs"
    );
}

/// A normal run still prints, and every byte of it.
#[test]
fn a_normal_run_prints_every_byte_of_a_long_line() {
    for way in WAYS {
        assert_every_byte_is_written(way, None);
    }
}

/// A write the system takes a few bytes at a time (the library that
/// `chunking_shim` makes, preloaded, takes 7) is finished by the loop:
/// the same 82 KB of output, byte for byte, as with whole writes. A loop
/// that did not continue, or went on from the wrong byte, changes it.
#[test]
fn a_write_the_system_takes_a_few_bytes_at_a_time_is_finished() {
    let dir = TempDir::new("chunking-shim");
    let shim = chunking_shim(&dir);
    for way in WAYS {
        assert_every_byte_is_written(way, Some(&shim));
    }
}
