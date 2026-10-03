//! Every real file under `cases/`, recursively: each header must parse,
//! and the verdicts must agree with what `cases/ownership/README.md`
//! says about them. This is what CI runs against the actual cases.

use std::path::{Path, PathBuf};

use fibref::cases::{
    list_cases_recursive, read_header, run_dir, AuditExpect, PendingEvaluator, Verdict,
};

use super::support::walk_fib_files;

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
    12, 13, 14, 18, 21, 34, 40, 82, 90, 93, 105, 107, 108, 109, 112, 113, 114, 118, 119, 121, 123,
    125, 126, 127, 138, 139, 142, 145, 146, 147, 155, 157, 159, 196, 242, 246, 247,
];
const LEAK_CYCLE: [u32; 2] = [15, 80];
/// The cases whose verdict is a run-time trap (method.md rule 3).
const TRAP: [u32; 16] = [
    101, 102, 103, 104, 172, 173, 184, 195, 209, 212, 214, 215, 216, 217, 224, 234,
];
/// Numbers below the last that no case has: 30 and 35 were withdrawn when
/// D1 removed field places, 191 was never written, and 219 was taken by
/// the stage-1 fixes of the library work before they were renumbered. (205 was reserved
/// for the standard library's hygiene package, R2; its case is
/// `205-show-of-option-and-list`.)
const WITHDRAWN: [u32; 4] = [30, 35, 191, 219];
/// The last case number that existed when this was written. The suite
/// only grows, so a directory whose highest number is lower has lost its
/// last cases, which the contiguity of the numbers below it cannot show.
const LAST_KNOWN: u32 = 195;

fn number_of(path: &Path) -> u32 {
    let name = path.file_name().unwrap().to_string_lossy();
    name.split('-')
        .next()
        .and_then(|n| n.parse().ok())
        .unwrap_or_else(|| panic!("{name} does not start with a number"))
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
    // 191 was never written ((args) with a word that is not UTF-8 needs a command
    // line, which the case harness does not give; the tests of
    // crates/fibc/tests/cli/args.rs and crates/fibref/tests/run_io.rs
    // are its evidence, and case 186 has the empty (args)); 81 to 95
    // are the promoted findings of the rule-4 adversary, 96 to 99 those
    // of the rule-5 generator, 100 the annotated bindings of syntax §1.5,
    // 101 to 127 the owner's decisions of 2026-09-28 lifting the v1
    // restrictions, 128 to 149 vector patterns and guards, 150 to 153 the
    // copy-in at call entry, 154 the built-in float comparisons, 155 to
    // 161 colour parameters in impl heads, 162 to 165 no forwarding of a
    // captured & parameter (types §10), 166 to 168 the fair executor's
    // spin-waits and swap! contention (types §8.8), 169 the native Show
    // and Hash instances on scalars (types §2.12), 170 to 173 the findings
    // of fibc gen (compiler.md §5) pinned as cases, 174 to 176 the state
    // machine of an async body and its executor (types §8.8), 177 a dyn
    // over a native instance (types §8.5), 178 the texts of show on floats
    // and str (types §2.12), 179 to 182 the prelude's Map and Set (M5),
    // 183 and 184 str-from-bytes, str-join, str-chars and char->str (M5),
    // 185 and 186 read-file, write-file, args and println (M5), 187 the
    // digits of show on floats, 188 the read errors of read-file, 189 the
    // strtod and strtof externs in the interpreter, 190 (alloc) memory
    // zeroed, 192 the normal path of println and its loop of writes, 193 a
    // raw ptr uncounted in every container that can hold one (types §8.1),
    // 194 the bit casts of a float (f64->bits, bits->f64, f32->bits,
    // bits->f32), 195 (alloc) of a block that cannot be had traps.
    // The last number is the highest on disk (and at least LAST_KNOWN), so
    // a new case changes nothing here; a lost one is a gap, or a highest
    // number below LAST_KNOWN.
    // The listing is by name, so 100 sorts after 10: compare as numbers.
    let ownership = Path::new(CASES_DIR).join("ownership");
    let cases = list_cases_recursive(&ownership).unwrap();
    let mut numbers: Vec<u32> = cases.iter().map(|p| number_of(p)).collect();
    numbers.sort_unstable();
    let last = *numbers.last().expect("the directory holds cases");
    assert!(
        last >= LAST_KNOWN,
        "the highest case is {last}, and {LAST_KNOWN} existed: cases were lost"
    );
    let expected: Vec<u32> = (1..=last).filter(|n| !WITHDRAWN.contains(n)).collect();
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

/// The programs of several modules (syntax §5) under cases/modules,
/// each its directory's main.fib, all pass in the reference interpreter.
#[test]
fn the_module_cases_all_pass_in_the_interpreter() {
    let modules = Path::new(CASES_DIR).join("modules");
    let report = run_dir(&modules, &fibref::eval::Interpreter).unwrap();
    let rows: Vec<String> = report
        .results
        .iter()
        .map(|r| format!("{} {}", r.name(), r.status.label()))
        .collect();
    assert!(report.counts.total() >= 6, "{rows:?}");
    assert!(report.ok() && report.counts.pending == 0, "{rows:?}");
    assert!(
        rows[0].starts_with("001-require-alias-and-use/main.fib"),
        "{rows:?}"
    );
}
