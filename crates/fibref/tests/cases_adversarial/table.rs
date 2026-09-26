//! The plain-text table: one line per case, every case named, the
//! counts line agreeing with the counts struct, and a Pending line only
//! when something is pending.

use std::path::PathBuf;

use fibref::cases::{
    judge, render, AuditExpect, AuditSummary, CaseResult, Expected, Header, HeaderError,
    HeaderErrorKind, Outcome, Report, Status, Value, Verdict,
};

fn result(name: &str, status: Status) -> CaseResult {
    CaseResult {
        path: PathBuf::from("cases").join(name),
        status,
    }
}

fn header_error(name: &str, line: usize, kind: HeaderErrorKind) -> CaseResult {
    result(
        name,
        Status::HeaderError(HeaderError {
            path: PathBuf::from("cases").join(name),
            line,
            kind,
        }),
    )
}

/// Lines of the table before the blank line that precedes the counts.
fn table_lines(text: &str) -> Vec<&str> {
    text.split("\n\n").next().unwrap_or("").lines().collect()
}

#[test]
fn one_line_per_case_plus_a_heading() {
    let report = Report::from_results(vec![
        result("01.fib", Status::Pass),
        result("02.fib", Status::Fail("result: expected 1, got 2".into())),
        result("03.fib", Status::Pending("no interpreter yet".into())),
        header_error("04.fib", 2, HeaderErrorKind::MissingKey("expect")),
    ]);
    let text = render(&report);
    let lines = table_lines(&text);
    assert_eq!(lines.len(), 5, "heading + 4 rows:\n{text}");
    for (line, name) in lines[1..]
        .iter()
        .zip(["01.fib", "02.fib", "03.fib", "04.fib"])
    {
        assert!(line.starts_with(name), "row {line:?} is not for {name}");
    }
}

#[test]
fn a_multi_line_compiler_message_does_not_break_the_table_into_extra_rows() {
    // Interpretation: a table has one line per case. A compiler that
    // prints a two-line error must not produce a row that looks like two
    // cases, or shift the row count. The status comes from `judge`, as
    // the runner would build it. See the spec questions in the report.
    let header = Header {
        spec: "§4".into(),
        verdict: Verdict::Accept {
            result: Expected::Int(1),
            audit: AuditExpect::Clean,
        },
    };
    let status = judge(
        &header,
        &Outcome::Rejected {
            message: "error at 3:1: unbound symbol conj\n  (conj [] 1)\n  ^".into(),
        },
    );
    assert!(matches!(status, Status::Fail(_)));
    let report = Report::from_results(vec![
        result("01.fib", status),
        result("02.fib", Status::Pass),
    ]);
    let text = render(&report);
    let lines = table_lines(&text);
    assert_eq!(
        lines.len(),
        3,
        "heading + 2 rows expected, got {} lines:\n{text}",
        lines.len()
    );
    assert!(lines[2].starts_with("02.fib"), "{text}");
}

#[test]
fn a_multi_line_pending_reason_does_not_break_the_table_either() {
    let report = Report::from_results(vec![
        result("01.fib", Status::Pending("not yet:\nweak refs".into())),
        result("02.fib", Status::Pass),
    ]);
    let text = render(&report);
    let lines = table_lines(&text);
    assert_eq!(lines.len(), 3, "{text}");
}

#[test]
fn counts_line_agrees_with_the_counts_struct() {
    let report = Report::from_results(vec![
        result("01.fib", Status::Pass),
        result("02.fib", Status::Pass),
        result("03.fib", Status::Fail("x".into())),
        result("04.fib", Status::Pending("p".into())),
        result("05.fib", Status::Pending("p".into())),
        result("06.fib", Status::Pending("p".into())),
        header_error("07.fib", 1, HeaderErrorKind::Empty),
    ]);
    let text = render(&report);
    let c = report.counts;
    let expected = format!(
        "{} cases: {} pass, {} fail, {} pending, {} header error",
        c.total(),
        c.pass,
        c.fail,
        c.pending,
        c.header_error
    );
    assert!(
        text.contains(&expected),
        "expected {expected:?} in:\n{text}"
    );
    assert!(text.contains("7 cases: 2 pass, 1 fail, 3 pending, 1 header error"));
}

#[test]
fn pending_rows_are_not_labelled_pass() {
    let report = Report::from_results(vec![
        result("01.fib", Status::Pending("no interpreter yet".into())),
        result("02.fib", Status::Pending("no interpreter yet".into())),
    ]);
    let text = render(&report);
    for line in table_lines(&text).iter().skip(1) {
        assert!(
            !line.to_ascii_lowercase().contains("pass"),
            "pending row must not say pass: {line}"
        );
        assert!(line.contains("PENDING"), "{line}");
    }
    assert!(text.contains("PENDING: 2 of 2"), "{text}");
}

#[test]
fn header_error_row_names_the_line_and_the_key() {
    let report = Report::from_results(vec![header_error(
        "07-bad.fib",
        3,
        HeaderErrorKind::BadValue {
            key: "result",
            value: "one".into(),
        },
    )]);
    let text = render(&report);
    let row = table_lines(&text)[1];
    assert!(row.starts_with("07-bad.fib"), "{row}");
    assert!(row.contains("line 3"), "{row}");
    assert!(row.contains("result"), "{row}");
    assert!(row.contains("one"), "{row}");
}

#[test]
fn fail_row_carries_the_detail_from_judge() {
    let header = Header {
        spec: "§5".into(),
        verdict: Verdict::Reject {
            error: "cell cannot be shared".into(),
        },
    };
    let status = judge(
        &header,
        &Outcome::Compiled {
            result: Value::Int(9),
            audit: AuditSummary::clean(),
        },
    );
    let report = Report::from_results(vec![result("13.fib", status)]);
    let text = render(&report);
    let row = table_lines(&text)[1];
    assert!(row.contains("FAIL"), "{row}");
    assert!(row.contains("cell cannot be shared"), "{row}");
    assert!(row.contains('9'), "{row}");
}

#[test]
fn rows_have_no_trailing_whitespace() {
    let report = Report::from_results(vec![
        result("01.fib", Status::Pass),
        result("02-longer.fib", Status::Pass),
    ]);
    for line in render(&report).lines() {
        assert_eq!(line, line.trim_end(), "trailing whitespace in {line:?}");
    }
}
