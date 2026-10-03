//! Comparing the table with the cases: which rows no case covers, and which
//! `covers:` lines claim what the case's code does not do.
//!
//! A case *calls* a row when its code, with the comments cut off and the
//! text of its strings blanked, has a token that is the row's spelling or
//! its name; a token with a module prefix counts by the part after its last
//! `/` (`char/digit?` calls `digit?`, `fib.prelude/nth` calls `nth`). The
//! rows that are syntax or a name that is also a constructor (`@x`, `~@x`,
//! `#(..)`, `(:k x)`, `->Name`, `some`) have no such token and are judged by
//! the rules of `syntax`. The point is that a header that lists a name
//! cannot make it covered by itself.

use std::collections::BTreeMap;

use super::cases::Case;
use super::rows::Row;
use super::syntax;

/// The most names one case may cover: a longer list tests nothing in
/// particular (README §9.1).
pub const MOST_COVERED: usize = 12;

/// `source` with the comments removed and the strings and character
/// literals blanked, so that a name in a comment or a string is no call.
pub fn code_of(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut chars = source.chars();
    let mut in_string = false;
    while let Some(c) = chars.next() {
        match (in_string, c) {
            (true, '\\') => {
                chars.next();
                out.push_str("  ");
            }
            (true, '"') => {
                in_string = false;
                out.push('"');
            }
            (true, _) => out.push(' '),
            (false, '"') => {
                in_string = true;
                out.push('"');
            }
            (false, ';') => {
                for rest in chars.by_ref() {
                    if rest == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            (false, '\\') => {
                chars.next();
                out.push_str("  ");
            }
            (false, _) => out.push(c),
        }
    }
    out
}

fn tokens(code: &str) -> impl Iterator<Item = &str> {
    code.split(|c: char| c.is_whitespace() || "()[]{},\"".contains(c))
        .filter(|t| !t.is_empty())
}

/// The part of a token after its last `/`, when it has a module prefix.
fn unqualified(token: &str) -> &str {
    match token.rfind('/') {
        Some(at) if at > 0 && at + 1 < token.len() => &token[at + 1..],
        _ => token,
    }
}

/// Whether the code of a case calls the row.
pub fn calls(code: &str, row: &Row) -> bool {
    if syntax::called(code, row) {
        return true;
    }
    !syntax::judged_here(row)
        && tokens(code).any(|t| {
            let bare = unqualified(t);
            [t, bare]
                .iter()
                .any(|s| *s == row.spelling || *s == row.name)
        })
}

/// The rows of the tranche that no case covers.
pub fn uncovered<'a>(rows: &'a [Row], cases: &[Case], tranche: u8) -> Vec<&'a Row> {
    rows.iter()
        .filter(|r| r.tranche <= tranche)
        .filter(|r| !cases.iter().any(|c| c.covers.contains(&r.name)))
        .collect()
}

/// What is wrong with the `covers:` lines: a name that is no row, a row the
/// case's code never calls, a case that covers too many names.
pub fn claims(rows: &[Row], cases: &[Case]) -> Vec<String> {
    let mut found = Vec::new();
    for case in cases {
        if case.covers.len() > MOST_COVERED {
            found.push(format!(
                "{}: covers {} names; at most {MOST_COVERED}",
                case.name,
                case.covers.len()
            ));
        }
        for name in &case.covers {
            let named: Vec<&Row> = rows.iter().filter(|r| &r.name == name).collect();
            if named.is_empty() {
                found.push(format!(
                    "{}: `covers: {name}` is no row of the table",
                    case.name
                ));
            }
            for row in named.iter().filter(|r| !calls(&case.code, r)) {
                found.push(format!(
                    "{}: covers `{name}` but its code never calls `{}` (spec line {})",
                    case.name, row.spelling, row.line
                ));
            }
        }
    }
    found
}

/// The names that two rows of one section share.
pub fn duplicate_names(rows: &[Row]) -> Vec<String> {
    let mut seen: BTreeMap<(&str, &str), usize> = BTreeMap::new();
    for row in rows {
        *seen.entry((&row.section, &row.name)).or_default() += 1;
    }
    seen.into_iter()
        .filter(|(_, n)| *n > 1)
        .map(|((section, name), n)| format!("{section}: `{name}` is {n} rows"))
        .collect()
}

/// The rows grouped by section, one line each, for a failure message.
pub fn by_section(rows: &[&Row]) -> String {
    let mut groups: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for row in rows {
        groups.entry(&row.section).or_default().push(&row.name);
    }
    let lines: Vec<String> = groups
        .into_iter()
        .map(|(section, names)| format!("  {section}: {}", names.join(" ")))
        .collect();
    lines.join("\n")
}

/// The names each case covers, one line per case that covers any.
pub fn per_case(cases: &[Case]) -> String {
    let lines: Vec<String> = cases
        .iter()
        .filter(|c| !c.covers.is_empty())
        .map(|c| format!("  {}: {}", c.name, c.covers.join(" ")))
        .collect();
    format!("covered, by case:\n{}", lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(name: &str, spelling: &str, tranche: u8) -> Row {
        Row {
            section: "4.4 fib.seq".to_string(),
            name: name.to_string(),
            spelling: spelling.to_string(),
            tranche,
            line: 1,
        }
    }

    fn case(name: &str, covers: &[&str], source: &str) -> Case {
        Case {
            name: name.to_string(),
            covers: covers.iter().map(|s| s.to_string()).collect(),
            open: Vec::new(),
            bounded: false,
            code: code_of(source),
        }
    }

    fn table() -> Vec<Row> {
        vec![
            row("map", "map", 1),
            row("filter", "filter", 1),
            row("take", "take", 2),
            row("deref", "@x", 1),
            row("digit?", "digit?", 1),
            row("clojure.string/join", "str/join", 1),
        ]
    }

    #[test]
    fn code_has_no_comments_and_no_string_text() {
        let code = code_of(";; map\n(f \"map and ; this\" 1) ; filter\n(g \\a)\n");
        assert!(!code.contains("map") && !code.contains("filter") && !code.contains("this"));
        assert!(code.contains("(f \"") && code.contains("(g"));
        assert_eq!(code.lines().count(), 3);
    }

    #[test]
    fn a_row_is_called_by_a_token_not_by_a_substring() {
        let r = row("map", "map", 1);
        assert!(calls("(map f xs)", &r));
        assert!(calls("(fib.prelude/map f xs)", &r));
        assert!(!calls("(mapcat f xs)", &r));
        assert!(!calls("(bitmap x)", &r));
        assert!(calls("(char/digit? c)", &row("digit?", "digit?", 1)));
        assert!(calls(
            "(str/join \",\" xs)",
            &row("clojure.string/join", "str/join", 1)
        ));
        assert!(calls(
            "(clojure.string/join xs)",
            &row("clojure.string/join", "str/join", 1)
        ));
    }

    #[test]
    fn a_reader_row_is_called_by_its_prefix() {
        let at = row("deref", "@x", 1);
        assert!(calls("(count @acc)", &at));
        assert!(!calls("(count acc)", &at));
        assert!(!calls(&code_of("(f \"@acc\")"), &at));
        assert!(calls("`(do ~x)", &row("`x", "`x", 1)));
    }

    #[test]
    fn a_clean_suite_has_no_finding_and_no_uncovered_row() {
        let cases = [
            case("240-a.fib", &["map", "filter"], "(map f (filter p xs))"),
            case(
                "241-b.fib",
                &["deref", "digit?"],
                "(do @c (char/digit? \\1))",
            ),
            case("242-c.fib", &["clojure.string/join"], "(str/join \",\" xs)"),
        ];
        assert_eq!(claims(&table(), &cases), Vec::<String>::new());
        assert!(uncovered(&table(), &cases, 1).is_empty());
    }

    /// Canary 1: a row of the tranche that no case covers is found, and a
    /// row of a later tranche is not asked for.
    #[test]
    fn canary_a_row_with_no_case_is_uncovered() {
        let cases = [case("240-a.fib", &["map"], "(map f xs)")];
        let rows = table();
        let missing = uncovered(&rows, &cases, 1);
        let names: Vec<&str> = missing.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["filter", "deref", "digit?", "clojure.string/join"]);
        assert!(by_section(&missing).contains("4.4 fib.seq: filter deref"));
    }

    /// Canary 2: a `covers:` of a name that is no row.
    #[test]
    fn canary_a_covers_of_a_missing_name_is_found() {
        let cases = [case(
            "240-a.fib",
            &["map", "mapp"],
            "(map f xs) (mapp f xs)",
        )];
        let found = claims(&table(), &cases);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].contains("`covers: mapp` is no row"), "{found:?}");
    }

    /// Canary 3: a case that lists a name and never calls it.
    #[test]
    fn canary_a_case_that_lists_a_name_and_never_calls_it_is_found() {
        let cases = [case(
            "240-a.fib",
            &["map", "filter"],
            ";; filter\n(map f \"filter\")",
        )];
        let found = claims(&table(), &cases);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(
            found[0].contains("covers `filter` but its code never calls `filter`"),
            "{found:?}"
        );
    }

    /// Canary 4 (C2-23): `some` is `Option`'s constructor, and its token is in
    /// nearly every case. A case that covers the row is found unless it calls
    /// `(some pred c)` with two operands.
    #[test]
    fn canary_a_case_that_only_matches_some_does_not_cover_the_row() {
        let rows = vec![row("some", "some", 2), row("map", "map", 1)];
        let constructor = "(match (get m 1) ((some x) x) (nil (unwrap-or (some 0) 1)))";
        let found = claims(&rows, &[case("2450-a.fib", &["some"], constructor)]);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(
            found[0].contains("covers `some` but its code never calls `some`"),
            "{found:?}"
        );
        let call = "(some even? [1 3 4])";
        assert!(claims(&rows, &[case("2450-b.fib", &["some"], call)]).is_empty());
    }

    /// The syntax rows of tranche 2 are covered by a case that has the
    /// syntax, and by nothing else (the rules are `syntax`'s).
    #[test]
    fn a_syntax_row_is_covered_by_a_case_that_has_the_syntax() {
        let rows = vec![
            row("#(...)", "#(f", 2),
            row("#{...}", "#{a", 2),
            row(":k", ":k", 2),
            row("->Name", "->Point", 2),
            row("print-method", "Debug", 2),
        ];
        let names: Vec<&str> = rows.iter().map(|r| r.name.as_str()).collect();
        let source = "(impl Debug P (debug (self) \"p\")) (map #(* % 2) #{1}) (:age (->P 1))";
        let good = case("1000-a.fib", &names, source);
        assert_eq!(claims(&rows, &[good]), Vec::<String>::new());
        let bare = "(impl Show P (show (self) \"p\")) (map (fn (x) x) [1]) (get m :age)";
        let found = claims(&rows, &[case("1000-b.fib", &names, bare)]);
        assert_eq!(found.len(), 5, "{found:?}");
    }

    #[test]
    fn a_case_covering_more_than_twelve_names_is_found() {
        let names: Vec<String> = (0..13).map(|i| format!("n{i}")).collect();
        let rows: Vec<Row> = names.iter().map(|n| row(n, n, 1)).collect();
        let source = names.join(" ");
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let found = claims(&rows, &[case("240-a.fib", &refs, &source)]);
        assert_eq!(found.len(), 1);
        assert!(
            found[0].contains("covers 13 names; at most 12"),
            "{found:?}"
        );
    }

    #[test]
    fn two_rows_of_one_section_with_one_name_are_found() {
        let mut rows = table();
        assert!(duplicate_names(&rows).is_empty());
        rows.push(row("map", "map", 2));
        assert_eq!(duplicate_names(&rows), ["4.4 fib.seq: `map` is 2 rows"]);
        let mut other = row("map", "map", 2);
        other.section = "4.5 fib.coll".to_string();
        let rows = vec![row("map", "map", 1), other];
        assert!(
            duplicate_names(&rows).is_empty(),
            "one name in two sections is fine"
        );
    }
}
