//! Reading the case files of `cases/stdlib/`: the names a case covers, its
//! code, and what its name and its header must say alike.
//!
//! A case is a `*.fib` directly in the directory or the `main.fib` of a
//! subdirectory (`fibref::cases::list_cases`); `support/` has no `main.fib`
//! and is not one. The number before the first dash is the case's number and
//! belongs to one case; the word after it is the kind: `open-` says the
//! header has `open:` items, `count-` and `bound-` that it has an `allocs`
//! bound (the kind words are reserved: a unit case's slug does not begin
//! with one, whatever it is about; the rest of the kinds, `ref-` `law-` `rule-`, are judged by what
//! they compute, README §9.4).

use std::path::{Path, PathBuf};

use fibref::cases::{list_cases, parse_header, parse_labels, Verdict};

/// A case of the library suite.
#[derive(Debug, Clone)]
pub struct Case {
    /// The file name, or `dir/main.fib` for a program of several modules.
    pub name: String,
    /// The Clojure names its header says it calls, in order.
    pub covers: Vec<String>,
    /// The §7 items its header says it waits for.
    pub open: Vec<String>,
    /// Whether the header bounds the allocations.
    pub bounded: bool,
    /// Its source with the comments removed and the strings blanked.
    pub code: String,
}

/// The §7 items that a case may still wait for, and the package ids of the
/// tranche 1 and tranche 2 plans a case may wait for (`open: L6` fails as
/// `OpenPassed` the day that package lands, which is how the flip of the
/// plan's step M is forced). Anything else in an `open:` line is a typo or a
/// reason that is not one: the test `open_cases_are_allowed` fails on it.
///
/// Tranche 2 (Z0, stdlib plan t2 §4.4) added the items `L3b L7 L8 L14 L16 L17
/// L19 E2 E8 E11 B5` and the packages a case may wait for, `X3` to `X12`,
/// `P1`, `Y3`, `Y4`, `Y5`, `Y11`, `Y12`: a package that finds it needs
/// another id asks for it, and a case never waits for a package of an
/// earlier wave (X1, X2, Y1, Y2 land before the cases that would wait for
/// them are written). `L7` was already a package id of tranche 1; it is now
/// also the spec item L7 (patterns in parameters), and one entry serves both.
pub const ALLOWED_OPEN: &[&str] = &[
    // tranche 1: §7 items, and the package ids of its plan
    "L1",
    "L15",
    "L20",
    "L21",
    "L22",
    "L23",
    "L24",
    "L26",
    "L28",
    "L29",
    "C1",
    "C5",
    "C7",
    "C9",
    "E14",
    "L5b",
    "L6",
    "L7",
    "L8b",
    "L11",
    "T2-sources",
    // tranche 2: §7 items
    "L3b",
    "L8",
    "L14",
    "L16",
    "L17",
    "L19",
    "E2",
    "E8",
    "E11",
    "B5",
    // tranche 2: packages
    "X3",
    "X4",
    "X5",
    "X6",
    "X7",
    "X8",
    "X9",
    "X10",
    "X11",
    "X12",
    "P1",
    "Y3",
    "Y4",
    "Y5",
    "Y11",
    "Y12",
];

/// The cases of `dir`, with their labels and code.
pub fn load_cases(dir: &Path) -> Result<Vec<Case>, String> {
    let mut cases = Vec::new();
    for path in list_cases(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        cases.push(load_case(dir, &path)?);
    }
    if cases.is_empty() {
        return Err(format!("{}: no case", dir.display()));
    }
    Ok(cases)
}

fn load_case(dir: &Path, path: &Path) -> Result<Case, String> {
    let source = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let labels = parse_labels(path, &source).map_err(|e| e.to_string())?;
    let header = parse_header(path, &source).map_err(|e| e.to_string())?;
    let bounded = matches!(
        header.verdict,
        Verdict::Accept {
            allocs: Some(_),
            ..
        }
    );
    let shown: PathBuf = path.strip_prefix(dir).unwrap_or(path).to_path_buf();
    Ok(Case {
        name: shown.to_string_lossy().into_owned(),
        covers: labels.covers,
        open: labels.open,
        bounded,
        code: super::cover::code_of(&source),
    })
}

/// The number and the kind of a case name: `240-take-while.fib` is
/// `("240", "take")`, `905-open-x.fib` is `("905", "open")`, a directory's
/// `015-reject-x/main.fib` is `("015", "reject")`.
pub fn number_and_kind(name: &str) -> (String, String) {
    let base = name.split('/').next().unwrap_or(name);
    let (number, rest) = base.split_once('-').unwrap_or((base, ""));
    let kind = rest.split(['-', '.']).next().unwrap_or("");
    (number.to_string(), kind.to_string())
}

/// What is wrong with the names and labels of `cases`: a number two cases
/// share, a name that does not start with digits, an `open-` case with no
/// `open:` (or the reverse), a `count-` or `bound-` case with no `allocs`.
pub fn naming(cases: &[Case]) -> Vec<String> {
    let mut found = Vec::new();
    let mut seen: Vec<(String, &str)> = Vec::new();
    for case in cases {
        let (number, kind) = number_and_kind(&case.name);
        if number.is_empty() || !number.chars().all(|c| c.is_ascii_digit()) {
            found.push(format!(
                "{}: the name does not start with a number",
                case.name
            ));
            continue;
        }
        if let Some((_, other)) = seen.iter().find(|(n, _)| *n == number) {
            found.push(format!(
                "{}: the number {number} is also {other}'s",
                case.name
            ));
        }
        seen.push((number, &case.name));
        if (kind == "open") != !case.open.is_empty() {
            found.push(format!(
                "{}: `open-` and the `open:` header must go together",
                case.name
            ));
        }
        if (kind == "count" || kind == "bound") && !case.bounded {
            found.push(format!(
                "{}: a {kind}- case needs an `allocs: <= N` header; a unit case's slug must \
                 not begin with the word {kind} (kind words are reserved, README \"Kinds\"): \
                 rename it, say `the-{kind}-of-..`",
                case.name
            ));
        }
    }
    found
}

/// The `open:` items of `cases` that are not in `allowed`.
pub fn open_items_outside(cases: &[Case], allowed: &[&str]) -> Vec<String> {
    let mut found = Vec::new();
    for case in cases {
        for item in case.open.iter().filter(|i| !allowed.contains(&i.as_str())) {
            found.push(format!(
                "{}: `open: {item}` is no item that may be open",
                case.name
            ));
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn case(name: &str, open: &[&str], bounded: bool) -> Case {
        Case {
            name: name.to_string(),
            covers: Vec::new(),
            open: open.iter().map(|s| s.to_string()).collect(),
            bounded,
            code: String::new(),
        }
    }

    #[test]
    fn the_number_and_the_kind_come_from_the_name() {
        let split = |n: &str| number_and_kind(n);
        assert_eq!(split("240-take-while.fib"), ("240".into(), "take".into()));
        assert_eq!(split("905-open-x.fib"), ("905".into(), "open".into()));
        assert_eq!(
            split("015-reject-x/main.fib"),
            ("015".into(), "reject".into())
        );
        assert_eq!(
            split("021-count-a-walk.fib"),
            ("021".into(), "count".into())
        );
    }

    #[test]
    fn a_well_named_suite_has_no_finding() {
        let cases = [
            case("000-a.fib", &[], false),
            case("021-count-x.fib", &[], true),
            case("900-open-x.fib", &["L20"], false),
        ];
        assert_eq!(naming(&cases), Vec::<String>::new());
    }

    #[test]
    fn each_naming_rule_fires() {
        let shared = [case("000-a.fib", &[], false), case("000-b.fib", &[], false)];
        assert!(naming(&shared)[0].contains("also 000-a.fib"));
        let nameless = [case("a-b.fib", &[], false)];
        assert!(naming(&nameless)[0].contains("start with a number"));
        let unlabelled = [case("900-open-x.fib", &[], false)];
        assert!(naming(&unlabelled)[0].contains("go together"));
        let forgot_kind = [case("901-x.fib", &["L20"], false)];
        assert!(naming(&forgot_kind)[0].contains("go together"));
        let unbounded = [
            case("021-count-x.fib", &[], false),
            case("022-bound-x.fib", &[], false),
        ];
        assert_eq!(naming(&unbounded).len(), 2);
        // The finding says what to do about a unit case that only starts
        // with the word, which is the usual reason for it.
        assert!(naming(&unbounded)[0].contains("must not begin with the word count"));
    }

    #[test]
    fn an_open_item_outside_the_list_is_a_finding() {
        let cases = [case("900-open-x.fib", &["L20", "L99"], false)];
        let found = open_items_outside(&cases, ALLOWED_OPEN);
        assert_eq!(found.len(), 1);
        assert!(found[0].contains("`open: L99`"), "{found:?}");
    }

    /// Tranche 2's items and package ids may be waited for (Z0), the ones the
    /// plan did not name may not, and the list has no entry twice.
    #[test]
    fn tranche_two_items_and_packages_may_be_open_and_nothing_else_new() {
        let items = "L3b L7 L8 L14 L16 L17 L19 E2 E8 E11 B5";
        let packages = "X3 X4 X5 X6 X7 X8 X9 X10 X11 X12 P1 Y3 Y4 Y5 Y11 Y12";
        for id in items.split(' ').chain(packages.split(' ')) {
            let cases = [case("1000-open-x.fib", &[id], false)];
            assert!(open_items_outside(&cases, ALLOWED_OPEN).is_empty(), "{id}");
        }
        for id in ["X1", "X2", "X13", "Y1", "Y2", "Y10", "E3", "E4", "Z0"] {
            let cases = [case("1000-open-x.fib", &[id], false)];
            assert_eq!(open_items_outside(&cases, ALLOWED_OPEN).len(), 1, "{id}");
        }
        let mut sorted = ALLOWED_OPEN.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), ALLOWED_OPEN.len(), "an id is listed twice");
    }

    /// Four digits are a number (plan t2 §4.4: `1000` to `5999`), and the
    /// number is the whole run of digits: `1000-x` is not `100-x`, so the
    /// two never count as one case's number, and two cases of one four-digit
    /// number do.
    #[test]
    fn a_four_digit_number_is_a_number_and_1000_x_is_not_100_x() {
        let split = |n: &str| number_and_kind(n);
        assert_eq!(split("1000-open-x.fib"), ("1000".into(), "open".into()));
        assert_eq!(split("2250-ref-x.fib"), ("2250".into(), "ref".into()));
        assert_eq!(
            split("1650-reject-x/main.fib"),
            ("1650".into(), "reject".into())
        );
        assert_ne!(split("1000-x.fib").0, split("100-x.fib").0);
        let apart = [
            case("100-x.fib", &[], false),
            case("1000-x.fib", &[], false),
        ];
        assert_eq!(naming(&apart), Vec::<String>::new());
        let same = [
            case("1000-x.fib", &[], false),
            case("1000-y.fib", &[], false),
        ];
        assert!(naming(&same)[0].contains("the number 1000 is also 1000-x.fib's"));
        let kinds = [
            case("1000-open-x.fib", &[], false),
            case("2100-count-x.fib", &[], false),
        ];
        assert_eq!(naming(&kinds).len(), 2, "{:?}", naming(&kinds));
    }

    #[test]
    fn the_real_suite_loads() {
        let cases = load_cases(Path::new(crate::stdlib_table::CASES)).unwrap();
        assert!(cases
            .iter()
            .any(|c| c.name == "000-each-while-early-exit.fib"));
        assert!(cases.iter().all(|c| !c.code.contains(";;")));
    }
}
