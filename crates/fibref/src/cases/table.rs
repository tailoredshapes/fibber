//! A plain-text rendering of a [`Report`]: one row per case, the counts,
//! and a prominent line when anything is Pending, because Pending is
//! not a pass and must not hide in a green run.

use std::fmt::Write;

use super::runner::{Counts, Report};

/// Folds a possibly multi-line detail onto one line, so a compiler
/// message or a pending reason spanning several lines cannot look like
/// extra rows or shift the row count. Lines are trimmed and joined with
/// ` | `; blank lines are dropped.
fn one_line(detail: &str) -> String {
    detail
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" | ")
}

/// Renders the report as a table followed by the counts: exactly one
/// row per case, whatever the detail or the file name contains (Linux
/// allows a newline in a file name; it is folded like a detail).
pub fn render(report: &Report) -> String {
    let rows: Vec<(String, &str, String)> = report
        .results
        .iter()
        .map(|r| {
            (
                one_line(&r.name()),
                r.status.label(),
                one_line(&r.status.detail()),
            )
        })
        .collect();
    let name_width = rows
        .iter()
        .map(|r| r.0.len())
        .max()
        .unwrap_or(0)
        .max("case".len());
    let label_width = rows
        .iter()
        .map(|r| r.1.len())
        .max()
        .unwrap_or(0)
        .max("status".len());
    let mut out = String::new();
    // Writing to a String cannot fail, so the results of writeln! are ignored.
    let _ = writeln!(
        out,
        "{:name_width$}  {:label_width$}  detail",
        "case", "status"
    );
    for (name, label, detail) in &rows {
        // A row with no detail ends after the label; no trailing spaces.
        let row = format!("{name:name_width$}  {label:label_width$}  {detail}");
        out.push_str(row.trim_end());
        out.push('\n');
    }
    out.push_str(&render_counts(&report.counts));
    out
}

/// The summary lines under the table.
fn render_counts(counts: &Counts) -> String {
    let mut out = format!(
        "\n{} cases: {} pass, {} fail, {} pending, {} header error",
        counts.total(),
        counts.pass,
        counts.fail,
        counts.pending,
        counts.header_error
    );
    // The open count is shown only when there is one, so the line of a
    // suite with no open case stays what it has always been.
    if counts.open > 0 {
        let _ = write!(out, ", {} open", counts.open);
    }
    out.push('\n');
    if counts.open > 0 {
        let _ = writeln!(
            out,
            "OPEN: {} of {} cases fail as their `open` label says; an open case is not a pass.",
            counts.open,
            counts.total()
        );
    }
    if counts.pending > 0 {
        let _ = writeln!(
            out,
            "PENDING: {} of {} cases did not run; pending is not a pass.",
            counts.pending,
            counts.total()
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cases::header::{HeaderError, HeaderErrorKind};
    use crate::cases::runner::CaseResult;
    use crate::cases::verdict::Status;
    use std::path::PathBuf;

    fn result(name: &str, status: Status) -> CaseResult {
        CaseResult {
            path: PathBuf::from("cases").join(name),
            status,
        }
    }

    #[test]
    fn renders_rows_counts_and_pending_warning() {
        let report = Report::from_results(vec![
            result("01-a.fib", Status::Pass),
            result(
                "02-longer-name.fib",
                Status::Fail("result: expected 1, got 2".into()),
            ),
            result("03-p.fib", Status::Pending("no interpreter yet".into())),
            result(
                "04-h.fib",
                Status::HeaderError(HeaderError {
                    path: PathBuf::from("cases/04-h.fib"),
                    line: 2,
                    kind: HeaderErrorKind::MissingKey("expect"),
                }),
            ),
        ]);
        let text = render(&report);
        let expected = "\
case                status   detail
01-a.fib            pass
02-longer-name.fib  FAIL     result: expected 1, got 2
03-p.fib            PENDING  no interpreter yet
04-h.fib            HEADER   line 2: missing required header key `expect`

4 cases: 1 pass, 1 fail, 1 pending, 1 header error
PENDING: 1 of 4 cases did not run; pending is not a pass.
";
        assert_eq!(text, expected);
    }

    #[test]
    fn open_rows_show_their_items_and_the_counts_line_names_them() {
        let report = Report::from_results(vec![
            result("01-a.fib", Status::Pass),
            result(
                "900-open-x.fib",
                Status::Open("L20: cannot unify (Option i64) with bool".into()),
            ),
            result("901-open-y.fib", Status::OpenPassed),
        ]);
        let expected = "\
case            status  detail
01-a.fib        pass
900-open-x.fib  OPEN    L20: cannot unify (Option i64) with bool
901-open-y.fib  FAIL    the item landed: remove `open`

3 cases: 1 pass, 1 fail, 0 pending, 0 header error, 1 open
OPEN: 1 of 3 cases fail as their `open` label says; an open case is not a pass.
";
        assert_eq!(render(&report), expected);
    }

    #[test]
    fn no_pending_line_when_nothing_is_pending() {
        let report = Report::from_results(vec![result("01-a.fib", Status::Pass)]);
        let text = render(&report);
        assert!(!text.contains("PENDING"), "{text}");
        assert!(
            text.ends_with("1 cases: 1 pass, 0 fail, 0 pending, 0 header error\n"),
            "{text}"
        );
    }

    #[test]
    fn empty_report_renders_header_and_zero_counts() {
        let text = render(&Report::default());
        assert_eq!(
            text,
            "case  status  detail\n\n0 cases: 0 pass, 0 fail, 0 pending, 0 header error\n"
        );
    }

    #[test]
    fn multi_line_details_stay_on_one_row() {
        let report = Report::from_results(vec![
            result(
                "01.fib",
                Status::Fail("expected accept, but rejected: boom\n  (conj [] 1)\n  ^".into()),
            ),
            result(
                "02.fib",
                Status::Pending("not yet:\r\nweak refs\n\n".into()),
            ),
            result("03.fib", Status::Pass),
        ]);
        let text = render(&report);
        let table: Vec<&str> = text.split("\n\n").next().unwrap_or("").lines().collect();
        assert_eq!(table.len(), 4, "heading + 3 rows:\n{text}");
        assert_eq!(
            table[1],
            "01.fib  FAIL     expected accept, but rejected: boom | (conj [] 1) | ^"
        );
        assert_eq!(table[2], "02.fib  PENDING  not yet: | weak refs");
        assert!(table[3].starts_with("03.fib"), "{text}");
    }

    #[test]
    fn a_newline_in_the_name_stays_on_one_row() {
        let report = Report::from_results(vec![
            result("01\nsplit.fib", Status::Pass),
            result("02.fib", Status::Pass),
        ]);
        let text = render(&report);
        let table: Vec<&str> = text.split("\n\n").next().unwrap_or("").lines().collect();
        assert_eq!(table.len(), 3, "heading + 2 rows:\n{text}");
        assert_eq!(table[1], "01 | split.fib  pass");
        assert_eq!(table[2], "02.fib          pass");
    }

    #[test]
    fn one_line_folds_and_drops_blank_lines() {
        assert_eq!(one_line(""), "");
        assert_eq!(one_line("single"), "single");
        assert_eq!(one_line("a\n\n  b  \r\nc\n"), "a | b | c");
    }
}
