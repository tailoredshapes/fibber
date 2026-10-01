//! The compiled side of the comparison (compiler.md §5 step 3): the
//! case compiled and run in a child process, `fibc run --trace FILE`,
//! and what its exit status and streams say.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::trace::Trace;

/// The exit status of `fibc run` when the front end rejected the
/// program (its message is on standard error after `rejected:`).
pub const EXIT_REJECTED: i32 = 3;
/// The exit status when the compiler cannot lower the program yet
/// (`unsupported: REASON` on standard error): the harness reports
/// Pending, never a pass.
pub const EXIT_UNSUPPORTED: i32 = 4;
/// The exit status when compilation itself failed (an lIR the checker
/// refused, a JIT error): a compiler bug, never a pass.
pub const EXIT_COMPILE_FAILED: i32 = 5;
/// `abort()` under a shell: 128 + SIGABRT.
const EXIT_ABORT: i32 = 134;

const LIMIT: Duration = Duration::from_secs(300);

/// What the child process said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Said {
    /// It ran and `main` returned this.
    Result(i64),
    /// The front end rejected the program.
    Rejected(String),
    /// The compiler does not lower this program yet.
    Unsupported(String),
    /// The run trapped with this message.
    Trapped(String),
    /// Anything else: a compile failure, a crash, a timeout.
    Failed(String),
}

/// A finished child: what it said and the trace it wrote.
#[derive(Debug, Clone)]
pub struct ChildRun {
    pub said: Said,
    pub trace: Trace,
}

/// Runs `fibc run --trace file` as a child and reads it.
pub fn run(fibc: &Path, file: &Path) -> ChildRun {
    run_in(fibc, file, &[])
}

/// [`run`] with `-I dir` for each of the case's library roots, in order.
pub fn run_in(fibc: &Path, file: &Path, roots: &[PathBuf]) -> ChildRun {
    let out = match spawn(fibc, file, roots) {
        Ok(o) => o,
        Err(m) => {
            return ChildRun {
                said: Said::Failed(m),
                trace: Trace::default(),
            }
        }
    };
    let trace = Trace::parse(&out.stderr);
    ChildRun {
        said: interpret(&out),
        trace,
    }
}

struct Output {
    status: i32,
    stdout: String,
    stderr: String,
}

fn interpret(o: &Output) -> Said {
    let after = |prefix: &str| {
        o.stderr
            .lines()
            .find_map(|l| l.strip_prefix(prefix))
            .map(str::to_string)
    };
    let tail = |prefix: &str| {
        o.stderr
            .split_once(prefix)
            .map(|(_, rest)| rest.trim().to_string())
    };
    match o.status {
        // The result is the last line; the program's own output (println)
        // comes before it (compiler.md §1).
        0 => match o
            .stdout
            .trim()
            .lines()
            .last()
            .map(str::trim)
            .unwrap_or("")
            .parse::<i64>()
        {
            Ok(n) => Said::Result(n),
            Err(_) => Said::Failed(format!(
                "exit 0 but no result on standard output: {:?}",
                o.stdout
            )),
        },
        EXIT_REJECTED => Said::Rejected(tail("rejected:").unwrap_or_default()),
        EXIT_UNSUPPORTED => Said::Unsupported(after("unsupported: ").unwrap_or_default()),
        EXIT_ABORT => match after("trap: ") {
            Some(m) => Said::Trapped(m),
            None => Said::Failed(format!("aborted without a trap message: {}", o.stderr)),
        },
        code => Said::Failed(format!(
            "exit status {code}: {}",
            untraced(&o.stderr).trim()
        )),
    }
}

/// The standard error output without the trace lines.
fn untraced(stderr: &str) -> String {
    stderr
        .lines()
        .filter(|l| Trace::parse(l).lines.is_empty())
        .map(|l| format!("{l}\n"))
        .collect()
}

fn spawn(fibc: &Path, file: &Path, roots: &[PathBuf]) -> Result<Output, String> {
    // The case's roots are its header's alone: FIB_LIB, which the child
    // would read, is not part of the case (the interpreter side does not).
    let mut child = Command::new(fibc)
        .arg("run")
        .arg("--trace")
        .args(roots.iter().flat_map(|r| ["-I".as_ref(), r.as_os_str()]))
        .arg(file)
        .env_remove("FIB_LIB")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot run {}: {e}", fibc.display()))?;
    let out = drain(child.stdout.take());
    let err = drain(child.stderr.take());
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(st)) => break st,
            Ok(None) if start.elapsed() > LIMIT => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("timed out after {}s", LIMIT.as_secs()));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(2)),
            Err(e) => return Err(format!("wait failed: {e}")),
        }
    };
    Ok(Output {
        status: code(&status),
        stdout: out.join().unwrap_or_default(),
        stderr: err.join().unwrap_or_default(),
    })
}

fn drain<R: Read + Send + 'static>(pipe: Option<R>) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut s = String::new();
        if let Some(mut p) = pipe {
            let _ = p.read_to_string(&mut s);
        }
        s
    })
}

#[cfg(unix)]
fn code(st: &std::process::ExitStatus) -> i32 {
    use std::os::unix::process::ExitStatusExt;
    st.code().unwrap_or_else(|| 128 + st.signal().unwrap_or(0))
}

#[cfg(not(unix))]
fn code(st: &std::process::ExitStatus) -> i32 {
    st.code().unwrap_or(-1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn out(status: i32, stdout: &str, stderr: &str) -> Output {
        Output {
            status,
            stdout: stdout.into(),
            stderr: stderr.into(),
        }
    }

    #[test]
    fn reads_each_kind_of_answer() {
        assert_eq!(interpret(&out(0, "42\n", "A 1 o\n")), Said::Result(42));
        assert_eq!(interpret(&out(0, "hello\nworld\n7\n", "")), Said::Result(7));
        assert_eq!(
            interpret(&out(EXIT_REJECTED, "", "rejected:\nt:1:2: bad\n")),
            Said::Rejected("t:1:2: bad".into())
        );
        assert_eq!(
            interpret(&out(EXIT_UNSUPPORTED, "", "unsupported: threads\n")),
            Said::Unsupported("threads".into())
        );
        assert_eq!(
            interpret(&out(134, "", "A 1 o\ntrap: integer / by zero\n")),
            Said::Trapped("integer / by zero".into())
        );
        assert!(matches!(
            interpret(&out(139, "", "A 1 o\n")),
            Said::Failed(_)
        ));
        assert!(matches!(interpret(&out(0, "", "")), Said::Failed(_)));
    }
}
