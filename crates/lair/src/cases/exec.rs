//! Running a process with a time limit and collecting its output.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// What a finished process gave.
pub struct Outcome {
    /// The exit code, or 128 + the signal number, as a shell reports it.
    pub status: i32,
    /// The signal that killed it, if one did.
    pub signal: Option<i32>,
    /// It was killed at the time limit; `status` is then 124.
    pub timed_out: bool,
    pub stdout: String,
    pub stderr: String,
}

/// The time a case may take on one path.
pub const LIMIT: Duration = Duration::from_secs(120);

/// Run `program args..`; going over the case time limit is an error.
pub fn run(program: &Path, args: &[&str]) -> Result<Outcome, String> {
    let o = run_within(program, args, LIMIT)?;
    if o.timed_out {
        return Err(format!("timed out after {}s", LIMIT.as_secs()));
    }
    Ok(o)
}

/// Run `program args..`, killing it after `limit`; a kill is reported
/// in the outcome, with what it wrote until then.
pub fn run_within(program: &Path, args: &[&str], limit: Duration) -> Result<Outcome, String> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot run {}: {e}", program.display()))?;
    let out = drain(child.stdout.take());
    let err = drain(child.stderr.take());
    let start = Instant::now();
    let (status, signal, timed_out) = loop {
        match child.try_wait() {
            Ok(Some(st)) => {
                let (status, signal) = code(&st);
                break (status, signal, false);
            }
            Ok(None) if start.elapsed() > limit => {
                let _ = child.kill();
                let _ = child.wait();
                break (124, None, true);
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(2)),
            Err(e) => return Err(format!("wait failed: {e}")),
        }
    };
    Ok(Outcome {
        status,
        signal,
        timed_out,
        stdout: out.join().unwrap_or_default(),
        stderr: err.join().unwrap_or_default(),
    })
}

/// Read a pipe to its end on a thread of its own.
fn drain<R: Read + Send + 'static>(pipe: Option<R>) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut s = String::new();
        if let Some(mut p) = pipe {
            let _ = p.read_to_string(&mut s);
        }
        s
    })
}

/// The shell's status and the signal, if the process was killed by one.
#[cfg(unix)]
fn code(st: &std::process::ExitStatus) -> (i32, Option<i32>) {
    use std::os::unix::process::ExitStatusExt;
    match (st.code(), st.signal()) {
        (Some(c), _) => (c, None),
        (None, Some(sig)) => (128 + sig, Some(sig)),
        (None, None) => (-1, None),
    }
}

#[cfg(not(unix))]
fn code(st: &std::process::ExitStatus) -> (i32, Option<i32>) {
    (st.code().unwrap_or(-1), None)
}
