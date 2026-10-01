//! The tool run with words that name no program (spec/bootstrap.md §5):
//! no file, only options, an option that is not one, a limit that is not
//! a number. Each prints nothing on standard output, a usage line on
//! standard error and ends with status 2, as `fibref expand` does.

use std::path::PathBuf;
use std::time::Duration;

use crate::compare::Difference;
use crate::run::{Failure, Sut};
use crate::tool::run_with_flags;

/// The word lists the check runs, each with what is wrong with it.
pub const BAD_WORDS: [&[&str]; 5] = [
    &[],
    &["--context"],
    &["--bogus", "a.fib"],
    &["--max-steps", "x", "a.fib"],
    &["--max-steps"],
];

/// How long a tool that was given no program may take: it must not read
/// anything.
const SHORT: Duration = Duration::from_secs(30);

/// A failure for each word list on which the tool does not refuse.
pub fn check_usage(sut: &Sut) -> Vec<Failure> {
    BAD_WORDS
        .iter()
        .filter_map(|words| refusal(sut, words))
        .collect()
}

fn refusal(sut: &Sut, words: &[&str]) -> Option<Failure> {
    let words: Vec<String> = words.iter().map(|w| w.to_string()).collect();
    let run = run_with_flags(&sut.tool, &words, &[] as &[PathBuf], SHORT, Some(&sut.cwd));
    let mut differences = Vec::new();
    if !run.outcome.stdout.is_empty() {
        differences.push(Difference::Line {
            number: 1,
            file: None,
            expected: None,
            actual: Some(String::from_utf8_lossy(&run.outcome.stdout).into_owned()),
        });
    }
    if run.outcome.status != 2 {
        differences.push(Difference::Status {
            expected: 2,
            actual: run.outcome.status,
        });
    }
    if run.stderr.is_empty() {
        differences.push(Difference::NoUsageLine);
    }
    (!differences.is_empty()).then(|| Failure {
        group: "usage".to_string(),
        words: words.clone(),
        subject: format!("the tool run with {words:?} and no file"),
        differences,
        stderr: run.stderr,
        input: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tmp::TempDir;
    use crate::tool::Tool;

    fn script(dir: &TempDir, name: &str, body: &str) -> Sut {
        let path = dir.path().join(name);
        std::fs::write(&path, body).expect("writable");
        Sut {
            tool: Tool::script(&path),
            cwd: dir.path().to_path_buf(),
        }
    }

    #[test]
    fn a_tool_that_refuses_with_a_usage_line_and_status_two_passes() {
        let dir = TempDir::new("usage-ok");
        let sut = script(
            &dir,
            "ok.sh",
            "echo 'usage: expand [options] FILE..' >&2\nexit 2\n",
        );
        assert!(check_usage(&sut).is_empty());
    }

    #[test]
    fn a_tool_that_prints_a_dump_exits_zero_or_is_silent_is_reported_for_each_word_list() {
        let dir = TempDir::new("usage-bad");
        let prints = script(&dir, "prints.sh", "echo hello\necho usage >&2\nexit 2\n");
        let failures = check_usage(&prints);
        assert_eq!(failures.len(), BAD_WORDS.len());
        assert!(failures[0].to_string().contains("hello"), "{}", failures[0]);
        let zero = script(&dir, "zero.sh", "echo usage >&2\nexit 0\n");
        assert_eq!(check_usage(&zero).len(), BAD_WORDS.len());
        let silent = script(&dir, "silent.sh", "exit 2\n");
        let failures = check_usage(&silent);
        assert_eq!(failures.len(), BAD_WORDS.len());
        assert!(
            failures[0].to_string().contains("usage line"),
            "{}",
            failures[0]
        );
    }

    #[test]
    fn a_tool_that_refuses_only_some_of_the_word_lists_is_reported_for_the_rest() {
        let dir = TempDir::new("usage-some");
        // Refuses unless an option is among the words.
        let sut = script(
            &dir,
            "some.sh",
            "case \"$*\" in *--bogus*) echo ok;; *) echo usage >&2; exit 2;; esac\n",
        );
        let failures = check_usage(&sut);
        assert_eq!(failures.len(), 1);
        assert!(
            failures[0].subject.contains("--bogus"),
            "{}",
            failures[0].subject
        );
    }
}
