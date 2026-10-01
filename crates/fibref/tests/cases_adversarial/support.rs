//! Shared helpers: a temporary directory that is unique per test and
//! removed on drop, and an evaluator scripted by the case source, so
//! that every verdict can be forced without an interpreter.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use fibref::cases::{AuditSummary, Evaluator, Outcome, Value};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// A fresh directory under the system temp dir, unique across tests and
/// across concurrent test binaries, removed on drop.
pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new(name: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!(
            "fibref-adversarial-{}-{nanos}-{n}-{name}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        TempDir(path)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    /// Writes `contents` at `name` (which may contain subdirectories).
    pub fn write(&self, name: &str, contents: &str) -> PathBuf {
        self.write_bytes(name, contents.as_bytes())
    }

    pub fn write_bytes(&self, name: &str, contents: &[u8]) -> PathBuf {
        let path = self.0.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, contents).unwrap();
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Every `.fib` file under `dir`, in no order: a walk of the directory
/// that shares no code with the harness's own listing, so that this is
/// something to check that listing against.
pub fn walk_fib_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("readable") {
        let path = entry.expect("entry").path();
        if path.is_dir() {
            walk_fib_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "fib") {
            out.push(path);
        }
    }
}

/// How many cases `cases/ownership` holds now. The tests that run all of
/// them ask this and do not hold the number, which a new case would
/// change in several files; a case that is lost is the business of
/// `real_cases` (the numbers are 1 up to the last, less the withdrawn).
pub fn ownership_case_count() -> usize {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../cases/ownership");
    let mut files = Vec::new();
    walk_fib_files(&dir, &mut files);
    assert!(
        files.len() > 100,
        "{} cases under {}",
        files.len(),
        dir.display()
    );
    files.len()
}

/// An evaluator scripted by a directive somewhere in the source:
///
/// - `(scripted-compiled N)`: `Compiled` returning `N` with a clean audit
/// - `(scripted-leak-cycle N)`: `Compiled` returning `N` with one leak cycle
/// - `(scripted-reject MESSAGE)`: `Rejected` with the rest of the line
/// - anything else: `Unsupported`
///
/// The directive is deliberately unlike anything in a header line so
/// the header parser cannot see it.
pub struct Scripted;

impl Evaluator for Scripted {
    fn run(&self, source: &str) -> Outcome {
        for line in source.lines() {
            if let Some(rest) = line.strip_prefix("(scripted-compiled ") {
                return compiled(rest, AuditSummary::clean());
            }
            if let Some(rest) = line.strip_prefix("(scripted-leak-cycle ") {
                let audit = AuditSummary {
                    clean: false,
                    leak_cycles: 1,
                    leaks: 0,
                    errors: vec![],
                };
                return compiled(rest, audit);
            }
            if let Some(rest) = line.strip_prefix("(scripted-reject ") {
                return Outcome::Rejected {
                    message: rest.trim_end_matches(')').to_string(),
                };
            }
        }
        Outcome::Unsupported {
            reason: "scripted: no directive".to_string(),
        }
    }
}

fn compiled(rest: &str, audit: AuditSummary) -> Outcome {
    let digits = rest.trim_end_matches(')').trim();
    Outcome::Compiled {
        result: Value::Int(digits.parse().unwrap()),
        audit,
    }
}

/// A valid accept header expecting `result` with a clean audit.
pub fn accept_header(result: i64) -> String {
    format!(";; spec: §4\n;; expect: accept\n;; result: {result}\n;; audit: clean\n")
}

/// A valid reject header expecting `error`.
pub fn reject_header(error: &str) -> String {
    format!(";; spec: §5\n;; expect: reject\n;; error: {error}\n")
}

/// An accept case whose program returns `result` with a clean audit.
pub fn accept_case(result: i64) -> String {
    format!("{}(defun main () -> i64 {result})\n", accept_header(result))
}

/// A reject case whose program the checker rejects with `unbound name
/// nope`.
pub fn reject_case() -> String {
    format!(
        "{}(defun main () -> i64 (nope))\n",
        reject_header("unbound name nope")
    )
}
