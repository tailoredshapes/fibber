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
/// a directory of its own, which the caller removes. The header's `roots`
/// are relative to the case's directory, which the copy is not in, so they
/// are made absolute first.
fn with_bound(path: &Path, max: u64, tag: &str) -> (PathBuf, PathBuf) {
    let source = fs::read_to_string(path).expect("the case");
    let old = format!("allocs: <= {}", bound(path));
    assert!(source.contains(&old), "{old} is in the header");
    let dir = std::env::temp_dir().join(format!("fibc-allocs-{}-{tag}", std::process::id()));
    fs::create_dir_all(&dir).expect("temp dir");
    let out = dir.join(path.file_name().expect("a file name"));
    let moved = absolute_roots(&source, path.parent().expect("a directory"));
    fs::write(&out, moved.replace(&old, &format!("allocs: <= {max}"))).expect("write");
    (out, dir)
}

/// `source` with the directories of its `;; roots:` line joined to `base`.
fn absolute_roots(source: &str, base: &Path) -> String {
    let rewrite = |line: &str| match line.strip_prefix(";; roots:") {
        Some(dirs) => {
            let dirs: Vec<String> = dirs
                .split_whitespace()
                .map(|d| base.join(d).display().to_string())
                .collect();
            format!(";; roots: {}", dirs.join(" "))
        }
        None => line.to_string(),
    };
    let lines: Vec<String> = source.lines().map(rewrite).collect();
    lines.join("\n") + "\n"
}

/// The `cases/stdlib/NNN-count-*.fib` files: the cases whose header
/// carries a measured `allocs` bound (cases/stdlib/README.md).
fn stdlib_count_cases() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../cases/stdlib");
    let mut found: Vec<PathBuf> = fs::read_dir(&dir)
        .expect("the cases directory")
        .map(|e| e.expect("an entry").path())
        .filter(|p| {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let slug = name.trim_start_matches(|c: char| c.is_ascii_digit());
            slug.starts_with("-count-") && name.ends_with(".fib")
        })
        .collect();
    found.sort();
    assert!(
        !found.is_empty(),
        "no count- case: the test would prove nothing"
    );
    found
}

/// The bound as written passes, one below it fails and names the count,
/// one above it passes: the check fires, and the bound is the count. A
/// case that allocates nothing has no bound below its count (`0 - 1` is
/// no bound): the lowered half is skipped for it, and the fixtures of
/// `a_bound_of_zero_...` show that a bound of 0 can fail.
fn bound_is_exact(h: &Harness, path: &Path, label: &str) {
    let n = bound(path);
    assert_eq!(h.run_case(path).status, Status::Pass, "case {label}");
    if let Some(below) = n.checked_sub(1) {
        let (low, dir) = with_bound(path, below, label);
        let lowered = h.run_case(&low).status;
        let _ = fs::remove_dir_all(&dir);
        match lowered {
            Status::Fail(why) => {
                for needle in [format!("at most {below}"), format!("allocated {n}")] {
                    assert!(
                        why.contains(&needle),
                        "case {label}: {why:?} lacks {needle:?}"
                    );
                }
            }
            other => panic!("case {label} with the bound {below}: {other:?}"),
        }
    }
    let (high, dir) = with_bound(path, n + 1, label);
    let raised = h.run_case(&high).status;
    let _ = fs::remove_dir_all(&dir);
    assert_eq!(
        raised,
        Status::Pass,
        "case {label} with the bound {}",
        n + 1
    );
}

/// A fixture case of `main`'s source with the bound `<= 0`, written to a
/// directory of its own, which the caller removes.
fn zero_bound_fixture(body: &str, tag: &str) -> (PathBuf, PathBuf) {
    let dir = std::env::temp_dir().join(format!("fibc-allocs-zero-{}-{tag}", std::process::id()));
    fs::create_dir_all(&dir).expect("temp dir");
    let out = dir.join(format!("900-{tag}.fib"));
    let header = ";; spec:   method.md rule 3\n;; expect: accept\n;; result: 3\n;; audit:  clean\n;; allocs: <= 0\n";
    fs::write(&out, format!("{header}{body}\n")).expect("write");
    (out, dir)
}

/// A bound of 0 is a bound: a case that allocates nothing passes it (so
/// a case of that kind is not an underflow in `bound_is_exact`), and a
/// program that allocates one object fails it, naming the count.
#[test]
fn a_bound_of_zero_passes_when_nothing_allocates_and_fails_when_something_does() {
    let h = harness();
    let (quiet, dir) = zero_bound_fixture("(defun main () -> i64 (+ 1 2))", "nothing");
    // The same check the library's `count-` cases go through: for a
    // bound of 0 it has nothing to lower, and must not underflow.
    bound_is_exact(&h, &quiet, "nothing");
    let _ = fs::remove_dir_all(&dir);
    let (loud, dir) = zero_bound_fixture(
        "(defun main () -> i64 (str-len (str-concat \"ab\" \"c\")))",
        "one-string",
    );
    let loud_status = h.run_case(&loud).status;
    let _ = fs::remove_dir_all(&dir);
    match loud_status {
        Status::Fail(why) => {
            for needle in ["at most 0", "allocated 1"] {
                assert!(why.contains(needle), "{why:?} lacks {needle:?}");
            }
        }
        other => panic!("a program that allocates, under the bound 0: {other:?}"),
    }
}

#[test]
fn a_bound_one_below_the_count_fails() {
    let h = harness();
    for number in ["202", "203", "204"] {
        bound_is_exact(&h, &case(number), number);
    }
}

/// Method rule 3: a case that claims a tight bound has a test that lowers
/// it by one and requires the failure. Every `count-` case of the library
/// suite is held to it, compiled as well as interpreted.
#[test]
fn a_stdlib_count_bound_one_below_the_count_fails() {
    let h = harness();
    for path in stdlib_count_cases() {
        let label = path
            .file_name()
            .expect("a name")
            .to_string_lossy()
            .into_owned();
        bound_is_exact(&h, &path, &label);
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

/// `fibc run --trace` and a `FIB_TRACE` set in the environment give one
/// trace, and neither counts what the compiler allocates when it
/// evaluates a `def` (compiler.md §4: static data has no ordinal). Case
/// 203 has a `def` Vec of 500 elements: with the variable alone the
/// def's objects used to be counted (1050 `A` lines against 4).
#[test]
fn the_environment_variable_and_the_flag_count_alike_and_defs_are_not_counted() {
    let path = case("203");
    let count = |flag: bool| {
        let mut run = std::process::Command::new(env!("CARGO_BIN_EXE_fibc"));
        run.arg("run").env_remove("FIB_TRACE");
        if flag {
            run.arg("--trace");
        } else {
            run.env("FIB_TRACE", "1");
        }
        let out = run.arg(&path).output().expect("fibc runs");
        let err = String::from_utf8_lossy(&out.stderr).into_owned();
        err.lines().filter(|l| l.starts_with("A ")).count() as u64
    };
    let (flag, env) = (count(true), count(false));
    assert_eq!(env, flag, "the variable and the flag disagree");
    assert!(
        flag > 0 && flag <= bound(&path),
        "{flag} against {}",
        bound(&path)
    );
}

/// `fibc itrace` writes the trace and nothing else to standard output;
/// the result and the audit go to standard error, after it.
#[test]
fn itrace_prints_the_trace_alone_on_standard_output() {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_fibc"))
        .arg("itrace")
        .arg(case("202"))
        .output()
        .expect("fibc runs");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.lines().count() > 0, "an empty trace proves nothing");
    for line in stdout.lines() {
        assert!(
            matches!(line.split(' ').next(), Some("A" | "F" | "S" | "D" | "T")),
            "not a trace line: {line:?}"
        );
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.starts_with("result: "), "{stderr:?}");
    assert!(stderr.contains("audit:  clean=true"), "{stderr:?}");
}
