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
