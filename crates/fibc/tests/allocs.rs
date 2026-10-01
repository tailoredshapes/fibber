//! The header key `allocs` (spec/method.md rule 3) through the rule-6
//! harness, which counts the `A` lines of the compiled run's
//! `FIB_TRACE=1` trace: a bound that is the count passes, the same file
//! with the bound lowered by one fails and names the count; and the two
//! tools count alike, case by case. The interpreter's side alone is
//! `crates/fibref/tests/allocs.rs`.

use std::fs;
use std::path::{Path, PathBuf};

use fibc::harness::Harness;
use fibref::cases::{parse_header, Evaluator, Outcome, Status, Verdict};
use fibref::eval::Interpreter;

fn harness() -> Harness {
    Harness {
        fibc: PathBuf::from(env!("CARGO_BIN_EXE_fibc")),
    }
}

/// The one file `cases/ownership/NNN-*.fib` (or `N-*`).
fn case(number: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../cases/ownership");
    let mut found: Vec<PathBuf> = fs::read_dir(&dir)
        .expect("the cases directory")
        .map(|e| e.expect("an entry").path())
        .filter(|p| {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            name.starts_with(&format!("{number}-")) && name.ends_with(".fib")
        })
        .collect();
    assert_eq!(found.len(), 1, "case {number}: {found:?}");
    found.remove(0)
}

fn bound(path: &Path) -> u64 {
    let source = fs::read_to_string(path).expect("the case");
    match parse_header(path, &source).expect("a header").verdict {
        Verdict::Accept {
            allocs: Some(n), ..
        } => n,
        other => panic!("{}: no allocs bound: {other:?}", path.display()),
    }
}

/// The case with its bound changed to `max`, under the same file name in
/// a directory of its own, which the caller removes.
fn with_bound(path: &Path, max: u64, tag: &str) -> (PathBuf, PathBuf) {
    let source = fs::read_to_string(path).expect("the case");
    let old = format!("allocs: <= {}", bound(path));
    assert!(source.contains(&old), "{old} is in the header");
    let dir = std::env::temp_dir().join(format!("fibc-allocs-{}-{tag}", std::process::id()));
    fs::create_dir_all(&dir).expect("temp dir");
    let out = dir.join(path.file_name().expect("a file name"));
    fs::write(&out, source.replace(&old, &format!("allocs: <= {max}"))).expect("write");
    (out, dir)
}

#[test]
fn a_bound_one_below_the_count_fails() {
    let h = harness();
    for number in ["202", "203", "204"] {
        let path = case(number);
        let n = bound(&path);
        assert_eq!(h.run_case(&path).status, Status::Pass, "case {number}");
        let (low, dir) = with_bound(&path, n - 1, number);
        let lowered = h.run_case(&low).status;
        let _ = fs::remove_dir_all(&dir);
        match lowered {
            Status::Fail(why) => {
                for needle in [format!("at most {}", n - 1), format!("allocated {n}")] {
                    assert!(
                        why.contains(&needle),
                        "case {number}: {why:?} lacks {needle:?}"
                    );
                }
            }
            other => panic!("case {number} with the bound {}: {other:?}", n - 1),
        }
        let (high, dir) = with_bound(&path, n + 1, number);
        let raised = h.run_case(&high).status;
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(
            raised,
            Status::Pass,
            "case {number} with the bound {}",
            n + 1
        );
    }
}

/// The compiled run's `A` lines and the interpreter's events give one
/// number, on single-threaded cases that allocate in different ways:
/// closures, a `Vec`, a `Map`, strings, a `def`, derived instances. A
/// threaded run is left out: compiler.md §4 lets the compiled program
/// allocate more there (a `swap!` that retries), and case 10, which
/// `pmap`s, is the example: 11383 against the interpreter's 11381.
#[test]
fn the_two_tools_count_alike() {
    let h = harness();
    let numbers = [
        "01", "02", "03", "05", "95", "169", "177", "180", "198", "199", "200", "201", "202",
        "203", "204",
    ];
    for number in numbers {
        let path = case(number);
        let source = fs::read_to_string(&path).expect("the case");
        let (outcome, compiled) = h.counted(&path, &source);
        assert!(
            matches!(outcome, Outcome::Compiled { .. }),
            "case {number}: {outcome:?}"
        );
        let (_, interpreted) = Interpreter.run_counted(&source, &path);
        assert!(
            matches!(compiled, Some(n) if n > 0),
            "case {number}: the compiled count {compiled:?}; a case that allocates nothing proves nothing"
        );
        assert_eq!(compiled, interpreted, "case {number}");
    }
}
