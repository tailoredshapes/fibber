//! Reading the case files of `cases/stdlib/`: the names a case covers, its
//! code, and what its name and its header must say alike.
//!
//! A case is a `*.fib` directly in the directory or the `main.fib` of a
//! subdirectory (`fibref::cases::list_cases`); `support/` has no `main.fib`
//! and is not one. The number before the first dash is the case's number and
//! belongs to one case; the word after it is the kind: `open-` says the
//! header has `open:` items, `count-` and `bound-` that it has an `allocs`
//! bound (the rest of the kinds, `ref-` `law-` `rule-`, are judged by what
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
/// tranche 1 plan a case may wait for (`open: L6` fails as `OpenPassed` the
/// day that package lands, which is how the flip of the plan's step M is
/// forced). Anything else in an `open:` line is a typo or a reason that is
/// not one: the test `open_cases_are_allowed` fails on it.
pub const ALLOWED_OPEN: &[&str] = &[
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
                "{}: a {kind}- case has an `allocs: <= N` header",
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
    }

    #[test]
    fn an_open_item_outside_the_list_is_a_finding() {
        let cases = [case("900-open-x.fib", &["L20", "L99"], false)];
        let found = open_items_outside(&cases, ALLOWED_OPEN);
        assert_eq!(found.len(), 1);
        assert!(found[0].contains("`open: L99`"), "{found:?}");
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
