//! Runner tests: a scripted evaluator over a temporary directory, and
//! the real case tree, whose headers must all parse.

use super::*;
use crate::cases::evaluator::{AuditSummary, Outcome, PendingEvaluator, Value};
use std::fs;

/// Answers from a directive in the source: `(return N)` compiles to `N`
/// with a clean audit, `(reject-me)` is rejected, anything else is
/// unsupported.
struct Scripted;

impl Evaluator for Scripted {
    fn run(&self, source: &str) -> Outcome {
        if source.contains("(reject-me)") {
            return Outcome::Rejected {
                message: "rejected: reject-me".to_string(),
            };
        }
        match source.find("(return ") {
            Some(at) => {
                let rest = &source[at + "(return ".len()..];
                let digits: String = rest
                    .chars()
                    .take_while(|c| *c == '-' || c.is_ascii_digit())
                    .collect();
                Outcome::Compiled {
                    result: Value::Int(digits.parse().unwrap()),
                    audit: AuditSummary::clean(),
                }
            }
            None => Outcome::Unsupported {
                reason: "no directive".to_string(),
            },
        }
    }
}

/// A fresh directory under the system temp dir, removed on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("fibref-runner-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        TempDir(path)
    }

    fn write(&self, name: &str, contents: &str) -> PathBuf {
        let path = self.0.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, contents).unwrap();
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

const ACCEPT_3: &str = ";; spec: §4\n;; expect: accept\n;; result: 3\n;; audit: clean\n";
const REJECT: &str = ";; spec: §5\n;; expect: reject\n;; error: reject-me\n";

fn names(report: &Report) -> Vec<String> {
    report.results.iter().map(CaseResult::name).collect()
}

#[test]
fn run_dir_orders_by_file_name_and_counts_every_status() {
    let dir = TempDir::new("mixed");
    dir.write("02-pass.fib", &format!("{ACCEPT_3}(return 3)\n"));
    dir.write("01-fail.fib", &format!("{ACCEPT_3}(return 4)\n"));
    dir.write("03-pending.fib", ACCEPT_3);
    dir.write("04-header.fib", ";; spec: §4\n;; expect: accept\n");
    dir.write("05-reject.fib", &format!("{REJECT}(reject-me)\n"));
    dir.write("notes.md", "not a case");
    let report = run_dir(&dir.0, &Scripted).unwrap();
    assert_eq!(
        names(&report),
        [
            "01-fail.fib",
            "02-pass.fib",
            "03-pending.fib",
            "04-header.fib",
            "05-reject.fib"
        ]
    );
    assert_eq!(
        report.counts,
        Counts {
            pass: 2,
            fail: 1,
            pending: 1,
            header_error: 1,
            open: 0,
        }
    );
    assert_eq!(report.counts.total(), 5);
    assert!(!report.ok());
    assert!(matches!(report.results[0].status, Status::Fail(_)));
    assert_eq!(report.results[1].status, Status::Pass);
    assert!(matches!(report.results[2].status, Status::Pending(_)));
    assert!(matches!(report.results[3].status, Status::HeaderError(_)));
    assert_eq!(report.results[4].status, Status::Pass);
}

#[test]
fn pending_only_report_is_ok_but_has_no_passes() {
    let dir = TempDir::new("pending");
    dir.write("a.fib", ACCEPT_3);
    dir.write("b.fib", REJECT);
    let report = run_dir(&dir.0, &PendingEvaluator).unwrap();
    assert_eq!(report.counts.pending, 2);
    assert_eq!(report.counts.pass, 0);
    assert!(report.ok());
}

#[test]
fn header_error_alone_makes_report_not_ok() {
    let dir = TempDir::new("header");
    dir.write("a.fib", ";; spec: §4\n;; expect: maybe\n");
    let report = run_dir(&dir.0, &PendingEvaluator).unwrap();
    assert_eq!(report.counts.header_error, 1);
    assert!(!report.ok());
}

#[test]
fn empty_directory_is_an_empty_report_that_is_not_ok() {
    let dir = TempDir::new("empty");
    let report = run_dir(&dir.0, &PendingEvaluator).unwrap();
    assert_eq!(report, Report::default());
    assert!(!report.ok(), "zero cases must never read as a green run");
}

#[test]
fn missing_directory_is_an_error_not_a_panic() {
    let result = run_dir(Path::new("/nonexistent/fibref/cases"), &PendingEvaluator);
    assert!(result.is_err());
}

#[test]
fn unreadable_case_is_a_header_error() {
    let result = run_case(Path::new("/nonexistent/case.fib"), &PendingEvaluator);
    match result.status {
        Status::HeaderError(e) => {
            assert_eq!(e.line, 0);
            assert!(matches!(e.kind, HeaderErrorKind::Unreadable(_)));
        }
        other => panic!("expected HeaderError, got {other:?}"),
    }
}

/// A `*.fib` entry that cannot be read is listed and reported, never
/// dropped: a broken checkout must not pass with one case fewer.
#[test]
#[cfg(unix)]
fn dangling_link_named_like_a_case_is_listed_and_is_a_header_error() {
    let dir = TempDir::new("dangling");
    dir.write("01.fib", "");
    dir.write("sub/02.fib", "");
    std::os::unix::fs::symlink(dir.0.join("nowhere.fib"), dir.0.join("03-gone.fib")).unwrap();
    std::os::unix::fs::symlink(dir.0.join("nowhere.fib"), dir.0.join("sub/04-gone.fib")).unwrap();
    assert_eq!(
        list_cases(&dir.0).unwrap(),
        [dir.0.join("01.fib"), dir.0.join("03-gone.fib")]
    );
    assert_eq!(
        list_cases_recursive(&dir.0).unwrap(),
        [
            dir.0.join("01.fib"),
            dir.0.join("03-gone.fib"),
            dir.0.join("sub/02.fib"),
            dir.0.join("sub/04-gone.fib")
        ]
    );
    let report = run_dir(&dir.0, &Scripted).unwrap();
    assert_eq!(report.counts.header_error, 2, "{report:?}");
    assert!(!report.ok());
    assert!(matches!(
        &report.results[1].status,
        Status::HeaderError(HeaderError {
            line: 0,
            kind: HeaderErrorKind::Unreadable(_),
            ..
        })
    ));
}

#[test]
fn recursive_listing_finds_nested_cases_only() {
    let dir = TempDir::new("nested");
    dir.write("b/02.fib", "");
    dir.write("a/01.fib", "");
    dir.write("a/README.md", "");
    dir.write("00.fib", "");
    let found = list_cases_recursive(&dir.0).unwrap();
    let relative: Vec<PathBuf> = found
        .iter()
        .map(|p| p.strip_prefix(&dir.0).unwrap().to_path_buf())
        .collect();
    assert_eq!(
        relative,
        [
            PathBuf::from("00.fib"),
            PathBuf::from("a/01.fib"),
            PathBuf::from("b/02.fib")
        ]
    );
    assert_eq!(list_cases(&dir.0).unwrap(), [dir.0.join("00.fib")]);
}

#[test]
fn a_support_directory_holds_modules_and_no_case() {
    let dir = TempDir::new("support");
    dir.write("01.fib", "");
    dir.write("support/tl/rng.fib", "");
    dir.write("sub/support/x.fib", "");
    dir.write("sub/02.fib", "");
    dir.write("03-prog/main.fib", "");
    dir.write("03-prog/support/y.fib", "");
    let found = list_cases_recursive(&dir.0).unwrap();
    let relative: Vec<PathBuf> = found
        .iter()
        .map(|p| p.strip_prefix(&dir.0).unwrap().to_path_buf())
        .collect();
    assert_eq!(
        relative,
        [
            PathBuf::from("01.fib"),
            PathBuf::from("03-prog/main.fib"),
            PathBuf::from("sub/02.fib")
        ]
    );
}

/// The test CI runs against the real cases: every header under
/// `cases/` must parse. It does not run the cases.
#[test]
fn every_real_case_header_parses() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../cases");
    let files = list_cases_recursive(&root).unwrap();
    assert!(
        files.len() >= 20,
        "found only {} case files under {}",
        files.len(),
        root.display()
    );
    let errors: Vec<String> = files
        .iter()
        .filter_map(|path| crate::cases::header::read_header(path).err())
        .map(|e| e.to_string())
        .collect();
    assert!(errors.is_empty(), "header errors:\n{}", errors.join("\n"));
}

#[test]
fn a_report_with_no_cases_is_not_ok() {
    let report = Report::from_results(Vec::new());
    assert_eq!(report.counts.total(), 0);
    assert!(!report.ok(), "zero cases must never read as a green run");
}

/// [`Scripted`], which also counts: `(allocs N)` in the source is the
/// number of objects the run allocated.
struct Counting;

impl Evaluator for Counting {
    fn run(&self, source: &str) -> Outcome {
        Scripted.run(source)
    }

    fn run_counted(&self, source: &str, path: &Path) -> (Outcome, Option<u64>) {
        let counted = source.find("(allocs ").map(|at| {
            let rest = &source[at + "(allocs ".len()..];
            let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
            digits.parse().unwrap()
        });
        (self.run_at(source, path), counted)
    }
}

const BOUNDED: &str =
    ";; spec: §4\n;; expect: accept\n;; result: 3\n;; audit: clean\n;; allocs: <= 7\n";

#[test]
fn the_runner_hands_the_evaluators_count_to_the_verdict() {
    let dir = TempDir::new("allocs");
    dir.write("1-under.fib", &format!("{BOUNDED}(return 3) (allocs 0)\n"));
    dir.write("2-exact.fib", &format!("{BOUNDED}(return 3) (allocs 7)\n"));
    dir.write("3-over.fib", &format!("{BOUNDED}(return 3) (allocs 8)\n"));
    dir.write("4-none.fib", &format!("{BOUNDED}(return 3)\n"));
    let report = run_dir(&dir.0, &Counting).unwrap();
    let statuses: Vec<String> = report
        .results
        .iter()
        .map(|r| r.status.to_string())
        .collect();
    assert_eq!(statuses[0], "pass", "{statuses:?}");
    assert_eq!(statuses[1], "pass", "{statuses:?}");
    assert_eq!(
        statuses[2],
        "FAIL: allocs: expected at most 7 heap objects, the run allocated 8"
    );
    assert_eq!(
        statuses[3],
        "FAIL: allocs: the header says `<= 7` but the evaluator reported no allocation count"
    );
    assert_eq!(report.counts.fail, 2);
    assert!(!report.ok(), "an exceeded bound fails the run");
}

#[test]
fn an_evaluator_that_does_not_count_fails_a_bounded_case_never_passes_it() {
    let dir = TempDir::new("uncounted");
    dir.write("a.fib", &format!("{BOUNDED}(return 3) (allocs 0)\n"));
    let report = run_dir(&dir.0, &Scripted).unwrap();
    assert_eq!(report.counts.fail, 1, "{report:?}");
    assert_eq!(report.counts.pass, 0);
}

fn only(dir: &TempDir, prefixes: &[&str]) -> Result<Vec<String>, String> {
    let prefixes: Vec<String> = prefixes.iter().map(|p| p.to_string()).collect();
    run_dir_only(&dir.0, &Scripted, &prefixes)
        .map(|report| names(&report))
        .map_err(|e| e.to_string())
}

fn numbered_dir(name: &str) -> TempDir {
    let dir = TempDir::new(name);
    for file in ["200-a.fib", "201-b.fib", "240-c.fib", "241-d.fib"] {
        dir.write(file, &format!("{ACCEPT_3}(return 3)\n"));
    }
    dir.write("250-prog/main.fib", &format!("{ACCEPT_3}(return 3)\n"));
    dir
}

#[test]
fn only_runs_the_cases_with_one_of_the_prefixes_in_file_order() {
    let dir = numbered_dir("only");
    assert_eq!(only(&dir, &["24"]).unwrap(), ["240-c.fib", "241-d.fib"]);
    assert_eq!(
        only(&dir, &["241-", "200"]).unwrap(),
        ["200-a.fib", "241-d.fib"]
    );
    assert_eq!(only(&dir, &["250"]).unwrap(), ["250-prog/main.fib"]);
    assert_eq!(only(&dir, &[]).unwrap().len(), 5, "no prefix is every case");
}

#[test]
fn a_prefix_is_a_prefix_and_not_a_substring() {
    let dir = TempDir::new("substring");
    dir.write("125-b.fib", &format!("{ACCEPT_3}(return 3)\n"));
    dir.write("250-a.fib", &format!("{ACCEPT_3}(return 3)\n"));
    assert_eq!(only(&dir, &["25"]).unwrap(), ["250-a.fib"]);
}

#[test]
fn a_prefix_that_matches_no_case_is_an_error_even_beside_one_that_does() {
    let dir = numbered_dir("nomatch");
    assert_eq!(
        only(&dir, &["999-"]),
        Err("no case matches 999-".to_string())
    );
    assert_eq!(
        only(&dir, &["240", "999-"]),
        Err("no case matches 999-".to_string())
    );
}

#[test]
fn a_prefix_is_matched_against_the_name_not_the_directory_or_the_extension() {
    let dir = numbered_dir("key");
    assert!(only(&dir, &["main"]).is_err());
    assert!(only(&dir, &["fib"]).is_err());
    assert!(only(&dir, &["240-c.fib"]).is_ok());
}

#[test]
fn only_on_a_missing_directory_is_an_io_error() {
    let missing = Path::new("/nonexistent/fibref/cases");
    let got = run_dir_only(missing, &Scripted, &["1".to_string()]);
    assert!(matches!(got, Err(SelectError::Io(_))));
}

const OPEN: &str =
    ";; spec: §5.5\n;; expect: accept\n;; result: 3\n;; audit: clean\n;; open: L20 C9\n";

#[test]
fn an_open_case_is_counted_apart_and_does_not_fail_the_report() {
    let dir = TempDir::new("open");
    dir.write("01-pass.fib", &format!("{ACCEPT_3}(return 3)\n"));
    dir.write("02-open.fib", &format!("{OPEN}(reject-me)\n"));
    let report = run_dir(&dir.0, &Scripted).unwrap();
    assert_eq!((report.counts.pass, report.counts.open), (1, 1));
    assert_eq!((report.counts.fail, report.counts.total()), (0, 2));
    assert!(report.ok(), "an open case is not a failure of the run");
    match &report.results[1].status {
        Status::Open(why) => assert!(why.starts_with("L20 C9: "), "{why}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn an_open_case_that_passes_is_a_failure() {
    let dir = TempDir::new("openpassed");
    dir.write("01-open.fib", &format!("{OPEN}(return 3)\n"));
    let report = run_dir(&dir.0, &Scripted).unwrap();
    assert_eq!(report.results[0].status, Status::OpenPassed);
    assert_eq!((report.counts.fail, report.counts.open), (1, 0));
    assert!(!report.ok(), "the label is out of date: the run fails");
}

#[test]
fn an_open_label_on_a_reject_case_is_a_header_error() {
    let dir = TempDir::new("openreject");
    dir.write("01.fib", &format!("{REJECT};; open: L20\n(reject-me)\n"));
    let report = run_dir(&dir.0, &Scripted).unwrap();
    assert_eq!(report.counts.header_error, 1, "{report:?}");
}
