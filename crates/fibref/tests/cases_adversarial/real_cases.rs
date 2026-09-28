//! Every real file under `cases/`, recursively: each header must parse,
//! and the verdicts must agree with what `cases/ownership/README.md`
//! says about them. This is what CI runs against the actual cases.

use std::path::{Path, PathBuf};

use fibref::cases::{
    list_cases_recursive, read_header, run_dir, AuditExpect, PendingEvaluator, Verdict,
};

const CASES_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../cases");
/// The curated suite. `cases/found/` holds untriaged findings from the
/// adversary and the generator, which need not follow these rules until
/// they are promoted.
const SUITE_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../cases/ownership");

fn real_cases() -> Vec<PathBuf> {
    let cases = list_cases_recursive(Path::new(SUITE_DIR)).expect("cases/ownership is readable");
    assert!(
        !cases.is_empty(),
        "no .fib files under {SUITE_DIR}; the test would prove nothing"
    );
    cases
}

const REJECT: &[u32] = &[
    12, 13, 14, 18, 21, 34, 40, 82, 90, 93, 105, 107, 108, 109, 112,
];
const LEAK_CYCLE: [u32; 2] = [15, 80];
/// The cases whose verdict is a run-time trap (method.md rule 3).
const TRAP: [u32; 4] = [101, 102, 103, 104];
/// The last case number of this suite. Another series starts at 131.
const LAST: u32 = 112;

fn number_of(path: &Path) -> u32 {
    let name = path.file_name().unwrap().to_string_lossy();
    name.split('-')
        .next()
        .and_then(|n| n.parse().ok())
        .unwrap_or_else(|| panic!("{name} does not start with a number"))
}

/// An independent walk of `cases/`, so the harness's own listing is
/// checked against something that does not share its code.
fn walk_fib_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("readable") {
        let path = entry.expect("entry").path();
        if path.is_dir() {
            walk_fib_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "fib") {
            out.push(path);
        }
    }
}

#[test]
fn the_harness_lists_exactly_the_fib_files_on_disk() {
    let mut on_disk = Vec::new();
    walk_fib_files(Path::new(SUITE_DIR), &mut on_disk);
    on_disk.sort();
    assert_eq!(real_cases(), on_disk);
}

#[test]
fn every_real_case_header_parses() {
    let mut errors = Vec::new();
    for path in real_cases() {
        if let Err(e) = read_header(&path) {
            errors.push(e.to_string());
        }
    }
    assert!(errors.is_empty(), "header errors:\n{}", errors.join("\n"));
}

#[test]
fn the_ownership_directory_holds_cases_1_to_last_less_the_withdrawn() {
    // 30 and 35 were withdrawn when D1 removed field places; 81 to 95
    // are the promoted findings of the rule-4 adversary, 96 to 99 those
    // of the rule-5 generator, 100 the annotated bindings of syntax §1.5,
    // 101 onwards the owner's decisions of 2026-09-28 (types §10).
    // The listing is by name, so 100 sorts after 10: compare as numbers.
    let ownership = Path::new(CASES_DIR).join("ownership");
    let cases = list_cases_recursive(&ownership).unwrap();
    let mut numbers: Vec<u32> = cases.iter().map(|p| number_of(p)).collect();
    numbers.sort_unstable();
    let expected: Vec<u32> = (1..=LAST).filter(|n| ![30, 35].contains(n)).collect();
    assert_eq!(numbers, expected);
}

#[test]
fn real_case_verdicts_agree_with_the_readme() {
    // README: REJECT must be rejected, TRAP must trap; 15 and 80 are
    // the permitted cycle leaks; every other case is accept with a
    // clean audit.
    for path in real_cases() {
        let header = read_header(&path).unwrap_or_else(|e| panic!("{e}"));
        let n = number_of(&path);
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        match header.verdict {
            Verdict::Reject { error } => {
                assert!(REJECT.contains(&n), "{name} is reject, not in REJECT");
                assert!(!error.is_empty());
            }
            Verdict::Trap { trap } => {
                assert!(TRAP.contains(&n), "{name} is trap, not in TRAP");
                assert!(!trap.is_empty());
            }
            Verdict::Accept { audit, .. } => {
                assert!(
                    !REJECT.contains(&n) && !TRAP.contains(&n),
                    "{name} is accept but README says reject or trap"
                );
                let expected = if LEAK_CYCLE.contains(&n) {
                    AuditExpect::LeakCycle
                } else {
                    AuditExpect::Clean
                };
                assert_eq!(audit, expected, "{name}");
            }
        }
        assert!(!header.spec.is_empty(), "{name} has an empty spec");
    }
}

#[test]
fn reject_case_names_say_reject_and_others_do_not() {
    for path in real_cases() {
        let header = read_header(&path).unwrap_or_else(|e| panic!("{e}"));
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let says_reject = name.contains("reject");
        let is_reject = matches!(header.verdict, Verdict::Reject { .. });
        assert_eq!(says_reject, is_reject, "{name}: name and verdict disagree");
        let says_trap = name.contains("-trap-");
        let is_trap = matches!(header.verdict, Verdict::Trap { .. });
        assert_eq!(says_trap, is_trap, "{name}: name and trap verdict disagree");
    }
}

#[test]
fn real_cases_run_with_no_header_errors_and_nothing_passes_yet() {
    let ownership = Path::new(CASES_DIR).join("ownership");
    let report = run_dir(&ownership, &PendingEvaluator).unwrap();
    assert_eq!(report.counts.header_error, 0, "{:?}", report.results);
    assert_eq!(report.counts.fail, 0);
    assert_eq!(
        report.counts.pass, 0,
        "nothing can pass without an interpreter"
    );
    assert_eq!(report.counts.pending, report.counts.total());
    assert!(report.ok());
}
