//! The files a program reads (syntax §4.3): `read-file` is `nil` when
//! the file cannot be read to its end or is not UTF-8, in the compiled
//! runtime as in the interpreter, which asks the standard library. A
//! case under cases/ cannot make an unreadable or non-UTF-8 file, a file
//! in /proc or a named pipe, so this test does, and compares the compiled
//! program, and the interpreter run in this process, with what the
//! standard library says of the same paths.
//!
//! A file in /proc says it is empty (its size is 0) and has text, and a
//! pipe has no size at all and delivers its bytes as the writer sends
//! them, so a `read-file` that asked `ftell` how long the file was, or
//! took the first short read for the end, would read nothing of them
//! (spec/bootstrap.md §4, row 2). The pipe is fed by a thread, in pieces
//! with pauses between them, one of which ends inside a character.

#![cfg(unix)]

#[path = "../../lair/tests/common/bounded.rs"]
mod bounded;
#[path = "../../fibref/tests/io_support/mod.rs"]
mod io_support;

use std::ffi::{c_char, CString};
use std::fs;
use std::io::Write;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use fibref::cases::{Outcome, Value};
use fibref::eval::run_source_with;
use io_support::TempDir;

const READER: &str = r#"
(defun report (p: str) -> unit
  (println (match (read-file p)
             ((some t) (str-join [p " some " (show (str-len t))]))
             (nil (str-join [p " nil"])))))

(defun main () -> i64
  (let ((ps (args)))
    (do (dotimes (i (count ps)) (report (nth ps i))) 0)))
"#;

/// The same for one path, as the program's result: the byte length of the
/// text, or -1, which the interpreter run in this process returns.
const PROBE: &str = r#"
(defun main () -> i64
  (let ((ps (args)))
    (match (read-file (nth ps 0))
      ((some t) (str-len t))
      (nil -1))))
"#;

/// What the standard library says of `path`: the byte length of the
/// text, or none.
fn std_says(path: &Path) -> Option<usize> {
    let bytes = fs::read(path).ok()?;
    String::from_utf8(bytes).ok().map(|t| t.len())
}

/// The files, each named by what it is: a text, a few that are empty
/// or larger than the runtime's first buffer (4096 bytes) so that the
/// buffer grows, and the ones that must read as `nil`; and, where there
/// is one, a file of /proc, whose size is 0 and whose text is not.
fn make_files(dir: &Path) -> Vec<PathBuf> {
    let at = |name: &str| dir.join(name);
    let write = |name: &str, bytes: &[u8]| fs::write(at(name), bytes).expect("write a test file");
    write("text", "héllo wörld\n".as_bytes());
    write("empty", b"");
    write("exactly-one-buffer", &[b'a'; 4096]);
    write("just-over", &[b'a'; 4097]);
    write("big", &vec![b'b'; 100_000]);
    write("big-multibyte", "wörld ✓ 𝄞\n".repeat(2000).as_bytes());
    write("bad-ff", b"ok\xff");
    write("bad-truncated", b"abc\xe2\x82");
    write("bad-overlong", b"\xc0\x80");
    write("bad-surrogate", b"\xed\xa0\x80");
    write("unreadable", b"secret");
    fs::set_permissions(at("unreadable"), fs::Permissions::from_mode(0o000)).expect("chmod");
    fs::create_dir(at("directory")).expect("mkdir");
    symlink("nowhere", at("dangling")).expect("symlink");
    let mut paths: Vec<PathBuf> = fs::read_dir(dir)
        .expect("list the test directory")
        .map(|e| e.expect("entry").path())
        .collect();
    paths.sort();
    paths.extend([at("missing"), PathBuf::from("/"), PathBuf::from("/tmp")]);
    paths.extend(proc_file());
    paths
}

/// A file whose length `stat` does not know: /proc/version, a few
/// dozen bytes of text, where the machine has /proc.
fn proc_file() -> Option<PathBuf> {
    let path = PathBuf::from("/proc/version");
    path.exists().then_some(path)
}

/// `fibc run PROGRAM -- ARGS..`, with a deadline.
fn fibc_run(program: &Path, args: &[&Path]) -> std::process::Output {
    bounded::output(
        Command::new(env!("CARGO_BIN_EXE_fibc"))
            .arg("run")
            .arg(program)
            .arg("--")
            .args(args),
    )
}

/// What the interpreter, run in this process, answers for `path`.
fn interpreter_says(path: &Path) -> i64 {
    let arg = path.to_str().expect("test paths are UTF-8").to_string();
    match run_source_with(PROBE, "probe.fib", &[arg]) {
        Outcome::Compiled {
            result: Value::Int(n),
            ..
        } => n,
        other => panic!(
            "the interpreter did not run the probe on {}: {other:?}",
            path.display()
        ),
    }
}

fn expected_line(path: &Path, says: Option<usize>) -> String {
    match says {
        Some(n) => format!("{} some {n}\n", path.display()),
        None => format!("{} nil\n", path.display()),
    }
}

#[test]
fn read_file_is_nil_for_every_file_that_cannot_be_read_whole_and_valid() {
    let dir = TempDir::new("files-compiled");
    let paths = make_files(dir.path());
    let program = dir.file("reader.fib", READER);
    let args: Vec<&Path> = paths.iter().map(PathBuf::as_path).collect();
    let out = fibc_run(&program, &args);
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let expected: String = paths
        .iter()
        .map(|p| expected_line(p, std_says(p)))
        .collect::<String>()
        + "0\n";
    assert!(
        out.status.success(),
        "{stdout}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(stdout, expected);
    if let Some(proc) = proc_file() {
        let line = expected_line(&proc, std_says(&proc));
        assert!(line.contains(" some "), "/proc/version reads: {line}");
        assert!(stdout.contains(&line), "{stdout}");
    }
}

/// The interpreter asks the standard library, so it agrees by
/// construction; the test is that it does, for each of the same paths.
#[test]
fn the_interpreter_answers_what_the_standard_library_does_for_the_same_paths() {
    let dir = TempDir::new("files-interpreted");
    for path in make_files(dir.path()) {
        let want = std_says(&path).map_or(-1, |n| n as i64);
        assert_eq!(interpreter_says(&path), want, "{}", path.display());
    }
}

/// A named pipe in `dir`.
fn make_fifo(dir: &Path) -> PathBuf {
    extern "C" {
        fn mkfifo(path: *const c_char, mode: u32) -> i32;
    }
    let path = dir.join("pipe");
    let name = CString::new(path.to_str().expect("test paths are UTF-8")).expect("no NUL");
    // SAFETY: `name` is a NUL-terminated string that outlives the call.
    let made = unsafe { mkfifo(name.as_ptr(), 0o600) };
    assert_eq!(made, 0, "mkfifo {}", path.display());
    path
}

/// What a writer sends: text of 10 000 bytes and more, longer than the
/// runtime's first buffer, with characters of 2, 3 and 4 bytes.
fn pipe_text() -> String {
    "wörld ✓ 𝄞\n".repeat(700)
}

/// A thread that opens the pipe for writing (which waits for a reader) and
/// sends `content` in pieces of 3001 bytes, so some end inside a
/// character, with a pause after each, so the reader sees short reads.
fn feed(fifo: &Path, content: &[u8]) -> JoinHandle<()> {
    let (fifo, content) = (fifo.to_path_buf(), content.to_vec());
    thread::spawn(move || {
        let Ok(mut pipe) = fs::OpenOptions::new().write(true).open(&fifo) else {
            return;
        };
        for piece in content.chunks(3001) {
            if pipe.write_all(piece).is_err() {
                return;
            }
            thread::sleep(Duration::from_millis(30));
        }
    })
}

/// Runs `read` while a thread feeds `content` into the pipe, and makes
/// sure the thread has ended: a reader that never opened the pipe leaves
/// the writer waiting in `open`, and opening the pipe here lets it go.
fn while_fed<T>(fifo: &Path, content: &[u8], read: impl FnOnce() -> T) -> T {
    let writer = feed(fifo, content);
    let answer = read();
    let release = fs::OpenOptions::new().read(true).write(true).open(fifo);
    writer.join().expect("the writer thread ends");
    drop(release);
    answer
}

/// The standard library reads the whole of the pipe: the baseline the other
/// two are held to, and a check of the writer.
#[test]
fn the_standard_library_reads_the_whole_pipe() {
    let dir = TempDir::new("files-pipe-std");
    let fifo = make_fifo(dir.path());
    let content = pipe_text();
    assert!(content.len() > 4096 * 2);
    let got = while_fed(&fifo, content.as_bytes(), || {
        fs::read(&fifo).expect("the pipe reads")
    });
    assert_eq!(got, content.as_bytes());
}

#[test]
fn the_compiled_read_file_reads_a_pipe_to_its_end() {
    let dir = TempDir::new("files-pipe-compiled");
    let fifo = make_fifo(dir.path());
    let program = dir.file("reader.fib", READER);
    let content = pipe_text();
    let out = while_fed(&fifo, content.as_bytes(), || fibc_run(&program, &[&fifo]));
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(
        out.status.success(),
        "{stdout}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let expected = expected_line(&fifo, Some(content.len())) + "0\n";
    assert_eq!(stdout, expected);
}

#[test]
fn the_interpreters_read_file_reads_a_pipe_to_its_end() {
    let dir = TempDir::new("files-pipe-interpreted");
    let fifo = make_fifo(dir.path());
    let content = pipe_text();
    let got = while_fed(&fifo, content.as_bytes(), || interpreter_says(&fifo));
    assert_eq!(got, content.len() as i64);
}

/// A pipe that delivers bytes that are not UTF-8 reads as `nil`, in both:
/// it is read to its end and then checked, as a file is.
#[test]
fn a_pipe_that_holds_bytes_that_are_not_utf8_reads_as_nil() {
    let dir = TempDir::new("files-pipe-bad");
    let fifo = make_fifo(dir.path());
    let program = dir.file("reader.fib", READER);
    let bad = b"ok\xffok".to_vec();
    assert!(std::str::from_utf8(&bad).is_err());
    let out = while_fed(&fifo, &bad, || fibc_run(&program, &[&fifo]));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        expected_line(&fifo, None) + "0\n",
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(while_fed(&fifo, &bad, || interpreter_says(&fifo)), -1);
}
