//! The files a program reads (syntax §4.3): `read-file` is `nil` when
//! the file cannot be read to its end or is not UTF-8, in the compiled
//! runtime as in the interpreter, which asks the standard library. A
//! case under cases/ cannot make an unreadable or non-UTF-8 file, so
//! this test does, and compares the compiled program with what the
//! standard library says of the same paths.

use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;

const READER: &str = r#"
(defun report (p: str) -> unit
  (println (match (read-file p)
             ((some t) (str-join [p " some " (show (str-len t))]))
             (nil (str-join [p " nil"])))))

(defun main () -> i64
  (let ((ps (args)))
    (do (dotimes (i (count ps)) (report (nth ps i))) 0)))
"#;

/// What the interpreter's `read-file` answers for `path`: the byte
/// length of the text, or none.
fn std_says(path: &Path) -> Option<usize> {
    let bytes = fs::read(path).ok()?;
    String::from_utf8(bytes).ok().map(|t| t.len())
}

/// The files, each named by what it is: a text, a few that are empty
/// or larger than the runtime's first buffer (4096 bytes) so that the
/// buffer grows, and the ones that must read as `nil`.
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
    paths
}

#[test]
fn read_file_is_nil_for_every_file_that_cannot_be_read_whole_and_valid() {
    let dir = std::env::temp_dir().join(format!("fibc-files-test-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("a temp dir");
    let paths = make_files(&dir);
    let program = dir.join("reader.fib");
    fs::write(&program, READER).expect("write the program");
    let out = Command::new(env!("CARGO_BIN_EXE_fibc"))
        .arg("run")
        .arg(&program)
        .arg("--")
        .args(&paths)
        .output()
        .expect("fibc runs");
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let expected: String = paths
        .iter()
        .map(|p| match std_says(p) {
            Some(n) => format!("{} some {n}\n", p.display()),
            None => format!("{} nil\n", p.display()),
        })
        .collect::<String>()
        + "0\n";
    let _ = fs::set_permissions(dir.join("unreadable"), fs::Permissions::from_mode(0o600));
    let _ = fs::remove_dir_all(&dir);
    assert!(
        out.status.success(),
        "{stdout}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(stdout, expected);
}
