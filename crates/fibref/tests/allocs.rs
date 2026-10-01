//! The header key `allocs` (spec/method.md rule 3) against the real
//! cases, through the interpreter: a bound that is the count passes, the
//! same file with the bound lowered by one fails and names the count, so
//! the check can fail and the bound is exact; and the pipeline cases
//! 202 and 203, the same program over 50 and over 500 elements, carry
//! one bound. The compiled side is `crates/fibc/tests/allocs.rs`.

use std::fs;
use std::path::{Path, PathBuf};

use fibref::cases::{parse_header, run_case, Status, Verdict};
use fibref::eval::Interpreter;

/// The one file `cases/ownership/NNN-*.fib`.
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

/// The maximum in the case's `allocs` header.
fn bound(path: &Path) -> u64 {
    let source = fs::read_to_string(path).expect("the case");
    match parse_header(path, &source).expect("a header").verdict {
        Verdict::Accept {
            allocs: Some(n), ..
        } => n,
        other => panic!("{}: no allocs bound: {other:?}", path.display()),
    }
}

/// The case's source with its `allocs` header changed to `<= max`,
/// written under the same file name in a directory of its own.
fn with_bound(path: &Path, max: u64, tag: &str) -> (PathBuf, PathBuf) {
    let source = fs::read_to_string(path).expect("the case");
    let old = format!("allocs: <= {}", bound(path));
    assert!(source.contains(&old), "{old} is in the header");
    let dir = std::env::temp_dir().join(format!("fibref-allocs-{}-{tag}", std::process::id()));
    fs::create_dir_all(&dir).expect("temp dir");
    let out = dir.join(path.file_name().expect("a file name"));
    fs::write(&out, source.replace(&old, &format!("allocs: <= {max}"))).expect("write");
    (out, dir)
}

fn status(path: &Path) -> Status {
    run_case(path, &Interpreter).status
}

#[test]
fn a_bound_one_below_the_count_fails() {
    for number in ["202", "203", "204"] {
        let path = case(number);
        let n = bound(&path);
        assert_eq!(status(&path), Status::Pass, "case {number} as written");
        let (low, dir) = with_bound(&path, n - 1, number);
        let lowered = status(&low);
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
        let raised = status(&high);
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(
            raised,
            Status::Pass,
            "case {number} with the bound {}",
            n + 1
        );
    }
}

#[test]
fn the_pipeline_over_ten_times_the_elements_has_the_same_bound() {
    let (small, large) = (case("202"), case("203"));
    assert_eq!(bound(&small), bound(&large));
    // The elements of the `def` Vec, which is the first bracketed literal.
    let elements = |path: &Path| {
        let source = fs::read_to_string(path).expect("the case");
        let start = source.find("(def xs").expect("the def");
        let open = start + source[start..].find('[').expect("a literal");
        let close = open + source[open..].find(']').expect("its end");
        source[open + 1..close].split_whitespace().count()
    };
    assert_eq!((elements(&small), elements(&large)), (50, 500));
}
