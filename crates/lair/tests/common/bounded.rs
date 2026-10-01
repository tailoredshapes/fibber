//! A child process with a deadline. `Command::output` waits for ever on
//! a child that hangs (an executable that blocks on a pipe, a link that
//! never ends), and a hung test then holds a CI job until its own limit.
//! [`output`] and [`output_within`] are `Command::output` that kill the
//! child, and the process group it leads, when `limit` has passed, and
//! then panic naming the command, the limit and what the child had
//! written, so the test fails and says what hung.
//!
//! Shared by `crates/lair/tests` (through `common`) and `crates/fibc/tests`
//! (`#[path]`), which have no crate of their own in common. Its own
//! tests are `crates/lair/tests/bounded.rs`, so that they run once and
//! not in every test binary that includes it.

#![allow(dead_code)] // each test binary uses some of it

use std::io::Read;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Output, Stdio};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// A program that is run: an executable the test built, a `fibc run`.
pub const RUN: Duration = Duration::from_secs(120);

/// A compile or a link: `fibc build`, `lair build`, `cc`.
pub const COMPILE: Duration = Duration::from_secs(300);

/// A nested `cargo build` of the library with LLVM linked in, which takes
/// minutes on a cold cache.
pub const CARGO: Duration = Duration::from_secs(1800);

extern "C" {
    fn kill(pid: i32, signal: i32) -> i32;
}

const SIGKILL: i32 = 9;

/// A pipe read to its end on a thread, into a buffer that can be looked at
/// while the thread still waits.
struct Drain {
    seen: Arc<Mutex<Vec<u8>>>,
    thread: JoinHandle<()>,
}

fn drain<R: Read + Send + 'static>(pipe: Option<R>) -> Drain {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&seen);
    let thread = thread::spawn(move || {
        let Some(mut pipe) = pipe else { return };
        let mut chunk = [0u8; 8192];
        while let Ok(n) = pipe.read(&mut chunk) {
            if n == 0 {
                break;
            }
            if let Ok(mut bytes) = sink.lock() {
                bytes.extend_from_slice(&chunk[..n]);
            }
        }
    });
    Drain { seen, thread }
}

impl Drain {
    fn text(&self) -> String {
        let bytes = self.seen.lock().map(|b| b.clone()).unwrap_or_default();
        String::from_utf8_lossy(&bytes).into_owned()
    }

    /// Everything the child wrote, once the pipe has closed.
    fn finish(self) -> Vec<u8> {
        let _ = self.thread.join();
        self.seen.lock().map(|b| b.clone()).unwrap_or_default()
    }
}

/// `command` run to its end with standard input empty and standard output
/// and error captured, as `Command::output` does it, killed after
/// [`RUN`]; see [`output_within`].
pub fn output(command: &mut Command) -> Output {
    output_within(command, RUN)
}

/// `command` run to its end with standard input empty and standard output
/// and error captured, killed (with its process group) after `limit`, on
/// which this panics. The child leads a process group of its own, so the
/// processes it started die with it.
pub fn output_within(command: &mut Command, limit: Duration) -> Output {
    let what = format!("{command:?}");
    let child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .spawn()
        .unwrap_or_else(|e| panic!("cannot run {what}: {e}"));
    wait_with_output(child, limit, &what)
}

/// The child of a command the caller set up and spawned (its standard
/// output may be a file or a device, so this does not touch it), waited
/// for at most `limit` and then killed; the captured output is of the
/// streams that were piped. `what` names it in the failure.
pub fn wait_with_output(mut child: Child, limit: Duration, what: &str) -> Output {
    let out = drain(child.stdout.take());
    let err = drain(child.stderr.take());
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if start.elapsed() > limit => {
                let pid = child.id() as i32;
                // SAFETY: kill(2) takes two integers; a negative pid is the
                // group of that leader, which fails harmlessly when the
                // child leads none.
                unsafe { kill(-pid, SIGKILL) };
                let _ = child.kill();
                let _ = child.wait();
                panic!(
                    "{what} did not finish in {limit:?} and was killed\nstdout so far:\n{}\n\
                     stderr so far:\n{}",
                    out.text(),
                    err.text()
                );
            }
            Ok(None) => thread::sleep(Duration::from_millis(5)),
            Err(e) => panic!("cannot wait for {what}: {e}"),
        }
    };
    Output {
        status,
        stdout: out.finish(),
        stderr: err.finish(),
    }
}
