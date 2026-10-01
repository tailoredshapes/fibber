//! The differential check itself: the tool against the oracle over a
//! list of files, in one mode (the dump or `--print`), in batches, with a
//! failure localized to its file and saying which mode failed.

use std::fmt;
use std::path::{Path, PathBuf};

use super::compare::{compare, Difference};
use super::tool::{oracle, run_tool, Mode, Tool, BATCH};

/// Failing files reported per failing batch (the batch is re-run one
/// file at a time to find them).
const LOCALIZED: usize = 3;

/// Failures printed in a report; the rest are counted.
const SHOWN: usize = 5;

/// Longest input text quoted in a failure.
const QUOTED: usize = 400;

/// A batch or a file on which the two readers disagree.
pub struct Failure {
    /// The mode the tool was run in.
    pub mode: Mode,
    /// The file, or the batch, that differs.
    pub subject: String,
    pub differences: Vec<Difference>,
    /// The tool's standard error (a `trap:` message says why it died).
    pub stderr: String,
    /// The input, quoted, when the subject is one file.
    pub input: Option<String>,
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "DIFFERENT in {} mode: {}", self.mode, self.subject)?;
        if let Some(input) = &self.input {
            writeln!(f, "  input: {input}")?;
        }
        for d in &self.differences {
            for line in d.to_string().lines() {
                writeln!(f, "  {line}")?;
            }
        }
        let stderr: Vec<&str> = self.stderr.lines().take(3).collect();
        if !stderr.is_empty() {
            writeln!(f, "  tool stderr: {}", stderr.join(" | "))?;
        }
        Ok(())
    }
}

/// Runs the tool in `mode` over `files`, at most [`BATCH`] per process,
/// and compares each batch's output and status with the oracle's in the
/// same mode. A batch that differs is re-run one file at a time, so the
/// failures name files.
///
/// A run's status is the largest of its files', so a batch cannot show a
/// file's status that is lower than another's; but the tool derives each
/// file's status from the first line of its dump, and that line is
/// compared.
pub fn check_files(tool: &Tool, mode: Mode, files: &[PathBuf]) -> Vec<Failure> {
    let mut failures = Vec::new();
    for batch in files.chunks(BATCH) {
        let Some(whole) = check_batch(tool, mode, batch) else {
            continue;
        };
        let each: Vec<Failure> = batch
            .iter()
            .filter_map(|file| check_batch(tool, mode, std::slice::from_ref(file)))
            .take(LOCALIZED)
            .collect();
        if each.is_empty() {
            failures.push(whole);
        } else {
            failures.extend(each);
        }
    }
    failures
}

/// `None` when the tool and the oracle agree on `batch`.
fn check_batch(tool: &Tool, mode: Mode, batch: &[PathBuf]) -> Option<Failure> {
    let run = run_tool(tool, mode, batch);
    let differences = compare(&oracle(batch, mode), &run.outcome);
    if differences.is_empty() {
        return None;
    }
    let (subject, input) = match batch {
        [one] => (one.display().to_string(), Some(quote_input(one))),
        _ => (
            format!(
                "a batch of {} files starting at {} (each file alone agrees)",
                batch.len(),
                batch[0].display()
            ),
            None,
        ),
    };
    Some(Failure {
        mode,
        subject,
        differences,
        stderr: run.stderr,
        input,
    })
}

fn quote_input(file: &Path) -> String {
    let Ok(bytes) = std::fs::read(file) else {
        return "(cannot be read)".to_string();
    };
    let text = String::from_utf8_lossy(&bytes);
    let head: String = text.chars().take(QUOTED).collect();
    let more = if head.len() < text.len() { " ..." } else { "" };
    format!("{head:?}{more}")
}

/// One line for `what` (a stage and its mode), and the first failures in
/// full.
pub fn report(what: &str, checked: usize, failures: &[Failure]) -> String {
    let mut out = format!("{what}: {checked} inputs, {} failing\n", failures.len());
    for failure in failures.iter().take(SHOWN) {
        out.push_str(&failure.to_string());
    }
    if failures.len() > SHOWN {
        out.push_str(&format!("... and {} more\n", failures.len() - SHOWN));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tmp::TempDir;

    fn failure(n: usize) -> Failure {
        Failure {
            mode: if n.is_multiple_of(2) {
                Mode::Print
            } else {
                Mode::Dump
            },
            subject: format!("file-{n}"),
            differences: vec![Difference::Status {
                expected: 0,
                actual: 134,
            }],
            stderr: "trap: todo: read-all\nsecond\nthird\nfourth\n".to_string(),
            input: Some("\"x\"".to_string()),
        }
    }

    #[test]
    fn a_report_shows_the_first_failures_in_full_and_counts_the_rest() {
        let failures: Vec<Failure> = (0..8).map(failure).collect();
        let text = report("stage [print]", 20, &failures);
        assert!(
            text.starts_with("stage [print]: 20 inputs, 8 failing\n"),
            "{text}"
        );
        assert!(
            text.contains("file-4") && !text.contains("file-5"),
            "{text}"
        );
        assert!(text.contains("... and 3 more"), "{text}");
        assert!(
            text.contains("trap: todo: read-all | second | third"),
            "{text}"
        );
        assert!(!text.contains("fourth"), "{text}");
        assert!(text.contains("exit status: rust reader 0, fibber reader 134"));
    }

    #[test]
    fn a_failure_says_which_mode_it_is_in() {
        let text = failure(0).to_string();
        assert!(
            text.starts_with("DIFFERENT in print mode: file-0\n"),
            "{text}"
        );
        let text = failure(1).to_string();
        assert!(
            text.starts_with("DIFFERENT in dump mode: file-1\n"),
            "{text}"
        );
    }

    #[test]
    fn a_report_of_no_failures_is_one_line() {
        assert_eq!(report("stage", 3, &[]), "stage: 3 inputs, 0 failing\n");
    }

    #[test]
    fn a_long_input_is_quoted_cut_and_an_unreadable_one_says_so() {
        let dir = TempDir::new("check-quote");
        let long = dir.path().join("long.fib");
        std::fs::write(&long, "é".repeat(QUOTED + 50)).expect("writable");
        let quoted = quote_input(&long);
        assert!(quoted.ends_with("\" ..."), "{quoted}");
        assert!(quoted.len() < 2 * QUOTED + 20);
        let short = dir.path().join("short.fib");
        std::fs::write(&short, "(a\n)").expect("writable");
        assert_eq!(quote_input(&short), "\"(a\\n)\"");
        assert_eq!(
            quote_input(&dir.path().join("missing.fib")),
            "(cannot be read)"
        );
    }
}
