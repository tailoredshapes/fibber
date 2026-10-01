//! What the command-line tests of the program's surroundings share: the
//! `(args)` words that are not UTF-8, the programs that print, and the
//! shell wrapper that makes a write fail. The tests of `fibref run`
//! (`tests/run_io.rs`) use it, and so do those of `fibc run` and of a
//! built executable (`crates/fibc/tests/cli.rs`, which includes this
//! file by path), so both ask the same questions of the same inputs.

#![allow(dead_code)] // each of the two test crates uses some of it

use std::ffi::OsString;
use std::fs::File;
use std::os::unix::ffi::OsStringExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// A fibber program that prints the bytes of each of its arguments, as
/// decimal numbers each followed by a space, one argument to a line: the
/// bytes a program sees in `(args)` are visible, not just the text.
pub const BYTES_PROGRAM: &str = r#"
(defun bytes-line (s: str) -> str
  (let ((bs (str-bytes s)))
    (loop ((i 0) (acc ""))
      (if (< i (array-len bs))
          (recur (+ i 1) (str-concat acc (str-concat (show (zext i64 (array-get bs i))) " ")))
          acc))))

(defun main () -> i64
  (let ((a (args)))
    (do (dotimes (i (count a)) (println (bytes-line (nth a i)))) 0)))
"#;

/// A program that prints to standard output and then to standard error:
/// "after" is written only if the first write did not trap.
pub const STDOUT_THEN_STDERR: &str =
    "(defun main () -> i64 (do (println \"to stdout\") (eprintln \"after\") 7))\n";

/// The same with the streams the other way round.
pub const STDERR_THEN_STDOUT: &str =
    "(defun main () -> i64 (do (eprintln \"to stderr\") (println \"after\") 7))\n";

/// A text of 2560 bytes, longer than the file-size limit [`limited`]
/// sets, so a write of it is cut short.
const LONG_TEXT: &str = "(defun long-text () -> str
  (loop ((s \"0123456789\") (i 0))
    (if (< i 8) (recur (str-concat s s) (+ i 1)) s)))\n";

/// [`STDOUT_THEN_STDERR`] with a long line in the middle of the first.
pub fn long_stdout_program() -> String {
    format!(
        "{LONG_TEXT}(defun main () -> i64 (do (println \"to stdout\") (println (long-text)) (eprintln \"after\") 7))\n"
    )
}

/// [`STDERR_THEN_STDOUT`] with a long line in the middle of the first.
pub fn long_stderr_program() -> String {
    format!(
        "{LONG_TEXT}(defun main () -> i64 (do (eprintln \"to stderr\") (eprintln (long-text)) (println \"after\") 7))\n"
    )
}

/// Case 192, whose output the tests compare byte for byte: 64 short
/// lines, then a line of 81920 bytes, on standard output; the result
/// 81984. Nothing on standard error: a case cannot use `eprintln` (the
/// compiled trace shares standard error), so [`STDERR_LONG_PROGRAM`] is
/// its twin for that stream.
pub const CASE_192: &str = "cases/ownership/192-println-writes-every-byte.fib";
pub const CASE_192_RESULT: i64 = 81984;

fn long_line() -> String {
    "01234567\u{e9}".repeat(8192)
}

pub fn case_192_stdout() -> String {
    let lines: String = (0..64).map(|i| format!("line {i}\n")).collect();
    format!("{lines}{}\n", long_line())
}

/// The standard-error twin of case 192: four short lines and then the
/// line of 81920 bytes on standard error, "done" on standard output,
/// the result 5.
pub const STDERR_LONG_PROGRAM: &str = "(defun long-text () -> str
  (loop ((s \"01234567\u{e9}\") (i 0))
    (if (< i 13) (recur (str-concat s s) (+ i 1)) s)))
(defun main () -> i64
  (do (eprintln \"err 0\") (eprintln \"err 1\") (eprintln \"err 2\") (eprintln \"err 3\")
      (eprintln (long-text))
      (println \"done\")
      5))
";

/// What [`STDERR_LONG_PROGRAM`] writes to standard error.
pub fn stderr_long_stderr() -> String {
    format!("err 0\nerr 1\nerr 2\nerr 3\n{}\n", long_line())
}

/// The xorshift64* generator of the inputs, seeded: no external crate, so
/// the same seed gives the same words on every machine.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    pub fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// A number in `0..n`.
    pub fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// The kinds of byte string a command line can hold, each a few bytes
/// long: the valid ones that bound the encodings, and every way to be
/// invalid. Each is used alone, after "ab", before "cd" and between them.
const KINDS: &[&[u8]] = &[
    // valid text and the first and last scalar value of each length
    b"hello",
    b"h\xc3\xa9llo w\xc3\xb6rld \xe2\x9c\x93 \xf0\x9d\x84\x9e",
    b"\xc2\x80",
    b"\xdf\xbf",
    b"\xe0\xa0\x80",
    b"\xed\x9f\xbf",
    b"\xee\x80\x80",
    b"\xef\xbf\xbd",
    b"\xef\xbf\xbf",
    b"\xf0\x90\x80\x80",
    b"\xf4\x8f\xbf\xbf",
    // a continuation byte with no lead
    b"\x80",
    b"\xbf",
    b"\x80\x80",
    b"\xbf\xc3\xa9",
    // a 2-byte lead cut short by the end, by ASCII, by another lead
    b"\xc2",
    b"\xdf",
    b"\xc2\x41",
    b"\xc2\xc2\xa9",
    // a 3-byte lead cut short after 0, 1 and 2 bytes of it
    b"\xe2",
    b"\xe2\x82",
    b"\xe2\x82\x41",
    b"\xe0\xa0",
    b"\xed\x9f",
    b"\xef\xbf",
    b"\xe1\x41\x41",
    // a 4-byte lead cut short after 0, 1, 2 and 3 bytes of it
    b"\xf0",
    b"\xf0\x90",
    b"\xf0\x90\x80",
    b"\xf0\x90\x80\x41",
    b"\xf4\x8f",
    b"\xf4\x8f\xbf",
    b"\xf1\x80\x80",
    b"\xf1\x80\x41",
    // overlong: a scalar value in more bytes than it needs
    b"\xc0\x80",
    b"\xc0\xaf",
    b"\xc1\xbf",
    b"\xe0\x80\x80",
    b"\xe0\x9f\xbf",
    b"\xf0\x80\x80\x80",
    b"\xf0\x8f\xbf\xbf",
    // a surrogate, D800 to DFFF, encoded
    b"\xed\xa0\x80",
    b"\xed\xad\xbf",
    b"\xed\xb0\x80",
    b"\xed\xbf\xbf",
    b"\xed\xa0\x80\xed\xb0\x80",
    // above 10FFFF, and the leads that no valid text has
    b"\xf4\x90\x80\x80",
    b"\xf4\xbf\xbf\xbf",
    b"\xf5\x80\x80\x80",
    b"\xf7\xbf\xbf\xbf",
    b"\xf8\x88\x80\x80\x80",
    b"\xfb\xbf\xbf\xbf\xbf",
    b"\xfc\x84\x80\x80\x80\x80",
    b"\xfd\xbf\xbf\xbf\xbf\xbf",
    b"\xfe",
    b"\xff",
    b"\xfe\xff",
    b"\xff\xfe\xfd",
    // the word that ends the tool's own words
    b"--",
];

/// A random scalar value, from the ranges that differ in encoded length
/// and at the edges of the surrogates.
fn scalar(rng: &mut Rng) -> char {
    let (lo, hi): (usize, usize) = [
        (0x20, 0x7e),
        (0x80, 0x7ff),
        (0x800, 0xd7ff),
        (0xe000, 0xffff),
        (0x10000, 0x10ffff),
    ][rng.below(5)];
    char::from_u32((lo + rng.below(hi - lo + 1)) as u32).expect("the ranges hold no surrogate")
}

/// One random word: up to 24 pieces, each a byte of a class (ASCII, a
/// continuation, a lead of each length, an invalid lead) or a whole
/// scalar value. No NUL: a command-line word cannot hold one.
fn random_word(rng: &mut Rng) -> Vec<u8> {
    let classes: [(u8, u8); 8] = [
        (0x20, 0x7e),
        (0x80, 0xbf),
        (0xc0, 0xc1),
        (0xc2, 0xdf),
        (0xe0, 0xef),
        (0xf0, 0xf4),
        (0xf5, 0xff),
        (0x01, 0x1f),
    ];
    let mut word = Vec::new();
    for _ in 0..rng.below(25) {
        if rng.below(4) == 0 {
            word.extend(scalar(rng).to_string().bytes());
        } else {
            let (lo, hi) = classes[rng.below(classes.len())];
            word.push(lo + rng.below(usize::from(hi - lo) + 1) as u8);
        }
    }
    word
}

/// The words of the tests: the empty word, every byte alone (255), each
/// kind four ways (alone, after "ab", before "cd", between), and 300
/// random ones from a fixed seed.
pub fn words() -> Vec<Vec<u8>> {
    let mut words: Vec<Vec<u8>> = vec![Vec::new()];
    words.extend((1..=255u8).map(|b| vec![b]));
    for kind in KINDS {
        words.push(kind.to_vec());
        words.push([b"ab".as_slice(), kind].concat());
        words.push([kind, b"cd".as_slice()].concat());
        words.push([b"ab".as_slice(), kind, b"cd".as_slice()].concat());
    }
    let mut rng = Rng::new(20_261_001);
    words.extend((0..300).map(|_| random_word(&mut rng)));
    words
}

/// The words as the operating system's arguments.
pub fn os_words(words: &[Vec<u8>]) -> Vec<OsString> {
    words.iter().cloned().map(OsString::from_vec).collect()
}

/// What [`BYTES_PROGRAM`] prints for `words` if each word reaches the
/// program as `String::from_utf8_lossy` makes it.
pub fn bytes_lines(words: &[Vec<u8>]) -> String {
    let mut text = String::new();
    for word in words {
        for b in String::from_utf8_lossy(word).bytes() {
            text.push_str(&format!("{b} "));
        }
        text.push('\n');
    }
    text
}

/// The first line of `got` that is not the line of `want`, with the word
/// that made it, or none when they agree.
pub fn first_difference(words: &[Vec<u8>], got: &str, want: &str) -> Option<String> {
    let (got, want): (Vec<&str>, Vec<&str>) = (got.lines().collect(), want.lines().collect());
    if got.len() != want.len() {
        return Some(format!("{} lines, wanted {}", got.len(), want.len()));
    }
    let i = (0..want.len()).find(|&i| got[i] != want[i])?;
    Some(format!(
        "word {i} {:02x?}: got `{}`, wanted `{}`",
        words[i], got[i], want[i]
    ))
}

/// `argv` run by `sh` with core dumps off (a trap aborts, and a core file
/// of a process that has LLVM in it is large), after the shell commands
/// `setup`, with `redirect` (such as `1>"$OUT"`) applied to the program.
pub fn sh(setup: &str, redirect: &str, argv: &[OsString]) -> Command {
    let setup = if setup.is_empty() { ":" } else { setup };
    let mut command = Command::new("sh");
    command
        .arg("-c")
        .arg(format!(
            "ulimit -c 0 2>/dev/null; {setup}; exec \"$@\" {redirect}"
        ))
        .arg("sh")
        .args(argv);
    command
}

/// `argv` with its descriptor `fd` (1 or 2) a file that may grow to one
/// block (`ulimit -f 1`: 512 bytes under dash, 1024 under bash) and no
/// more, `SIGXFSZ` ignored: a write that crosses the limit is cut short
/// (the bytes up to it are written and their count returned), and the
/// next write fails with EFBIG. The file is `$OUT`: set it with `env`.
pub fn limited(fd: u8, argv: &[OsString]) -> Command {
    sh("trap '' XFSZ; ulimit -f 1", &format!("{fd}>\"$OUT\""), argv)
}

/// The source of a shared library that makes every write(2) take at most
/// 7 bytes of what it is given: what a pipe, a socket or a write
/// interrupted by a signal may do. Loaded with `LD_PRELOAD` it turns any
/// program's writes into short ones that succeed, which no device
/// available to a test does. (Every descriptor, not just 1 and 2: the
/// interpreter writes to duplicates of them.)
const CHUNKING_SHIM: &str = r#"
#define _GNU_SOURCE
#include <dlfcn.h>
#include <unistd.h>

ssize_t write(int fd, const void *buf, size_t n) {
    static ssize_t (*real)(int, const void *, size_t);
    if (!real) real = (ssize_t (*)(int, const void *, size_t))dlsym(RTLD_NEXT, "write");
    if (n > 7) n = 7;
    return real(fd, buf, n);
}
"#;

/// Compiles [`CHUNKING_SHIM`] with `cc` (which `fibc build` needs to link
/// an executable anyway) into `dir` and returns the library to preload.
pub fn chunking_shim(dir: &TempDir) -> PathBuf {
    let source = dir.file("chunking-shim.c", CHUNKING_SHIM);
    let library = dir.path().join("chunking-shim.so");
    let built = Command::new("cc")
        .args(["-shared", "-fPIC", "-O1", "-o"])
        .arg(&library)
        .arg(&source)
        .arg("-ldl")
        .output()
        .expect("cc runs");
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    library
}

/// `/dev/full` as a descriptor to hand a child: every write to it fails
/// with ENOSPC.
pub fn full_device() -> Stdio {
    Stdio::from(
        File::options()
            .write(true)
            .open("/dev/full")
            .expect("/dev/full opens"),
    )
}

/// A scratch directory removed when the test passes and kept, with its
/// path printed, when it fails.
pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!("fibber-io-{}-{label}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("a scratch directory can be created");
        TempDir(path)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    /// Writes `text` to the file `name` of the directory and returns its path.
    pub fn file(&self, name: &str, text: &str) -> PathBuf {
        let path = self.0.join(name);
        std::fs::write(&path, text).expect("a scratch file can be written");
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!("the scratch files are kept in {}", self.0.display());
        } else {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_words_are_many_fixed_and_without_nul() {
        let words = words();
        assert!(words.len() >= 600, "{} words", words.len());
        assert_eq!(words, self::words());
        assert!(words.iter().all(|w| !w.contains(&0)));
    }

    #[test]
    fn the_words_hold_every_kind_of_invalid_sequence() {
        let invalid = |w: &Vec<u8>| std::str::from_utf8(w).is_err();
        let words = words();
        for lead in [
            &b"\xc2"[..],
            b"\xe2\x82",
            b"\xf0\x90\x80",
            b"\xc0\x80",
            b"\xed\xa0\x80",
            b"\xf4\x90\x80\x80",
            b"\xff",
            b"\x80",
        ] {
            assert!(
                words
                    .iter()
                    .any(|w| invalid(w) && w.windows(lead.len()).any(|s| s == lead)),
                "no invalid word has {lead:02x?}"
            );
        }
    }

    #[test]
    fn the_expected_lines_replace_each_maximal_subpart_with_one_u_fffd() {
        // E2 82 then A: one replacement for the two bytes; F0 90 80 at the
        // end: one for the three; C0 80: two (C0 is never a lead).
        let w = |b: &[u8]| vec![b.to_vec()];
        assert_eq!(bytes_lines(&w(b"\xe2\x82A")), "239 191 189 65 \n");
        assert_eq!(bytes_lines(&w(b"\xf0\x90\x80")), "239 191 189 \n");
        assert_eq!(bytes_lines(&w(b"\xc0\x80")), "239 191 189 239 191 189 \n");
        assert_eq!(bytes_lines(&w(b"")), "\n");
    }
}
