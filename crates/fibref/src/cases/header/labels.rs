//! The two header keys that label a case without changing its verdict:
//! `covers` and `open` (`cases/stdlib/README.md`).
//!
//! `;; covers: map filter` lists the names of the function table of
//! `spec/stdlib.md` §4 that the case calls; the test `stdlib_table`
//! reads it. `;; open: L20 C9` lists the items of `spec/stdlib.md` §7
//! whose absence is why an `accept` case fails today: the case is run,
//! and [`judge_labelled`] reports it OPEN while it fails and a failure
//! the day it passes, so the label cannot outlive its reason.
//!
//! Both values are names separated by white space. `covers` may be empty
//! (a case that judges an interface, not a row); `open` may not (an
//! empty list names no item, so it would excuse the case for nothing).
//! A name given twice in one list is a bad value.
//!
//! [`judge_labelled`]: crate::cases::verdict::judge_labelled

use std::path::Path;

use super::{collect, Fields, HeaderError, HeaderErrorKind};

/// The labels of a case header.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Labels {
    /// The table names the case calls, in the order written.
    pub covers: Vec<String>,
    /// The §7 items that excuse the case's failure, in the order written.
    pub open: Vec<String>,
}

impl Fields<'_> {
    /// The list a key holds, with each name once.
    fn names(&self, key: &'static str, may_be_empty: bool) -> Result<Vec<String>, HeaderError> {
        let Some(field) = self.get(key) else {
            return Ok(Vec::new());
        };
        let mut names: Vec<String> = Vec::new();
        for word in field.value.split_whitespace() {
            if names.iter().any(|n| n == word) {
                return Err(self.bad(key, field.line, field.value));
            }
            names.push(word.to_string());
        }
        if names.is_empty() && !may_be_empty {
            return Err(self.bad(key, field.line, field.value));
        }
        Ok(names)
    }

    fn bad(&self, key: &'static str, line: usize, value: &str) -> HeaderError {
        let value = value.to_string();
        self.error(line, HeaderErrorKind::BadValue { key, value })
    }

    /// Both labels, validated.
    pub(super) fn labels(&self) -> Result<Labels, HeaderError> {
        Ok(Labels {
            covers: self.names("covers", true)?,
            open: self.names("open", false)?,
        })
    }
}

/// The labels in the header at the top of `source`, which was read from
/// `path`; none when neither key is present.
pub fn parse_labels(path: &Path, source: &str) -> Result<Labels, HeaderError> {
    collect(path, source)?.labels()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cases::header::parse_header;

    const HEAD: &str = ";; spec: §4\n;; expect: accept\n;; result: 1\n;; audit: clean\n";

    fn labels(extra: &str) -> Result<Labels, HeaderError> {
        parse_labels(Path::new("t.fib"), &format!("{HEAD}{extra}"))
    }

    fn kind(extra: &str) -> (usize, HeaderErrorKind) {
        match parse_header(Path::new("t.fib"), &format!("{HEAD}{extra}")) {
            Err(e) => (e.line, e.kind),
            Ok(h) => panic!("expected a header error, parsed {h:?}"),
        }
    }

    fn bad(key: &'static str, value: &str) -> HeaderErrorKind {
        let value = value.to_string();
        HeaderErrorKind::BadValue { key, value }
    }

    #[test]
    fn a_header_without_labels_has_none() {
        assert_eq!(labels(""), Ok(Labels::default()));
    }

    #[test]
    fn covers_and_open_are_the_names_in_the_order_written() {
        let got = labels(";; covers: map  filter ->>\n;; open: L20 T2-sources\n");
        assert_eq!(
            got,
            Ok(Labels {
                covers: vec!["map".into(), "filter".into(), "->>".into()],
                open: vec!["L20".into(), "T2-sources".into()],
            })
        );
    }

    #[test]
    fn the_labels_do_not_change_the_verdict() {
        let plain = parse_header(Path::new("t.fib"), HEAD);
        let labelled = parse_header(
            Path::new("t.fib"),
            &format!("{HEAD};; covers: map\n;; open: L1\n"),
        );
        assert_eq!(plain, labelled);
    }

    #[test]
    fn covers_may_be_empty_open_may_not() {
        assert_eq!(labels(";; covers:\n"), Ok(Labels::default()));
        assert_eq!(kind(";; open:\n"), (5, bad("open", "")));
    }

    #[test]
    fn a_name_twice_in_a_list_is_a_bad_value_at_its_line() {
        assert_eq!(
            kind(";; covers: map filter map\n"),
            (5, bad("covers", "map filter map"))
        );
        assert_eq!(kind(";; open: L1 L1\n"), (5, bad("open", "L1 L1")));
    }

    #[test]
    fn open_is_for_accept_cases_only() {
        for head in [
            ";; spec: s\n;; expect: reject\n;; error: e\n;; open: L1\n",
            ";; spec: s\n;; expect: trap\n;; trap: t\n;; open: L1\n",
        ] {
            let got = parse_header(Path::new("t.fib"), head).map_err(|e| (e.line, e.kind));
            assert_eq!(
                got,
                Err((4, HeaderErrorKind::ForbiddenKey("open"))),
                "{head}"
            );
        }
    }

    #[test]
    fn covers_is_allowed_for_every_verdict() {
        let reject = ";; spec: s\n;; expect: reject\n;; error: e\n;; covers: nth\n";
        assert!(parse_header(Path::new("t.fib"), reject).is_ok());
        let trap = ";; spec: s\n;; expect: trap\n;; trap: t\n;; covers: nth\n";
        assert!(parse_header(Path::new("t.fib"), trap).is_ok());
    }

    #[test]
    fn labels_after_the_header_are_prose() {
        let source = format!("{HEAD}\n;; covers: map\n");
        assert_eq!(
            parse_labels(Path::new("t.fib"), &source),
            Ok(Labels::default())
        );
    }
}
