//! Running a process with a time limit and collecting its output.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// What a finished process gave.
pub struct Outcome {
    /// The exit code, or 128 + the signal number, as a shell reports it.
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
}

const LIMIT: Duration = Duration::from_secs(120);

/// Run `program args..`, killing it after the time limit.
pub fn run(program: &Path, args: &[&str]) -> Result<Outcome, String> {
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
    Ok(Outcome {
        status: code(&status),
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

#[cfg(unix)]
fn code(st: &std::process::ExitStatus) -> i32 {
    use std::os::unix::process::ExitStatusExt;
    st.code().unwrap_or_else(|| 128 + st.signal().unwrap_or(0))
}

#[cfg(not(unix))]
fn code(st: &std::process::ExitStatus) -> i32 {
    st.code().unwrap_or(-1)
}
