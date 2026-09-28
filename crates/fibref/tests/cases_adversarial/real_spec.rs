//! The real cases against the spec they cite: every `spec:` reference
//! names a section (and numbered item) that exists in
//! `spec/ownership.md`; no case has a stray header key after its
//! header ended; the section claims in `cases/ownership/README.md`
//! hold. Cases come before rules (`spec/method.md`, rule 3), so a case
//! that cites a section that does not exist cites nothing.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use fibref::cases::{list_cases_recursive, read_header};

/// The curated suite. `cases/found/` holds untriaged findings from the
/// adversary and the generator, which need not follow these rules until
/// they are promoted.
const SUITE_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../cases/ownership");
const SPEC: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../spec/ownership.md");
const KEYS: [&str; 6] = ["spec", "expect", "result", "audit", "error", "trap"];

fn real_cases() -> Vec<PathBuf> {
    let cases = list_cases_recursive(Path::new(SUITE_DIR)).expect("cases/ownership is readable");
    assert!(!cases.is_empty(), "no cases under {SUITE_DIR}");
    cases
}

fn name_of(path: &Path) -> String {
    path.file_name().unwrap().to_string_lossy().into_owned()
}

/// Section number -> how many numbered items (`1. `, `2. `...) it has.
fn spec_sections() -> BTreeMap<u32, u32> {
    let text = std::fs::read_to_string(SPEC).expect("spec/ownership.md is readable");
    let mut sections = BTreeMap::new();
    let mut current = None;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("## ") {
            let number = rest.split('.').next().and_then(|n| n.parse::<u32>().ok());
            current = number;
            if let Some(n) = number {
                sections.insert(n, 0);
            }
            continue;
        }
        let is_item = line
            .split_once(". ")
            .is_some_and(|(n, _)| n.parse::<u32>().is_ok());
        if let (Some(n), true) = (current, is_item) {
            *sections.entry(n).or_insert(0) += 1;
        }
    }
    assert!(!sections.is_empty(), "no `## N.` sections found in {SPEC}");
    sections
}

/// Which spec document a `§` reference points into. A reference is to
/// ownership.md unless a `types`, `syntax` or `method.md` word precedes
/// it in the same clause (clauses end at `;`, `(` and `)`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Doc {
    Ownership,
    Types,
    Syntax,
    Method,
}

/// Every `§N` / `§N.M` reference with the document it points into.
fn qualified_references(spec: &str) -> Vec<(Doc, u32, Option<u32>)> {
    let mut out = Vec::new();
    for clause in spec.split([';', '(', ')']) {
        let doc = if clause.contains("types") {
            Doc::Types
        } else if clause.contains("syntax") {
            Doc::Syntax
        } else if clause.contains("method") {
            Doc::Method
        } else {
            Doc::Ownership
        };
        for (n, m) in parse_sections(clause) {
            out.push((doc, n, m));
        }
    }
    out
}

/// The ownership.md references in a spec field, as (N, Some(M)).
fn references(spec: &str) -> Vec<(u32, Option<u32>)> {
    qualified_references(spec)
        .into_iter()
        .filter(|(d, _, _)| *d == Doc::Ownership)
        .map(|(_, n, m)| (n, m))
        .collect()
}

/// The `§N` and `§N.M` tokens of one clause.
fn parse_sections(spec: &str) -> Vec<(u32, Option<u32>)> {
    spec.split('§')
        .skip(1)
        .map(|rest| {
            let token: String = rest
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == '.')
                .collect();
            let token = token.trim_end_matches('.');
            let (n, m) = match token.split_once('.') {
                Some((n, m)) => (n, Some(m)),
                None => (token, None),
            };
            (
                n.parse()
                    .unwrap_or_else(|_| panic!("bad section in {spec:?}")),
                m.map(|m| m.parse().unwrap_or_else(|_| panic!("bad item in {spec:?}"))),
            )
        })
        .collect()
}

#[test]
fn reference_parsing_reads_the_forms_the_cases_use() {
    assert_eq!(references("§4"), vec![(4, None)]);
    assert_eq!(references("§3.2"), vec![(3, Some(2))]);
    assert_eq!(references("§4, §5"), vec![(4, None), (5, None)]);
    assert_eq!(references("§5 (proposed)"), vec![(5, None)]);
    assert_eq!(references("§3.1."), vec![(3, Some(1))]);
}

/// Whether `types.md` or `syntax.md` has a heading `## N.` (no item)
/// or `### N.M`.
fn heading_exists(doc: Doc, n: u32, m: Option<u32>) -> bool {
    let file = match doc {
        Doc::Types => "types.md",
        Doc::Syntax => "syntax.md",
        _ => return true,
    };
    let path = format!("{}/../../spec/{file}", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(path).expect("spec file is readable");
    let wanted = match m {
        Some(m) => format!("### {n}.{m} "),
        None => format!("## {n}. "),
    };
    text.lines().any(|l| l.starts_with(&wanted))
}

#[test]
fn qualified_references_follow_their_document() {
    assert_eq!(
        qualified_references("§1; types §2.7, §3.3; syntax §3.16"),
        vec![
            (Doc::Ownership, 1, None),
            (Doc::Types, 2, Some(7)),
            (Doc::Types, 3, Some(3)),
            (Doc::Syntax, 3, Some(16)),
        ]
    );
    assert_eq!(
        qualified_references("§4 (types §6.4)"),
        vec![(Doc::Ownership, 4, None), (Doc::Types, 6, Some(4))]
    );
}

#[test]
fn every_types_and_syntax_citation_names_a_real_heading() {
    for path in real_cases() {
        let header = read_header(&path).unwrap_or_else(|e| panic!("{e}"));
        for (doc, n, m) in qualified_references(&header.spec) {
            assert!(
                heading_exists(doc, n, m),
                "{}: cites {doc:?} §{n}{} which has no heading",
                name_of(&path),
                m.map(|m| format!(".{m}")).unwrap_or_default()
            );
        }
    }
}

#[test]
fn every_real_case_cites_at_least_one_section_that_exists_in_the_spec() {
    let sections = spec_sections();
    for path in real_cases() {
        let header = read_header(&path).unwrap_or_else(|e| panic!("{e}"));
        assert!(
            !qualified_references(&header.spec).is_empty(),
            "{}: spec field {:?} cites no § section",
            name_of(&path),
            header.spec
        );
        let refs = references(&header.spec);
        for (n, m) in refs {
            let items = sections.get(&n).unwrap_or_else(|| {
                panic!(
                    "{}: cites §{n} but spec/ownership.md has sections {:?}",
                    name_of(&path),
                    sections.keys().collect::<Vec<_>>()
                )
            });
            if let Some(m) = m {
                assert!(
                    m >= 1 && m <= *items,
                    "{}: cites §{n}.{m} but section {n} has {items} numbered items",
                    name_of(&path)
                );
            }
        }
    }
}

#[test]
fn no_real_case_has_a_header_key_line_after_its_header_ended() {
    // A `;; result: 2` in the prose after the header would be silently
    // ignored by the harness; the case files must not contain one.
    for path in real_cases() {
        let source = std::fs::read_to_string(&path).unwrap();
        let header_len = source
            .lines()
            .take_while(|l| {
                l.strip_prefix(";; ")
                    .and_then(|r| r.split_once(':'))
                    .is_some_and(|(k, _)| KEYS.contains(&k))
            })
            .count();
        assert!(
            header_len >= 3,
            "{}: header shorter than 3 lines",
            name_of(&path)
        );
        for (index, line) in source.lines().enumerate().skip(header_len) {
            let stray = line
                .strip_prefix(";;")
                .map(|r| r.trim_start())
                .and_then(|r| r.split_once(':'))
                .is_some_and(|(k, _)| KEYS.contains(&k.trim()));
            assert!(
                !stray,
                "{}:{}: header key after the header ended: {line:?}",
                name_of(&path),
                index + 1
            );
        }
    }
}

#[test]
fn every_real_case_header_is_followed_by_prose_or_code_on_the_next_line() {
    // The header ends at the first non-header line; that line must
    // exist (a file that is only a header has no program to run).
    for path in real_cases() {
        let source = std::fs::read_to_string(&path).unwrap();
        let after_header = source
            .lines()
            .skip_while(|l| l.starts_with(";; ") && l.contains(':'))
            .find(|l| !l.trim().is_empty());
        assert!(
            after_header.is_some_and(|l| !l.trim().is_empty()),
            "{}: nothing after the header",
            name_of(&path)
        );
        assert!(
            source.contains("(defun main"),
            "{}: no main function",
            name_of(&path)
        );
    }
}

#[test]
fn readme_section_claims_hold() {
    // README: 16 shows the §7 pattern; 17 pins §5; 19 and 20 cover §6;
    // 15 is the permitted leak (§6).
    let expected: [(u32, &str); 5] = [(15, "§6"), (16, "§7"), (17, "§5"), (19, "§6"), (20, "§6")];
    for path in real_cases() {
        let name = name_of(&path);
        let number: u32 = name.split('-').next().unwrap().parse().unwrap();
        if let Some((_, section)) = expected.iter().find(|(n, _)| *n == number) {
            let header = read_header(&path).unwrap_or_else(|e| panic!("{e}"));
            assert!(
                header.spec.contains(section),
                "{name}: README says {section}, header says {:?}",
                header.spec
            );
        }
    }
}

#[test]
fn reject_error_texts_are_plain_lowercase_phrases() {
    // The error text is matched as a literal substring, so a stray
    // capital, trailing punctuation or doubled space in the header would
    // make the case unpassable for no reason.
    for path in real_cases() {
        let header = read_header(&path).unwrap_or_else(|e| panic!("{e}"));
        if let fibref::cases::Verdict::Reject { error } = header.verdict {
            let name = name_of(&path);
            assert_eq!(
                error,
                error.trim(),
                "{name}: error text has outer whitespace"
            );
            assert!(!error.contains("  "), "{name}: doubled space in {error:?}");
            assert!(
                !error.ends_with('.') && !error.ends_with(':'),
                "{name}: trailing punctuation in {error:?}"
            );
            // Capitals are allowed: canonical texts name protocols and
            // types (`no implementation of Describe for (Cell i64)`).
            assert!(
                !error.starts_with(|c: char| c.is_uppercase()),
                "{name}: error text should not start with a capital: {error:?}"
            );
        }
    }
}
