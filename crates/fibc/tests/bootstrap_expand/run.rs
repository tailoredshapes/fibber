//! The differential check of the expander: the tool under test against
//! `fibref::expand_dump::expand_files` in process, over groups of files
//! that share the words the tool is given, in batches, a failure
//! localized to its file (spec/bootstrap.md §5).

use std::fmt;
use std::path::{Path, PathBuf};

use fibref::expand_dump::{flags, Options};

use crate::compare::{compare, Difference, Outcome};
use crate::tool::{run_with_flags, Run, Tool, BATCH, LIMIT};

/// Failing files reported per failing batch (the batch is run again one
/// file at a time to find them).
const LOCALIZED: usize = 3;

/// Failures printed in a report; the rest are counted.
const SHOWN: usize = 5;

/// Longest input text quoted in a failure.
const QUOTED: usize = 400;

/// The tool under test and the directory it runs in: the repository root,
/// where it finds `lib/prelude.fib`.
pub struct Sut {
    pub tool: Tool,
    pub cwd: PathBuf,
}

/// Files the tool is run on with the same options: what is compared is
/// the output for them in one process, and the oracle's for the same.
#[derive(Clone, Debug)]
pub struct Group {
    pub label: String,
    pub files: Vec<PathBuf>,
    pub opts: Options,
}

impl Group {
    pub fn new(label: &str, files: Vec<PathBuf>, opts: Options) -> Group {
        Group {
            label: label.to_string(),
            files,
            opts,
        }
    }
}

/// A batch or a file on which the tool and the oracle disagree.
pub struct Failure {
    pub group: String,
    pub words: Vec<String>,
    pub subject: String,
    pub differences: Vec<Difference>,
    pub stderr: String,
    pub input: Option<String>,
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "DIFFERENT in {} [{}]: {}",
            self.group,
            self.words.join(" "),
            self.subject
        )?;
        if let Some(input) = &self.input {
            writeln!(f, "  input: {input}")?;
        }
        for d in &self.differences {
            for line in d.to_string().lines() {
                // compare.rs speaks of the readers of bootstrap.rs.
                let line = line.replace("rust reader", "rust expander");
                writeln!(f, "  {}", line.replace("fibber reader", "fibber expander"))?;
            }
        }
        let stderr: Vec<&str> = self.stderr.lines().take(3).collect();
        if !stderr.is_empty() {
            writeln!(f, "  tool stderr: {}", stderr.join(" | "))?;
        }
        Ok(())
    }
}

/// What the Rust expander prints and exits with for `files` under `opts`:
/// the output of `fibref expand`.
pub fn oracle(files: &[PathBuf], opts: &Options) -> Outcome {
    // The test's paths are built from UTF-8 parts (the repository, the
    // temp directory and file names written here).
    let names: Vec<String> = files
        .iter()
        .map(|f| f.to_str().expect("test paths are UTF-8").to_string())
        .collect();
    let (text, status) = fibref::expand_dump::expand_files(&names, opts);
    Outcome {
        stdout: text.into_bytes(),
        status: i32::from(status),
    }
}

/// The tool run on `files` with the words of `opts`.
pub fn run_tool(sut: &Sut, opts: &Options, files: &[PathBuf]) -> Run {
    run_with_flags(&sut.tool, &flags(opts), files, LIMIT, Some(&sut.cwd))
}

/// Runs the tool over the group's files, at most [`BATCH`] to a process,
/// and compares each batch with the oracle's output; a batch that differs
/// is run again one file at a time, so the failures name files.
pub fn check_group(sut: &Sut, group: &Group) -> Vec<Failure> {
    let mut failures = Vec::new();
    for batch in group.files.chunks(BATCH) {
        let Some(whole) = check_batch(sut, group, batch) else {
            continue;
        };
        let each: Vec<Failure> = batch
            .iter()
            .filter_map(|file| check_batch(sut, group, std::slice::from_ref(file)))
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
fn check_batch(sut: &Sut, group: &Group, batch: &[PathBuf]) -> Option<Failure> {
    let run = run_tool(sut, &group.opts, batch);
    let differences = compare(&oracle(batch, &group.opts), &run.outcome);
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
        group: group.label.clone(),
        words: flags(&group.opts),
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

/// One line for the group, and the first failures in full.
pub fn report(group: &Group, failures: &[Failure]) -> String {
    let words = flags(&group.opts).join(" ");
    let mut out = format!(
        "{} [{words}]: {} inputs, {} failing\n",
        group.label,
        group.files.len(),
        failures.len()
    );
    for failure in failures.iter().take(SHOWN) {
        out.push_str(&failure.to_string());
    }
    if failures.len() > SHOWN {
        out.push_str(&format!("... and {} more\n", failures.len() - SHOWN));
    }
    out
}

/// Runs every group; the report of all, and whether none failed.
pub fn run_groups(sut: &Sut, groups: &[Group]) -> (String, bool) {
    let mut text = String::new();
    let mut ok = true;
    for group in groups {
        let failures = check_group(sut, group);
        ok &= failures.is_empty();
        text.push_str(&report(group, &failures));
    }
    (text, ok)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tmp::TempDir;
    use fibref::expand_dump::RunnerKind;

    fn no_runner() -> Options {
        Options {
            runner: RunnerKind::None,
            ..Options::default()
        }
    }

    #[test]
    fn the_oracle_is_the_expansion_dump_of_the_files_with_its_status() {
        let dir = TempDir::new("run-oracle");
        let ok = dir.path().join("ok.fib");
        let bad = dir.path().join("bad.fib");
        std::fs::write(&ok, "(defun f () -> i64 1)\n").expect("writable");
        std::fs::write(&bad, "(f)\n").expect("writable");
        let both = oracle(&[ok.clone(), bad.clone()], &no_runner());
        let text = String::from_utf8(both.stdout).expect("UTF-8");
        assert!(text.starts_with(&format!(
            "== {}\n-- module main {}\n",
            ok.display(),
            ok.display()
        )));
        assert!(text.contains("error ExpressionAtTopLevel 1:1 0..3: expression at top level\n"));
        assert_eq!(both.status, 1);
        assert_eq!(oracle(&[ok], &no_runner()).status, 0);
    }

    #[test]
    fn a_failure_names_the_group_the_words_and_the_expander_not_the_reader() {
        let failure = Failure {
            group: "corpus".into(),
            words: vec!["--context".into()],
            subject: "a.fib".into(),
            differences: vec![Difference::Status {
                expected: 0,
                actual: 134,
            }],
            stderr: "trap: todo: expand\nsecond\nthird\nfourth\n".into(),
            input: Some("\"x\"".into()),
        };
        let text = failure.to_string();
        assert!(
            text.starts_with("DIFFERENT in corpus [--context]: a.fib\n"),
            "{text}"
        );
        assert!(
            text.contains("exit status: rust expander 0, fibber expander 134"),
            "{text}"
        );
        assert!(
            text.contains("trap: todo: expand | second | third"),
            "{text}"
        );
        assert!(!text.contains("fourth"), "{text}");
    }

    #[test]
    fn a_report_counts_the_inputs_shows_five_failures_and_counts_the_rest() {
        let group = Group::new("g", vec![PathBuf::from("a.fib"); 9], no_runner());
        let one = |n: usize| Failure {
            group: "g".into(),
            words: vec![],
            subject: format!("file-{n}"),
            differences: vec![],
            stderr: String::new(),
            input: None,
        };
        let failures: Vec<Failure> = (0..8).map(one).collect();
        let text = report(&group, &failures);
        assert!(
            text.starts_with("g [--no-runner]: 9 inputs, 8 failing\n"),
            "{text}"
        );
        assert!(
            text.contains("file-4") && !text.contains("file-5"),
            "{text}"
        );
        assert!(text.contains("... and 3 more"), "{text}");
        assert_eq!(
            report(&group, &[]),
            "g [--no-runner]: 9 inputs, 0 failing\n"
        );
    }
}
