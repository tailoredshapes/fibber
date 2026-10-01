//! The tool started with no file to read (spec/bootstrap.md §2): `fibref
//! read` with no file, or with `--print` alone, prints its usage and
//! ends with status 2. The tool prints nothing to standard output, a
//! usage line to standard error, and exits 2. This is the plumbing of the
//! arguments, not a comparison of text: the `fibref` binary is not
//! available to the test, so the documented behaviour is what is asserted.
//! The two invocations are reported as the dump mode (no argument) and the
//! print mode (`--print` alone).
//!
//! `--print` is the one flag, and only as the first argument: any other
//! word, one that looks like a flag (`--x`, `--help`, `-p`, `--printx`,
//! `--PRINT`, `--`) included, is the name of a file, and `fibref read
//! --x FILE` says `unreadable` for `--x` and reads FILE (status 2).
//! [`check_flag_like_names`] runs the tool so, in both modes, with each such
//! word first and with `--print` as a later argument.

use std::path::{Path, PathBuf};

use super::check::{check_files, Failure};
use super::compare::{compare, Difference, Outcome};
use super::tool::{run_tool, Mode, Tool};

/// The status of a tool that was given nothing to read.
pub const USAGE_STATUS: i32 = 2;

/// What the tool is run as, in words, for the failure that names it.
fn invocation(mode: Mode) -> &'static str {
    match mode {
        Mode::Dump => "the tool with no file",
        Mode::Print => "the tool with --print alone",
    }
}

/// Runs the tool with no file and with `--print` alone, and returns one
/// failure for each run that did not print nothing to standard output,
/// something to standard error, and end with status [`USAGE_STATUS`].
pub fn check_usage(tool: &Tool) -> Vec<Failure> {
    let expected = Outcome {
        stdout: Vec::new(),
        status: USAGE_STATUS,
    };
    Mode::ALL
        .iter()
        .filter_map(|&mode| {
            let run = run_tool(tool, mode, &[]);
            let mut differences = compare(&expected, &run.outcome);
            if run.stderr.trim().is_empty() {
                differences.push(Difference::NoUsageLine);
            }
            (!differences.is_empty()).then(|| Failure {
                mode,
                subject: invocation(mode).to_string(),
                differences,
                stderr: run.stderr,
                input: None,
            })
        })
        .collect()
}

/// Words that look like flags and are file names: none is `--print`.
const FLAG_LIKE: [&str; 6] = ["--x", "--help", "-p", "--printx", "--PRINT", "--"];

/// How many runs [`check_flag_like_names`] makes: each word first and
/// `--print` after a file, in each mode.
pub fn flag_like_runs() -> usize {
    Mode::ALL.len() * (FLAG_LIKE.len() + 1)
}

/// Runs the tool with each of [`FLAG_LIKE`] first and a readable file
/// after it, and with `--print` after the file, in each mode, and returns
/// a failure for each run whose output or status is not what `fibref read`
/// prints (the oracle, which reads each word as a path): the word
/// `unreadable`, the file's text and status 2. A tool that takes any word
/// that starts with `--` for the flag drops the word and the line.
pub fn check_flag_like_names(tool: &Tool, dir: &Path) -> Vec<Failure> {
    std::fs::create_dir_all(dir).expect("the scratch directory is writable");
    let ok = dir.join("flag-like.fib");
    std::fs::write(&ok, "(a 1)\n").expect("writable");
    let mut failures = Vec::new();
    for mode in Mode::ALL {
        for word in FLAG_LIKE {
            failures.extend(check_files(tool, mode, &[PathBuf::from(word), ok.clone()]));
        }
        failures.extend(check_files(
            tool,
            mode,
            &[ok.clone(), PathBuf::from("--print")],
        ));
    }
    failures
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tmp::TempDir;

    /// A tool that is the shell script `body`.
    fn script(dir: &std::path::Path, name: &str, body: &str) -> Tool {
        let path = dir.join(name);
        std::fs::write(&path, body).expect("writable");
        Tool::script(&path)
    }

    const FAITHFUL: &str = "echo 'usage: read [--print] FILE..' >&2\nexit 2\n";

    #[test]
    fn a_tool_that_prints_usage_to_stderr_and_exits_2_passes_in_both_runs() {
        let dir = TempDir::new("usage-faithful");
        let failures = check_usage(&script(dir.path(), "faithful.sh", FAITHFUL));
        assert!(failures.is_empty(), "{}", failures[0]);
    }

    #[test]
    fn usage_on_standard_output_is_reported_in_both_runs() {
        let dir = TempDir::new("usage-stdout");
        let tool = script(
            dir.path(),
            "stdout.sh",
            "echo 'usage: read FILE..'\nexit 2\n",
        );
        let failures = check_usage(&tool);
        assert_eq!(failures.len(), 2);
        for failure in &failures {
            assert!(
                matches!(
                    failure.differences[..],
                    [
                        Difference::Line {
                            number: 1,
                            expected: None,
                            actual: Some(_),
                            ..
                        },
                        Difference::NoUsageLine
                    ]
                ),
                "{failure}"
            );
        }
    }

    #[test]
    fn a_status_that_is_not_2_is_reported() {
        let dir = TempDir::new("usage-status");
        for status in [0, 1, 3] {
            let body = format!("echo usage >&2\nexit {status}\n");
            let failures = check_usage(&script(dir.path(), "status.sh", &body));
            assert_eq!(failures.len(), 2, "status {status}");
            assert!(failures.iter().all(|f| matches!(
                f.differences[..],
                [Difference::Status { expected: 2, actual }] if actual == status
            )));
        }
    }

    #[test]
    fn a_tool_that_says_nothing_on_standard_error_is_reported() {
        let dir = TempDir::new("usage-silent");
        for body in ["exit 2\n", "echo '   ' >&2\nexit 2\n"] {
            let failures = check_usage(&script(dir.path(), "silent.sh", body));
            assert_eq!(failures.len(), 2, "{body:?}");
            assert!(failures
                .iter()
                .all(|f| f.differences == [Difference::NoUsageLine]));
        }
    }

    #[test]
    fn a_tool_that_reads_nothing_and_succeeds_is_reported() {
        let failures = check_usage(&Tool::program(std::path::Path::new("true")));
        assert_eq!(failures.len(), 2);
        assert!(failures.iter().all(|f| f.differences
            == [
                Difference::Status {
                    expected: 2,
                    actual: 0
                },
                Difference::NoUsageLine
            ]));
    }

    #[test]
    fn a_tool_that_only_mishandles_the_flag_is_reported_in_print_mode_alone() {
        let dir = TempDir::new("usage-flag");
        let body = format!("if [ \"$1\" = --print ]; then exit 0; fi\n{FAITHFUL}");
        let failures = check_usage(&script(dir.path(), "flag.sh", &body));
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].mode, Mode::Print);
        let text = failures[0].to_string();
        assert!(
            text.starts_with("DIFFERENT in print mode: the tool with --print alone\n"),
            "{text}"
        );
        assert!(text.contains("exit status: rust reader 2, fibber reader 0"));
        assert!(text.contains("standard error is empty"), "{text}");
    }

    #[test]
    fn a_tool_that_mishandles_no_file_but_not_the_flag_is_reported_in_dump_mode_alone() {
        let dir = TempDir::new("usage-noflag");
        let body = format!("if [ \"$#\" -eq 0 ]; then exit 0; fi\n{FAITHFUL}");
        let failures = check_usage(&script(dir.path(), "noflag.sh", &body));
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].mode, Mode::Dump);
        assert_eq!(failures[0].subject, "the tool with no file");
    }

    #[test]
    fn a_crash_and_a_tool_that_cannot_start_are_reported() {
        let dir = TempDir::new("usage-broken");
        let crash = script(dir.path(), "crash.sh", "echo boom >&2\nkill -ABRT $$\n");
        let failures = check_usage(&crash);
        assert_eq!(failures.len(), 2);
        assert!(failures
            .iter()
            .all(|f| matches!(f.differences[..], [Difference::Status { actual: 134, .. }])));
        let missing = check_usage(&Tool::program(&dir.path().join("no-such-tool")));
        assert_eq!(missing.len(), 2);
    }

    /// A tool that reads every word as a file name: the oracle's own text,
    /// made by the shell from the status of `cat`; a stand-in for the
    /// faithful tool that needs no reader.
    const NAMES_ARE_FILES: &str = "\
here=$(dirname \"$0\")
mode=dump
if [ \"$1\" = \"--print\" ]; then mode=print; shift; fi
status=0
for f in \"$@\"; do
  echo \"== $f\"
  if [ -f \"$f\" ]; then cat \"$here/ok.$mode\"; else echo unreadable; status=2; fi
done
exit $status
";

    /// The flag-like check against a stand-in script `body`, which sees
    /// `ok.dump` and `ok.print` (the oracle's text of the one input).
    fn flag_like_failures(dir: &TempDir, body: &str) -> Vec<Failure> {
        let ok = dir.path().join("flags").join("flag-like.fib");
        let text = "(a 1)\n";
        std::fs::create_dir_all(ok.parent().expect("a directory")).expect("writable");
        std::fs::write(&ok, text).expect("writable");
        for mode in Mode::ALL {
            let section = crate::tool::oracle(std::slice::from_ref(&ok), mode);
            let after_header = section.stdout.splitn(2, |b| *b == b'\n').nth(1);
            std::fs::write(
                dir.path().join(format!("ok.{mode}")),
                after_header.expect("a header line"),
            )
            .expect("writable");
        }
        let tool = script(dir.path(), "flags.sh", body);
        check_flag_like_names(&tool, &dir.path().join("flags"))
    }

    #[test]
    fn a_tool_that_reads_every_word_but_the_first_print_as_a_file_passes() {
        let dir = TempDir::new("usage-flags-faithful");
        let failures = flag_like_failures(&dir, NAMES_ARE_FILES);
        assert!(failures.is_empty(), "{}", failures[0]);
    }

    #[test]
    fn a_tool_that_takes_any_first_word_starting_with_dashes_for_the_flag_is_reported() {
        let dir = TempDir::new("usage-flags-dashes");
        // `--x FILE` is read as the flag and FILE: the line that names
        // `--x` is missing.
        let body = NAMES_ARE_FILES.replace(
            "if [ \"$1\" = \"--print\" ]; then mode=print; shift; fi",
            "case \"$1\" in --*) mode=print; shift ;; esac",
        );
        let failures = flag_like_failures(&dir, &body);
        assert!(!failures.is_empty());
        assert!(
            failures.iter().all(|f| f.mode == Mode::Dump),
            "{}",
            failures[0]
        );
        let text: String = failures.iter().map(|f| f.to_string()).collect();
        assert!(text.contains("== --x"), "{text}");
    }

    #[test]
    fn a_tool_that_treats_print_after_the_file_as_the_flag_is_reported() {
        let dir = TempDir::new("usage-flags-late");
        let body = NAMES_ARE_FILES.replace(
            "for f in",
            "for a in \"$@\"; do if [ \"$a\" = --print ]; then mode=print; fi; done\nfor f in",
        );
        let failures = flag_like_failures(&dir, &body);
        assert!(!failures.is_empty());
        let text: String = failures.iter().map(|f| f.to_string()).collect();
        assert!(text.contains("== --print"), "{text}");
    }
}
