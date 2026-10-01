//! Running the reader under test and the oracle it is compared with.
//!
//! The tool is `compiler/read.fib` built by `fibc build`; it prints what
//! `fibref read [--print] FILE..` prints (spec/bootstrap.md §2). The
//! oracle here reproduces `read_files` of `crates/fibref/src/main.rs` by
//! calling `fibref::dump::dump_source`, or with `--print`
//! `fibref::dump::print_source` (the `fibref` binary is not available to
//! a test of this crate).

use std::fmt;
use std::io::Read;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use super::compare::Outcome;

/// Files per tool process: the tool prints all of them in one go.
pub const BATCH: usize = 100;

/// How long one tool process may run before it is killed.
pub const LIMIT: Duration = Duration::from_secs(120);

/// The status of a process killed for running too long.
pub const TIMED_OUT: i32 = -1;

/// Runs the tool with no core dump (a trap aborts; a core file would land
/// in the working directory) and its address space capped at 4 GiB, so a
/// reader that allocates without end dies instead of taking the machine
/// down.
const WRAPPER: &str = "ulimit -c 0 2>/dev/null; ulimit -v 4194304 2>/dev/null; exec \"$0\" \"$@\"";

/// What the tool is asked to print: the dump of each file, or with
/// `--print`, as the first argument, each top-level form as the printer
/// writes it. A test runs every input in both modes; a failure says which.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Dump,
    Print,
}

impl Mode {
    /// Both modes, the dump first.
    pub const ALL: [Mode; 2] = [Mode::Dump, Mode::Print];

    /// The argument that comes before the files: none for the dump.
    pub fn flag(self) -> Option<&'static str> {
        match self {
            Mode::Dump => None,
            Mode::Print => Some("--print"),
        }
    }

    /// The Rust reader's text for `source` read as `name`.
    fn oracle_text(self, source: &str, name: &str) -> String {
        match self {
            Mode::Dump => fibref::dump::dump_source(source, name),
            Mode::Print => fibref::dump::print_source(source, name),
        }
    }
}

impl fmt::Display for Mode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Mode::Dump => "dump",
            Mode::Print => "print",
        })
    }
}

/// A reader tool: a program and the arguments that come before the files
/// (none for the built reader; the script's path for a stand-in run by
/// `sh`, which needs no execute permission).
pub struct Tool {
    pub program: PathBuf,
    pub leading: Vec<PathBuf>,
}

impl Tool {
    /// A tool that is run as `program FILE..`.
    pub fn program(program: &Path) -> Self {
        Tool {
            program: program.to_path_buf(),
            leading: Vec::new(),
        }
    }

    /// A tool that is a shell script, run as `sh script FILE..`.
    pub fn script(script: &Path) -> Self {
        Tool {
            program: PathBuf::from("sh"),
            leading: vec![script.to_path_buf()],
        }
    }
}

/// One run of a tool: what it printed and its standard error.
pub struct Run {
    pub outcome: Outcome,
    pub stderr: String,
}

/// The root of the repository.
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the repository root exists")
}

/// The reader's source: `compiler/read.fib`, or the file that
/// `BOOTSTRAP_READER` names, which is how a mutation review runs this
/// test on a mutated copy of `compiler/` (its modules load from its own
/// directory; the corpus is still the repository's).
pub fn reader_source() -> PathBuf {
    std::env::var_os("BOOTSTRAP_READER")
        .map(PathBuf::from)
        .unwrap_or_else(|| repo_root().join("compiler/read.fib"))
}

/// Builds the reader ([`reader_source`]) into `dir` with `fibc build` and
/// returns the tool.
pub fn build_reader(dir: &Path) -> Tool {
    let exe = dir.join("read");
    let source = reader_source();
    let out = Command::new(env!("CARGO_BIN_EXE_fibc"))
        .arg("build")
        .arg(&source)
        .arg("-o")
        .arg(&exe)
        .current_dir(repo_root())
        .output()
        .expect("fibc runs");
    assert!(
        out.status.success(),
        "fibc build {} failed ({}):\n{}{}",
        source.display(),
        out.status,
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    Tool::program(&exe)
}

/// What `fibref read [--print] FILE..` prints and exits with: a `== FILE`
/// header and the dump of each file (or, in print mode, its printed
/// forms), `unreadable` for one that cannot be read as UTF-8, and the
/// largest status (1 for a read error, 2 for an unreadable file).
pub fn oracle(files: &[PathBuf], mode: Mode) -> Outcome {
    let mut text = String::new();
    let mut status = 0;
    for file in files {
        // The test's paths are built from UTF-8 parts (the repository,
        // the temp directory and file names written here).
        let name = file.to_str().expect("test paths are UTF-8");
        text.push_str(&format!("== {name}\n"));
        match std::fs::read_to_string(file) {
            Ok(source) => {
                let dump = mode.oracle_text(&source, name);
                if dump.starts_with("error ") {
                    status = status.max(1);
                }
                text.push_str(&dump);
            }
            Err(_) => {
                text.push_str("unreadable\n");
                status = 2;
            }
        }
    }
    Outcome {
        stdout: text.into_bytes(),
        status,
    }
}

/// Runs the tool in `mode` on `files` with the standard [`LIMIT`].
pub fn run_tool(tool: &Tool, mode: Mode, files: &[PathBuf]) -> Run {
    run_tool_within(tool, mode, files, LIMIT)
}

/// Runs the tool in `mode` on `files` (the mode's flag, if any, comes
/// first) and waits at most `limit` for it.
pub fn run_tool_within(tool: &Tool, mode: Mode, files: &[PathBuf], limit: Duration) -> Run {
    let flags: Vec<String> = mode.flag().map(String::from).into_iter().collect();
    run_with_flags(tool, &flags, files, limit, None)
}

/// Runs the tool with the words `flags` before `files`, from the
/// directory `cwd` when one is given, and waits at most `limit` for it.
/// The expander's tests (`bootstrap_expand.rs`) run it this way: its
/// words are not the reader's one flag, and it reads the prelude from
/// the repository.
pub fn run_with_flags(
    tool: &Tool,
    flags: &[String],
    files: &[PathBuf],
    limit: Duration,
    cwd: Option<&Path>,
) -> Run {
    let mut command = Command::new("sh");
    command
        .arg("-c")
        .arg(WRAPPER)
        .arg(&tool.program)
        .args(&tool.leading)
        .args(flags)
        .args(files)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(dir) = cwd {
        command.current_dir(dir);
    }
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(e) => return failed_to_start(&tool.program, &e),
    };
    let stdout = drain(child.stdout.take());
    let stderr = drain(child.stderr.take());
    let status = wait(&mut child, limit);
    let mut stderr = String::from_utf8_lossy(&stderr.join().unwrap_or_default()).into_owned();
    if status.is_none() {
        stderr.push_str(&format!("timed out after {}s\n", limit.as_secs()));
    }
    Run {
        outcome: Outcome {
            stdout: stdout.join().unwrap_or_default(),
            status: status.unwrap_or(TIMED_OUT),
        },
        stderr,
    }
}

fn failed_to_start(program: &Path, e: &std::io::Error) -> Run {
    Run {
        outcome: Outcome {
            stdout: Vec::new(),
            status: TIMED_OUT,
        },
        stderr: format!("cannot run {}: {e}\n", program.display()),
    }
}

fn drain<R: Read + Send + 'static>(pipe: Option<R>) -> JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut p) = pipe {
            let _ = p.read_to_end(&mut bytes);
        }
        bytes
    })
}

/// The child's status, or `None` after killing it for running too long.
fn wait(child: &mut Child, limit: Duration) -> Option<i32> {
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(st)) => {
                return Some(st.code().unwrap_or_else(|| 128 + st.signal().unwrap_or(0)))
            }
            Ok(None) if start.elapsed() > limit => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(2)),
            Err(_) => return None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tmp::TempDir;

    /// A readable file, one that does not read, and one that is missing.
    fn three_files(dir: &Path) -> [PathBuf; 3] {
        std::fs::write(dir.join("ok.fib"), "(f 1 \"é\")\n'x\n").expect("writable");
        std::fs::write(dir.join("bad.fib"), "(f 1").expect("writable");
        [
            dir.join("ok.fib"),
            dir.join("bad.fib"),
            dir.join("missing.fib"),
        ]
    }

    fn text(outcome: &Outcome) -> String {
        String::from_utf8(outcome.stdout.clone()).expect("the oracle prints UTF-8")
    }

    #[test]
    fn the_dump_oracle_prints_every_node_and_the_largest_status() {
        let dir = TempDir::new("tool-oracle-dump");
        let files = three_files(dir.path());
        let d = dir.path().display();
        let expected = format!(
            "== {d}/ok.fib\nlist 3 1:1 0..10\n  sym \"f\" 1:2 1..2\n  int 1 i64 1:4 3..4\n  \
             str \"é\" 1:6 5..9\nlist 2 2:1 11..13\n  sym \"quote\" 2:1 11..12\n  \
             sym \"x\" 2:2 12..13\n== {d}/bad.fib\nerror Unclosed 1:1 0..1: unclosed (\n\
             == {d}/missing.fib\nunreadable\n"
        );
        let outcome = oracle(&files, Mode::Dump);
        assert_eq!(text(&outcome), expected);
        assert_eq!(outcome.status, 2);
        assert_eq!(oracle(&files[..2], Mode::Dump).status, 1);
        assert_eq!(oracle(&files[..1], Mode::Dump).status, 0);
    }

    #[test]
    fn the_print_oracle_prints_one_line_per_form_and_the_same_errors_and_statuses() {
        let dir = TempDir::new("tool-oracle-print");
        let files = three_files(dir.path());
        let d = dir.path().display();
        let expected = format!(
            "== {d}/ok.fib\n(f 1 \"é\")\n(quote x)\n== {d}/bad.fib\n\
             error Unclosed 1:1 0..1: unclosed (\n== {d}/missing.fib\nunreadable\n"
        );
        let outcome = oracle(&files, Mode::Print);
        assert_eq!(text(&outcome), expected);
        assert_eq!(outcome.status, 2);
        assert_eq!(oracle(&files[..2], Mode::Print).status, 1);
        assert_eq!(oracle(&files[..1], Mode::Print).status, 0);
    }

    #[test]
    fn the_two_modes_name_themselves_and_only_print_has_a_flag() {
        assert_eq!(Mode::ALL, [Mode::Dump, Mode::Print]);
        assert_eq!(Mode::Dump.flag(), None);
        assert_eq!(Mode::Print.flag(), Some("--print"));
        assert_eq!(Mode::Dump.to_string(), "dump");
        assert_eq!(Mode::Print.to_string(), "print");
    }

    /// The flag, when there is one, is the first argument, and the files
    /// follow it in order.
    #[test]
    fn the_flag_comes_before_the_files() {
        let echo = Tool {
            program: PathBuf::from("sh"),
            leading: [
                "-c",
                "for a in \"$@\"; do printf '<%s>' \"$a\"; done",
                "echo",
            ]
            .iter()
            .map(PathBuf::from)
            .collect(),
        };
        let files = [PathBuf::from("a.fib"), PathBuf::from("b.fib")];
        let printed = |mode, files: &[PathBuf]| {
            let run = run_tool(&echo, mode, files);
            assert_eq!(run.outcome.status, 0, "{}", run.stderr);
            text(&run.outcome)
        };
        assert_eq!(printed(Mode::Dump, &files), "<a.fib><b.fib>");
        assert_eq!(printed(Mode::Print, &files), "<--print><a.fib><b.fib>");
        assert_eq!(printed(Mode::Dump, &[]), "");
        assert_eq!(printed(Mode::Print, &[]), "<--print>");
    }
}
