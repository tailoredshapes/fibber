//! `run_dir`, `run_case` and `Report` over temporary directories: a mix
//! of verdicts, file-name ordering, what is and is not a case file, and
//! that nothing panics on unreadable input.

use std::path::Path;

use fibref::cases::{
    list_cases, list_cases_recursive, render, run_case, run_dir, Counts, Evaluator,
    HeaderErrorKind, PendingEvaluator, Report, Status,
};

use super::support::{accept_header, reject_header, Scripted, TempDir};

fn names(report: &Report) -> Vec<String> {
    report.results.iter().map(|r| r.name()).collect()
}

fn status_of<'a>(report: &'a Report, name: &str) -> &'a Status {
    &report
        .results
        .iter()
        .find(|r| r.name() == name)
        .unwrap_or_else(|| panic!("no result for {name} in {:?}", names(report)))
        .status
}

/// A directory with one case of every status, written out of order.
fn mixed_dir() -> TempDir {
    let dir = TempDir::new("mixed");
    dir.write(
        "05-fail-result.fib",
        &format!("{}(scripted-compiled 2)\n", accept_header(1)),
    );
    dir.write(
        "01-pass.fib",
        &format!("{}(scripted-compiled 1)\n", accept_header(1)),
    );
    dir.write(
        "04-header.fib",
        ";; spec: §4\n;; expect: accept\n;; result: x\n;; audit: clean\n",
    );
    dir.write(
        "03-pending.fib",
        &format!("{}(nothing scripted here)\n", accept_header(1)),
    );
    dir.write(
        "02-pass-reject.fib",
        &format!(
            "{}(scripted-reject error: binding passed twice to bar)\n",
            reject_header("passed twice")
        ),
    );
    dir.write(
        "06-fail-reject-compiled.fib",
        &format!("{}(scripted-compiled 1)\n", reject_header("passed twice")),
    );
    dir
}

#[test]
fn run_dir_counts_every_status_and_orders_by_file_name() {
    let dir = mixed_dir();
    let report = run_dir(dir.path(), &Scripted).expect("directory is readable");
    assert_eq!(
        names(&report),
        vec![
            "01-pass.fib",
            "02-pass-reject.fib",
            "03-pending.fib",
            "04-header.fib",
            "05-fail-result.fib",
            "06-fail-reject-compiled.fib",
        ]
    );
    assert_eq!(
        report.counts,
        Counts {
            pass: 2,
            fail: 2,
            pending: 1,
            header_error: 1,
            open: 0,
        }
    );
    assert_eq!(report.counts.total(), 6);
    assert!(
        !report.ok(),
        "a Fail or HeaderError must make the report not ok"
    );
    assert_eq!(status_of(&report, "01-pass.fib"), &Status::Pass);
    assert_eq!(status_of(&report, "02-pass-reject.fib"), &Status::Pass);
    assert!(matches!(
        status_of(&report, "03-pending.fib"),
        Status::Pending(_)
    ));
    assert!(matches!(
        status_of(&report, "05-fail-result.fib"),
        Status::Fail(_)
    ));
    assert!(matches!(
        status_of(&report, "06-fail-reject-compiled.fib"),
        Status::Fail(_)
    ));
}

#[test]
fn run_dir_header_error_carries_the_case_path_and_line() {
    let dir = mixed_dir();
    let report = run_dir(dir.path(), &Scripted).unwrap();
    match status_of(&report, "04-header.fib") {
        Status::HeaderError(e) => {
            assert_eq!(e.path, dir.path().join("04-header.fib"));
            assert_eq!(e.line, 3);
            assert!(
                matches!(e.kind, HeaderErrorKind::BadValue { key: "result", .. }),
                "{:?}",
                e.kind
            );
        }
        other => panic!("expected HeaderError, got {other:?}"),
    }
}

#[test]
fn run_dir_result_paths_are_inside_the_directory() {
    let dir = mixed_dir();
    let report = run_dir(dir.path(), &Scripted).unwrap();
    for result in &report.results {
        assert!(
            result.path.starts_with(dir.path()),
            "{} is not under {}",
            result.path.display(),
            dir.path().display()
        );
    }
}

#[test]
fn run_dir_ignores_non_case_files_and_subdirectories() {
    let dir = TempDir::new("ignore");
    dir.write("README.md", "# not a case\n");
    dir.write("notes.txt", ";; spec: x\n");
    dir.write("case.fib.bak", &accept_header(1));
    dir.write(
        "sub/nested.fib",
        &format!("{}(scripted-compiled 1)\n", accept_header(1)),
    );
    // A directory whose name ends in .fib is not a case file either.
    dir.write("dir.fib/inner.fib", &accept_header(1));
    dir.write(
        "only.fib",
        &format!("{}(scripted-compiled 1)\n", accept_header(1)),
    );
    let report = run_dir(dir.path(), &Scripted).unwrap();
    assert_eq!(names(&report), vec!["only.fib"]);
    assert_eq!(report.counts.total(), 1);
}

#[test]
fn run_dir_on_an_empty_directory_has_zero_counts_and_is_not_ok() {
    let dir = TempDir::new("empty");
    let report = run_dir(dir.path(), &Scripted).unwrap();
    assert!(report.results.is_empty());
    assert_eq!(report.counts, Counts::default());
    assert!(!report.ok(), "a run over zero cases must not read as green");
}

#[test]
fn run_dir_on_a_missing_directory_is_an_error_not_a_panic() {
    let dir = TempDir::new("missing-parent");
    let missing = dir.path().join("nope");
    assert!(run_dir(&missing, &Scripted).is_err());
}

#[test]
fn run_dir_with_pending_evaluator_is_ok_but_all_pending() {
    let dir = TempDir::new("pending-only");
    dir.write("a.fib", &accept_header(1));
    dir.write("b.fib", &reject_header("x"));
    let report = run_dir(dir.path(), &PendingEvaluator).unwrap();
    assert_eq!(
        report.counts,
        Counts {
            pass: 0,
            fail: 0,
            pending: 2,
            header_error: 0,
            open: 0,
        }
    );
    assert!(report.ok(), "pending alone must not fail the report");
    let text = render(&report);
    assert!(
        text.contains("PENDING"),
        "pending must be prominent:\n{text}"
    );
    assert!(
        text.contains("2 pending"),
        "counts must show pending:\n{text}"
    );
    assert!(
        text.contains("0 pass"),
        "pending must never be counted as pass:\n{text}"
    );
}

#[test]
fn a_single_header_error_makes_the_report_not_ok() {
    let dir = TempDir::new("one-header-error");
    dir.write(
        "a.fib",
        &format!("{}(scripted-compiled 1)\n", accept_header(1)),
    );
    dir.write("b.fib", "(defun main () 1)\n");
    let report = run_dir(dir.path(), &Scripted).unwrap();
    assert_eq!(report.counts.pass, 1);
    assert_eq!(report.counts.header_error, 1);
    assert!(!report.ok());
}

#[test]
fn a_single_fail_makes_the_report_not_ok() {
    let dir = TempDir::new("one-fail");
    dir.write(
        "a.fib",
        &format!("{}(scripted-compiled 1)\n", accept_header(1)),
    );
    dir.write(
        "b.fib",
        &format!("{}(scripted-compiled 2)\n", accept_header(1)),
    );
    let report = run_dir(dir.path(), &Scripted).unwrap();
    assert_eq!((report.counts.pass, report.counts.fail), (1, 1));
    assert!(!report.ok());
}

#[test]
fn file_name_order_is_lexicographic_not_numeric() {
    // The real cases are zero-padded so that the two agree; the contract
    // says "file-name order", which is lexicographic.
    let dir = TempDir::new("lexicographic");
    for name in ["10-b.fib", "2-a.fib", "1-c.fib"] {
        dir.write(name, &accept_header(1));
    }
    let report = run_dir(dir.path(), &PendingEvaluator).unwrap();
    assert_eq!(names(&report), vec!["1-c.fib", "10-b.fib", "2-a.fib"]);
    let listed: Vec<String> = list_cases(dir.path())
        .unwrap()
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(listed, vec!["1-c.fib", "10-b.fib", "2-a.fib"]);
}

#[test]
fn table_rows_follow_file_name_order() {
    let dir = mixed_dir();
    let report = run_dir(dir.path(), &Scripted).unwrap();
    let text = render(&report);
    let positions: Vec<usize> = names(&report)
        .iter()
        .map(|n| {
            text.find(n.as_str())
                .unwrap_or_else(|| panic!("{n} missing from\n{text}"))
        })
        .collect();
    let mut sorted = positions.clone();
    sorted.sort_unstable();
    assert_eq!(positions, sorted, "rows out of order:\n{text}");
}

#[test]
fn run_case_on_a_missing_file_is_a_header_error_not_a_panic() {
    let dir = TempDir::new("run-case-missing");
    let path = dir.path().join("missing.fib");
    let result = run_case(&path, &Scripted);
    assert_eq!(result.path, path);
    match result.status {
        Status::HeaderError(e) => {
            assert!(
                matches!(e.kind, HeaderErrorKind::Unreadable(_)),
                "{:?}",
                e.kind
            );
            assert_eq!(e.path, path);
        }
        other => panic!("expected HeaderError, got {other:?}"),
    }
}

#[test]
fn run_case_on_invalid_utf8_is_a_header_error_not_a_panic() {
    let dir = TempDir::new("run-case-utf8");
    let path = dir.write_bytes(
        "bad.fib",
        b";; spec: \xff\n;; expect: reject\n;; error: x\n",
    );
    let result = run_case(&path, &Scripted);
    assert!(
        matches!(result.status, Status::HeaderError(_)),
        "{:?}",
        result.status
    );
}

#[test]
fn run_case_hands_the_whole_source_to_the_evaluator() {
    // The directive sits after the header; the evaluator must see it.
    let dir = TempDir::new("run-case-source");
    let path = dir.write(
        "a.fib",
        &format!("{};; prose\n\n(scripted-compiled 3)\n", accept_header(3)),
    );
    assert_eq!(run_case(&path, &Scripted).status, Status::Pass);
}

#[test]
fn run_case_with_a_bad_header_does_not_run_the_evaluator() {
    struct Panicking;
    impl Evaluator for Panicking {
        fn run(&self, _: &str) -> fibref::cases::Outcome {
            panic!("evaluator must not run when the header is bad");
        }
    }
    let dir = TempDir::new("run-case-no-eval");
    let path = dir.write("a.fib", ";; spec: x\n;; expect: accept\n");
    let result = run_case(&path, &Panicking);
    assert!(matches!(result.status, Status::HeaderError(_)));
}

#[test]
fn list_cases_recursive_finds_nested_cases_sorted() {
    let dir = TempDir::new("recursive");
    dir.write("z/2.fib", "");
    dir.write("a/1.fib", "");
    dir.write("a/b/0.fib", "");
    dir.write("top.fib", "");
    dir.write("a/skip.txt", "");
    let found: Vec<String> = list_cases_recursive(dir.path())
        .unwrap()
        .iter()
        .map(|p| {
            p.strip_prefix(dir.path())
                .unwrap()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    assert_eq!(found, vec!["a/1.fib", "a/b/0.fib", "top.fib", "z/2.fib"]);
}

#[test]
fn pending_evaluator_answers_unsupported_no_interpreter_yet() {
    let outcome = PendingEvaluator.run("(defun main () -> i64 1)");
    assert_eq!(
        outcome,
        fibref::cases::Outcome::Unsupported {
            reason: "no interpreter yet".to_string()
        }
    );
}

#[test]
fn report_from_no_results_has_zero_counts_and_is_not_ok() {
    let report = Report::from_results(vec![]);
    assert_eq!(report.counts, Counts::default());
    assert!(!report.ok(), "zero cases must never read as a green run");
}

#[test]
fn case_result_name_is_the_file_name_only() {
    let dir = TempDir::new("name");
    let path = dir.write("deep/er/x.fib", &accept_header(1));
    let result = run_case(Path::new(&path), &PendingEvaluator);
    assert_eq!(result.name(), "x.fib");
}
