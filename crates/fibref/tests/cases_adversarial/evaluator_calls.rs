//! What the runner hands the evaluator: the file's bytes unchanged
//! (header comments and CRLF included), exactly once per parseable
//! case, in file-name order, and never for a case whose header is bad.

use std::cell::RefCell;
use std::path::Path;

use fibref::cases::{run_case, run_dir, Evaluator, Outcome, PendingEvaluator, Status};

use super::support::{accept_header, reject_header, TempDir};

/// Records every source it is given and answers Unsupported.
#[derive(Default)]
struct Recording {
    sources: RefCell<Vec<String>>,
}

impl Evaluator for Recording {
    fn run(&self, source: &str) -> Outcome {
        self.sources.borrow_mut().push(source.to_string());
        Outcome::Unsupported {
            reason: "recording".to_string(),
        }
    }
}

#[test]
fn the_evaluator_receives_the_file_unchanged_including_header_and_crlf() {
    let dir = TempDir::new("eval-source");
    let content = ";; spec: §4\r\n;; expect: accept\r\n;; result: 1\r\n;; audit: clean\r\n;; prose\r\n\r\n(defun main () -> i64 1)\r\n";
    let path = dir.write("a.fib", content);
    let recording = Recording::default();
    run_case(&path, &recording);
    assert_eq!(recording.sources.borrow().as_slice(), [content.to_string()]);
}

#[test]
fn the_evaluator_receives_a_file_without_a_trailing_newline_unchanged() {
    let dir = TempDir::new("eval-no-newline");
    let content = ";; spec: §5\n;; expect: reject\n;; error: x\n(defun main () 1)";
    let path = dir.write("a.fib", content);
    let recording = Recording::default();
    run_case(&path, &recording);
    assert_eq!(recording.sources.borrow().as_slice(), [content.to_string()]);
}

#[test]
fn the_evaluator_runs_once_per_parseable_case_in_file_name_order() {
    let dir = TempDir::new("eval-order");
    let c = format!("{}(c)\n", accept_header(3));
    let a = format!("{}(a)\n", accept_header(1));
    let b = format!("{}(b)\n", reject_header("x"));
    dir.write("03-c.fib", &c);
    dir.write("01-a.fib", &a);
    dir.write("02-bad-header.fib", ";; spec: x\n;; expect: accept\n");
    dir.write("02-b.fib", &b);
    dir.write("ignored.txt", &a);
    let recording = Recording::default();
    let report = run_dir(dir.path(), &recording).expect("readable");
    assert_eq!(recording.sources.borrow().as_slice(), [a, b, c]);
    assert_eq!(report.counts.pending, 3);
    assert_eq!(report.counts.header_error, 1);
}

#[test]
fn a_bad_header_never_reaches_the_evaluator_even_when_the_code_is_fine() {
    let dir = TempDir::new("eval-bad-header");
    dir.write(
        "a.fib",
        ";; spec: x\n;; expect: accept\n;; result: 1\n;; audit: clean\n;; error: y\n(defun main () 1)\n",
    );
    let recording = Recording::default();
    let report = run_dir(dir.path(), &recording).expect("readable");
    assert!(recording.sources.borrow().is_empty());
    assert_eq!(report.counts.header_error, 1);
}

#[test]
fn an_empty_fib_file_is_a_header_error_at_line_1_and_is_not_evaluated() {
    let dir = TempDir::new("eval-empty-file");
    dir.write("empty.fib", "");
    let recording = Recording::default();
    let report = run_dir(dir.path(), &recording).expect("readable");
    assert!(recording.sources.borrow().is_empty());
    match &report.results[0].status {
        Status::HeaderError(e) => {
            assert_eq!(e.line, 1);
            assert_eq!(e.path, dir.path().join("empty.fib"));
        }
        other => panic!("expected HeaderError, got {other:?}"),
    }
}

#[test]
fn a_header_error_result_carries_the_same_path_as_the_result() {
    let dir = TempDir::new("eval-paths");
    dir.write("bad.fib", ";; spec: x\n");
    let report = run_dir(dir.path(), &PendingEvaluator).expect("readable");
    let result = &report.results[0];
    assert_eq!(result.path, dir.path().join("bad.fib"));
    match &result.status {
        Status::HeaderError(e) => assert_eq!(e.path, result.path),
        other => panic!("expected HeaderError, got {other:?}"),
    }
}

#[test]
fn results_len_equals_counts_total() {
    let dir = TempDir::new("eval-total");
    for (i, header) in [
        accept_header(1),
        reject_header("x"),
        ";; spec: x\n".to_string(),
    ]
    .iter()
    .enumerate()
    {
        dir.write(&format!("{i}.fib"), header);
    }
    let report = run_dir(dir.path(), &PendingEvaluator).expect("readable");
    assert_eq!(report.results.len(), report.counts.total());
    assert_eq!(report.counts.total(), 3);
}

#[test]
fn run_case_on_a_directory_is_a_header_error_not_a_panic() {
    let dir = TempDir::new("eval-dir-as-case");
    dir.write("sub.fib/inner.fib", &accept_header(1));
    let recording = Recording::default();
    let result = run_case(&dir.path().join("sub.fib"), &recording);
    assert!(
        matches!(result.status, Status::HeaderError(_)),
        "{:?}",
        result.status
    );
    assert!(recording.sources.borrow().is_empty());
}

#[test]
fn result_paths_are_exactly_dir_joined_with_the_file_name() {
    let dir = TempDir::new("eval-join");
    dir.write("x.fib", &accept_header(1));
    let report = run_dir(dir.path(), &PendingEvaluator).expect("readable");
    assert_eq!(report.results[0].path, Path::new(dir.path()).join("x.fib"));
}
