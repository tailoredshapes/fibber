//! The tests of `common/bounded.rs`, the child process with a deadline
//! that the link, writes, args and C-consumer tests wait on their children
//! with: a child that hangs is killed, with what it started, and the test
//! that waited fails naming it.

#![cfg(unix)]

#[path = "common/bounded.rs"]
mod bounded;

use std::process::Command;
use std::time::{Duration, Instant};

use bounded::{output, output_within};

fn sh(script: &str) -> Command {
    let mut command = Command::new("sh");
    command.arg("-c").arg(script);
    command
}

#[test]
fn a_child_that_finishes_is_captured_as_output_would_capture_it() {
    let out = output(&mut sh("printf out; printf err >&2; exit 3"));
    assert_eq!(out.status.code(), Some(3));
    assert_eq!(out.stdout, b"out");
    assert_eq!(out.stderr, b"err");
}

#[test]
fn a_child_that_reads_its_standard_input_sees_it_closed() {
    let out = output(&mut sh("cat; echo done"));
    assert_eq!(out.stdout, b"done\n");
}

/// A child that hangs is killed and the test fails, naming what it
/// had written, within the limit and not at the end of the job.
#[test]
fn a_child_that_hangs_is_killed_and_reported() {
    let start = Instant::now();
    let caught = std::panic::catch_unwind(|| {
        output_within(
            &mut sh("echo started; sleep 600"),
            Duration::from_millis(300),
        )
    });
    let message = *caught
        .expect_err("a hang must fail")
        .downcast::<String>()
        .unwrap();
    assert!(message.contains("did not finish in 300ms"), "{message}");
    assert!(message.contains("started"), "{message}");
    assert!(
        start.elapsed() < Duration::from_secs(30),
        "{:?}",
        start.elapsed()
    );
}

/// What the child started is killed with it: the `sleep` of the
/// process group is gone, so the one that holds the pipes open does
/// not outlive the test.
#[test]
fn the_processes_a_child_started_die_with_it() {
    let marker = std::env::temp_dir().join(format!("bounded-marker-{}", std::process::id()));
    let script = format!("(sleep 3; touch '{}') & wait", marker.display());
    let _ = std::fs::remove_file(&marker);
    let caught =
        std::panic::catch_unwind(|| output_within(&mut sh(&script), Duration::from_millis(300)));
    assert!(caught.is_err());
    std::thread::sleep(Duration::from_secs(4));
    let survived = marker.exists();
    let _ = std::fs::remove_file(&marker);
    assert!(!survived, "the grandchild outlived the kill");
}
