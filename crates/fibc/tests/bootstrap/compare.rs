//! The comparison of two reader outputs (spec/bootstrap.md §3): the text
//! a tool printed and the exit status it ended with, byte for byte.
//!
//! "Expected" is always the Rust reader (the oracle) and "actual" the
//! tool under test. A difference names the first output line that
//! differs, the file whose dump it falls in, and each side's text of that
//! line, so a failure says where to look without a diff tool.

use std::fmt;

/// What one run of a reader tool produced.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Outcome {
    /// Everything the tool wrote to standard output.
    pub stdout: Vec<u8>,
    /// The exit status; a process killed by signal N ends with 128+N and
    /// one that timed out with -1.
    pub status: i32,
}

/// One way two outcomes differ.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Difference {
    /// The first output line (1-based) that is not the same on both
    /// sides; `None` on a side that has no such line.
    Line {
        number: usize,
        file: Option<String>,
        expected: Option<String>,
        actual: Option<String>,
    },
    /// The lines are the same but one output ends without a newline.
    FinalNewline { expected: bool, actual: bool },
    /// The exit statuses differ.
    Status { expected: i32, actual: i32 },
    /// A tool started with no file wrote nothing to standard error; it
    /// should say how it is used there (spec/bootstrap.md §2).
    NoUsageLine,
}

/// An output split into lines, remembering whether the last one ended in
/// a newline (an output with no lines at all counts as ending in one).
struct Lines<'a> {
    lines: Vec<&'a [u8]>,
    final_newline: bool,
}

impl<'a> Lines<'a> {
    fn of(bytes: &'a [u8]) -> Self {
        if bytes.is_empty() {
            return Lines {
                lines: Vec::new(),
                final_newline: true,
            };
        }
        let mut lines: Vec<&[u8]> = bytes.split(|b| *b == b'\n').collect();
        let final_newline = lines.last().is_some_and(|last| last.is_empty());
        if final_newline {
            lines.pop();
        }
        Lines {
            lines,
            final_newline,
        }
    }

    /// The path of the `== FILE` header at or above line `index` (0-based).
    fn file_at(&self, index: usize) -> Option<String> {
        let upto = (index + 1).min(self.lines.len());
        self.lines[..upto]
            .iter()
            .rev()
            .find_map(|l| l.strip_prefix(b"== "))
            .map(|path| String::from_utf8_lossy(path).into_owned())
    }

    fn get(&self, index: usize) -> Option<&'a [u8]> {
        self.lines.get(index).copied()
    }
}

fn show(line: &[u8]) -> String {
    String::from_utf8_lossy(line).into_owned()
}

/// Every way `actual` differs from `expected`; empty when they are the
/// same bytes with the same status.
pub fn compare(expected: &Outcome, actual: &Outcome) -> Vec<Difference> {
    let (e, a) = (Lines::of(&expected.stdout), Lines::of(&actual.stdout));
    let mut found = Vec::new();
    let longest = e.lines.len().max(a.lines.len());
    match (0..longest).find(|i| e.get(*i) != a.get(*i)) {
        Some(i) => found.push(Difference::Line {
            number: i + 1,
            file: e.file_at(i).or_else(|| a.file_at(i)),
            expected: e.get(i).map(show),
            actual: a.get(i).map(show),
        }),
        None if e.final_newline != a.final_newline => found.push(Difference::FinalNewline {
            expected: e.final_newline,
            actual: a.final_newline,
        }),
        None => {}
    }
    if expected.status != actual.status {
        found.push(Difference::Status {
            expected: expected.status,
            actual: actual.status,
        });
    }
    found
}

fn side(line: &Option<String>) -> String {
    match line {
        Some(text) => format!("{text:?}"),
        None => "(no such line)".to_string(),
    }
}

impl fmt::Display for Difference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Difference::Line {
                number,
                file,
                expected,
                actual,
            } => {
                let place = file.as_deref().unwrap_or("before the first header");
                writeln!(f, "output line {number} differs (in the dump of {place})")?;
                writeln!(f, "  rust reader:   {}", side(expected))?;
                write!(f, "  fibber reader: {}", side(actual))
            }
            Difference::FinalNewline { expected, actual } => write!(
                f,
                "the last line ends with a newline: rust reader {expected}, fibber reader {actual}"
            ),
            Difference::Status { expected, actual } => {
                write!(
                    f,
                    "exit status: rust reader {expected}, fibber reader {actual}"
                )
            }
            Difference::NoUsageLine => {
                write!(f, "standard error is empty: a usage line is expected")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outcome(text: &str, status: i32) -> Outcome {
        Outcome {
            stdout: text.as_bytes().to_vec(),
            status,
        }
    }

    const DUMP: &str = "== a.fib\nlist 2 1:1 0..5\n  sym \"a\" 1:2 1..2\n  int 1 i64 1:4 3..4\n\
                        == b.fib\nerror UnexpectedClose 1:1 0..1: unexpected ) with nothing open\n";

    #[test]
    fn identical_outcomes_are_not_reported() {
        assert_eq!(compare(&outcome(DUMP, 1), &outcome(DUMP, 1)), vec![]);
        assert_eq!(compare(&outcome("", 0), &outcome("", 0)), vec![]);
    }

    #[test]
    fn a_digit_of_a_position_is_reported_with_its_file_and_line() {
        let damaged = DUMP.replace("1:4 3..4", "1:4 3..5");
        let found = compare(&outcome(DUMP, 1), &outcome(&damaged, 1));
        assert_eq!(
            found,
            vec![Difference::Line {
                number: 4,
                file: Some("a.fib".to_string()),
                expected: Some("  int 1 i64 1:4 3..4".to_string()),
                actual: Some("  int 1 i64 1:4 3..5".to_string()),
            }]
        );
    }

    #[test]
    fn a_difference_in_a_later_file_names_that_file() {
        let damaged = DUMP.replace("1:1 0..1", "1:1 0..2");
        let found = compare(&outcome(DUMP, 1), &outcome(&damaged, 1));
        assert!(matches!(
            found.as_slice(),
            [Difference::Line { number: 6, file: Some(f), .. }] if f == "b.fib"
        ));
    }

    #[test]
    fn a_missing_line_is_reported_in_the_middle_and_at_the_end() {
        let middle = DUMP.replace("  sym \"a\" 1:2 1..2\n", "");
        let found = compare(&outcome(DUMP, 1), &outcome(&middle, 1));
        assert!(matches!(
            found.as_slice(),
            [Difference::Line {
                number: 3,
                expected: Some(_),
                actual: Some(_),
                ..
            }]
        ));
        let (head, _) = DUMP.split_at(DUMP.rfind("== b.fib").expect("a header"));
        let found = compare(&outcome(DUMP, 1), &outcome(head, 1));
        assert!(matches!(
            found.as_slice(),
            [Difference::Line {
                number: 5,
                actual: None,
                expected: Some(_),
                ..
            }]
        ));
    }

    #[test]
    fn an_extra_line_is_reported() {
        let longer = format!("{DUMP}nil 2:1 9..12\n");
        let found = compare(&outcome(DUMP, 1), &outcome(&longer, 1));
        assert!(matches!(
            found.as_slice(),
            [Difference::Line {
                number: 7,
                expected: None,
                actual: Some(_),
                ..
            }]
        ));
    }

    #[test]
    fn a_different_exit_status_is_reported_alone_when_the_text_is_the_same() {
        let found = compare(&outcome(DUMP, 1), &outcome(DUMP, 0));
        assert_eq!(
            found,
            vec![Difference::Status {
                expected: 1,
                actual: 0
            }]
        );
    }

    #[test]
    fn a_missing_final_newline_is_reported() {
        let cut = DUMP.strip_suffix('\n').expect("ends in a newline");
        let found = compare(&outcome(DUMP, 1), &outcome(cut, 1));
        assert_eq!(
            found,
            vec![Difference::FinalNewline {
                expected: true,
                actual: false
            }]
        );
        let found = compare(&outcome(cut, 1), &outcome(DUMP, 1));
        assert_eq!(
            found,
            vec![Difference::FinalNewline {
                expected: false,
                actual: true
            }]
        );
    }

    #[test]
    fn an_empty_output_differs_from_one_empty_line() {
        let found = compare(&outcome("", 0), &outcome("\n", 0));
        assert!(matches!(
            found.as_slice(),
            [Difference::Line { number: 1, expected: None, actual: Some(l), .. }] if l.is_empty()
        ));
    }

    #[test]
    fn a_crashed_tool_shows_both_the_lost_text_and_the_status() {
        let found = compare(&outcome(DUMP, 1), &outcome("", 134));
        assert_eq!(found.len(), 2);
        assert!(matches!(found[0], Difference::Line { number: 1, .. }));
        assert_eq!(
            found[1],
            Difference::Status {
                expected: 1,
                actual: 134
            }
        );
    }

    #[test]
    fn bytes_that_are_not_text_still_differ() {
        let good = Outcome {
            stdout: b"== a\nsym \"\xC3\xA9\" 1:1 0..2\n".to_vec(),
            status: 0,
        };
        let bad = Outcome {
            stdout: b"== a\nsym \"\xC3\xA8\" 1:1 0..2\n".to_vec(),
            status: 0,
        };
        assert_eq!(compare(&good, &bad).len(), 1);
        let worse = Outcome {
            stdout: b"== a\nsym \"\xFF\" 1:1 0..2\n".to_vec(),
            status: 0,
        };
        assert_eq!(compare(&worse, &worse.clone()), vec![]);
        assert_eq!(compare(&good, &worse).len(), 1);
    }

    #[test]
    fn the_report_quotes_both_lines() {
        let damaged = DUMP.replace("1:2 1..2", "1:3 1..2");
        let found = compare(&outcome(DUMP, 1), &outcome(&damaged, 1));
        let text = found[0].to_string();
        assert!(text.contains("output line 3"), "{text}");
        assert!(text.contains("a.fib"), "{text}");
        assert!(
            text.contains("1:2 1..2") && text.contains("1:3 1..2"),
            "{text}"
        );
    }
}
