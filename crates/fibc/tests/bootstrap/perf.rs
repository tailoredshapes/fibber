//! The reader's speed (spec/bootstrap.md §3). The differential compares
//! what the tool prints, and a reader that is quadratic prints what a
//! linear one prints, only later; the largest corpus file is 5 KB, so
//! nothing in it can tell them apart. The second mutation review found
//! mutants that differ from the reader in speed alone (a comma run
//! skipped one comma at a time, a lookahead memo that never hits, a
//! cursor that re-encodes the source at every peek) and the 120 s limit
//! of a batch (`tool::LIMIT`) kills none of those that take 48 s.
//!
//! So every input of a size that makes the cost visible is read alone, in
//! both modes, with a bound of [`BOUND`] seconds, and must also print what
//! the Rust reader prints:
//!
//! - the committed inputs whose names carry `-perf-`
//!   (`compiler/tests/reader/`, 250 KB each);
//! - two inputs made here, too large to keep in the repository: a run of
//!   8 MiB of commas before a form, which a lookahead memo that never
//!   hits turns into 1001 scans of the run, and a 1 MiB comment, whose
//!   cost is as large as the square of its length for a cursor that
//!   re-encodes the source at each peek.
//!
//! The tool is killed at the bound, so a mutant that would take a minute
//! costs five seconds. Measured on the machine this was written on, with
//! this tool (a release-like build under the test profile), the base
//! reader takes 0.00 to 0.03 s on the three committed inputs and under
//! 0.2 s on the two made here; the mutants take 22 to 48 s on those
//! (`BOOTSTRAP_READER` builds a mutated copy; the review's table is in
//! the report that came with this test). The bound is two orders of
//! magnitude above the base and below the slowest mutant by a factor of
//! more than four, so a runner that is ten times slower or busy still
//! passes, and a reader that is quadratic in the length of a comma run
//! or of a comment does not.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::check::Failure;
use super::compare::{compare, Difference};
use super::tool::{oracle, repo_root, run_tool_within, Mode, Tool, TIMED_OUT};

/// How long the tool may take to read one of the inputs, in one mode.
pub const BOUND: Duration = Duration::from_secs(5);

/// The smallest size of a committed `-perf-` input: one that has been
/// cut down to pass shows here.
const COMMITTED_MIN: u64 = 200_000;

/// The directory of the committed edge inputs.
fn inputs_dir(root: &Path) -> PathBuf {
    root.join("compiler/tests/reader")
}

/// The committed inputs whose names carry `-perf-`, in name order.
pub fn committed(root: &Path) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(inputs_dir(root))
        .map(|entries| {
            entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.file_name()
                        .is_some_and(|n| n.to_string_lossy().contains("-perf-"))
                })
                .collect()
        })
        .unwrap_or_default();
    found.sort();
    found
}

/// Writes the inputs that are made at test time into `dir`.
pub fn generated(dir: &Path) -> Vec<PathBuf> {
    std::fs::create_dir_all(dir).expect("the scratch directory is writable");
    let commas = dir.join("perf-commas-8m-then-form.fib");
    std::fs::write(&commas, format!("{}x\n", ",".repeat(8 << 20))).expect("writable");
    let comment = dir.join("perf-comment-1m.fib");
    std::fs::write(
        &comment,
        format!(";{}\n(x)\n", "abcdefghij".repeat(100_000)),
    )
    .expect("writable");
    vec![commas, comment]
}

/// What the speed check found: the failures and the longest a run took.
pub struct Timed {
    pub failures: Vec<Failure>,
    pub slowest: Duration,
}

/// Runs the tool in `mode` on each of `files` alone, killed after
/// `bound`, and compares its output with the oracle's: a failure is a
/// difference in what it printed, or the run not finishing in time.
pub fn check_speed(tool: &Tool, mode: Mode, files: &[PathBuf], bound: Duration) -> Timed {
    let mut timed = Timed {
        failures: Vec::new(),
        slowest: Duration::ZERO,
    };
    for file in files {
        let start = Instant::now();
        let run = run_tool_within(tool, mode, std::slice::from_ref(file), bound);
        let took = start.elapsed();
        timed.slowest = timed.slowest.max(took);
        let killed = run.outcome.status == TIMED_OUT;
        let differences = if killed || took > bound {
            vec![Difference::TooSlow {
                bound_ms: bound.as_millis(),
                took_ms: took.as_millis(),
            }]
        } else {
            compare(&oracle(std::slice::from_ref(file), mode), &run.outcome)
        };
        if !differences.is_empty() {
            timed.failures.push(Failure {
                mode,
                subject: file.display().to_string(),
                differences,
                stderr: run.stderr,
                input: None,
            });
        }
    }
    timed
}

/// Runs the speed check over the committed and the generated inputs in
/// both modes with [`BOUND`]; the report and whether all passed.
pub fn run_all(tool: &Tool, dir: &Path) -> (String, bool) {
    let mut files = committed(&repo_root());
    files.extend(generated(&dir.join("perf")));
    let mut text = String::new();
    let mut ok = true;
    for mode in Mode::ALL {
        let timed = check_speed(tool, mode, &files, BOUND);
        ok &= timed.failures.is_empty();
        let what = format!(
            "speed [{mode}] (slowest {:.2} s, bound {} s)",
            timed.slowest.as_secs_f64(),
            BOUND.as_secs()
        );
        text.push_str(&super::check::report(&what, files.len(), &timed.failures));
    }
    (text, ok)
}

/// A tool that waits `seconds` and then runs `real`, to show that the
/// check can fail on the real reader: slow, and otherwise right. (A kill
/// at the bound leaves the `sleep` running, and the run ends when its
/// pipes close, so the waits here are fractions of a second.)
fn slowed(dir: &Path, real: &Tool, seconds: f64) -> Tool {
    std::fs::create_dir_all(dir).expect("writable");
    let quoted = |p: &Path| {
        let text = p.to_str().expect("test paths are UTF-8").to_string();
        assert!(!text.contains('\''), "{text}");
        format!("'{text}'")
    };
    let command: Vec<String> = std::iter::once(&real.program)
        .chain(&real.leading)
        .map(|p| quoted(p))
        .collect();
    let script = dir.join("slow.sh");
    std::fs::write(
        &script,
        format!("sleep {seconds}\nexec {} \"$@\"\n", command.join(" ")),
    )
    .expect("writable");
    Tool::script(&script)
}

/// Whether the speed check reports, in each mode, every committed input
/// read by a copy of `real` that is right but slow: the canary of this
/// stage, run on the real tool.
pub fn slow_real_tool_is_noticed(dir: &Path, real: &Tool) -> Result<(), String> {
    let files = committed(&repo_root());
    let slow = slowed(&dir.join("canary-slow"), real, 0.6);
    let bound = Duration::from_millis(300);
    for mode in Mode::ALL {
        let timed = check_speed(&slow, mode, &files, bound);
        let all_slow = timed.failures.len() == files.len()
            && timed
                .failures
                .iter()
                .all(|f| matches!(f.differences[..], [Difference::TooSlow { .. }]));
        if !all_slow {
            let shown: String = timed.failures.iter().map(|f| f.to_string()).collect();
            return Err(format!(
                "a reader that waits 0.6 s was reported slow in {mode} mode on {} of {} \
                 inputs:\n{shown}",
                timed.failures.len(),
                files.len()
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::standin::{replay_tool, Damage};
    use crate::tmp::TempDir;

    #[test]
    fn the_committed_inputs_are_the_perf_files_and_are_large() {
        let files = committed(&repo_root());
        assert!(files.len() >= 3, "{files:?}");
        for f in &files {
            let size = std::fs::metadata(f).expect("exists").len();
            assert!(size >= COMMITTED_MIN, "{} is {size} bytes", f.display());
            assert!(f.extension().is_some_and(|e| e == "fib"), "{}", f.display());
        }
        let mut sorted = files.clone();
        sorted.sort();
        assert_eq!(files, sorted);
    }

    #[test]
    fn the_generated_inputs_are_a_comma_run_and_a_comment() {
        let dir = TempDir::new("perf-generated");
        let files = generated(dir.path());
        assert_eq!(files.len(), 2);
        let commas = std::fs::read(&files[0]).expect("readable");
        assert_eq!(commas.len(), (8 << 20) + 2);
        assert!(commas[..commas.len() - 2].iter().all(|b| *b == b','));
        assert!(commas.ends_with(b"x\n"));
        let comment = std::fs::read_to_string(&files[1]).expect("readable");
        assert!(comment.starts_with(";abcdefghij") && comment.ends_with("\n(x)\n"));
        assert_eq!(comment.len(), 1 + 1_000_000 + 1 + 4);
    }

    /// A tool that waits 0.6 s and then replays the oracle's output
    /// exactly: right, and slow. With a bound of 300 ms the only
    /// difference is the time; with a bound of ten seconds it passes.
    #[test]
    fn a_right_but_slow_tool_fails_on_time_alone_and_passes_with_room() {
        let dir = TempDir::new("perf-slow");
        let files = generated_small(dir.path());
        let replay = replay_tool(
            &dir.path().join("replay"),
            &files,
            Damage::Faithful,
            Mode::Dump,
        );
        let slow = slowed(&dir.path().join("slow"), &replay, 0.6);
        for mode in Mode::ALL {
            let timed = check_speed(&slow, mode, &files, Duration::from_millis(300));
            assert_eq!(timed.failures.len(), files.len(), "{mode}");
            for failure in &timed.failures {
                assert!(
                    matches!(
                        failure.differences[..],
                        [Difference::TooSlow { bound_ms: 300, .. }]
                    ),
                    "{failure}"
                );
                assert_eq!(failure.mode, mode);
            }
            let text = timed.failures[0].to_string();
            assert!(
                text.contains("too slow") && text.contains("the bound is 300 ms"),
                "{text}"
            );
            let roomy = check_speed(&slow, mode, &files, Duration::from_secs(10));
            assert!(roomy.failures.is_empty(), "{mode}: {}", roomy.failures[0]);
            assert!(roomy.slowest >= Duration::from_millis(600));
        }
    }

    /// Within the bound the output is still compared: a fast tool that is
    /// wrong fails, with the difference and not a time.
    #[test]
    fn a_fast_tool_that_prints_the_wrong_thing_is_reported_as_wrong() {
        let dir = TempDir::new("perf-wrong");
        let files = generated_small(dir.path());
        let damaged = replay_tool(
            &dir.path().join("damaged"),
            &files,
            Damage::Character(0),
            Mode::Dump,
        );
        let timed = check_speed(&damaged, Mode::Dump, &files, BOUND);
        assert_eq!(timed.failures.len(), 1);
        assert!(matches!(
            timed.failures[0].differences[..],
            [Difference::Line { number: 2, .. }]
        ));
        assert!(check_speed(&damaged, Mode::Print, &files, BOUND)
            .failures
            .is_empty());
    }

    /// Two small inputs, the shapes of the generated ones.
    fn generated_small(dir: &Path) -> Vec<PathBuf> {
        let inputs = dir.join("inputs");
        std::fs::create_dir_all(&inputs).expect("writable");
        let files = [("a.fib", ",,,,x\n"), ("b.fib", ";abc\n(x)\n")];
        files
            .iter()
            .map(|(name, text)| {
                let path = inputs.join(name);
                std::fs::write(&path, text).expect("writable");
                path
            })
            .collect()
    }
}
