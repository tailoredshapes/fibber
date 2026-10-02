//! A project laid out as the repository is (`lib/`, `cases/stdlib/`) and a
//! stand-in for the tool, a shell script that judges a case by what the
//! module `lib/m.fib` holds. It prints what `fibref cases` prints.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use crate::fsutil::TempDir;
use crate::runner::{Tool, ToolKind};
use crate::session::Config;

/// The module under test: three sites of interest, one per mutant below.
pub const MODULE: &str =
    "(ns m)\n(defun f (a: i64) -> i64 (+ a 1))\n(defun g (a: i64) -> bool (< a 5))\n";

/// The stand-in. `$1` is `cases` or `run`; for `cases DIR --only NAME` the
/// answer is by NAME and by what the module holds:
///
/// | module holds | `a.fib` | `b.fib` |
/// |---|---|---|
/// | unreadable (ends in `(`) | FAIL | FAIL |
/// | `(- a 1)` | FAIL, result | FAIL, trap |
/// | `(<= a 5)` | pass | FAIL, trap |
/// | `(+ a 2)` | sleeps 30 s | pass |
///
/// `u.fib` never notices the module; `bad.fib` always fails. `run` rejects a
/// module holding `(< a 4)` and dies of a signal's status on `(< a 0)`.
/// Every case run is appended to the log.
fn script(log: &Path) -> String {
    format!(
        r#"#!/bin/sh
m="$FIB_LIB/m.fib"
has() {{ grep -qF -- "$1" "$m"; }}
pass() {{ printf 'case  status  detail\n%s  pass\n\n1 cases: 1 pass, 0 fail, 0 pending, 0 header error\n' "$1"; exit 0; }}
fail() {{ printf 'case  status  detail\n%s  FAIL    %s\n\n1 cases: 0 pass, 1 fail, 0 pending, 0 header error\n' "$1" "$2"; exit 1; }}
case "$1" in
run)
  has '(< a 4)' && {{ printf 'rejected:\n%s/m.fib:3:20: type error\n' "$FIB_LIB"; exit 1; }}
  has '(< a 0)' && exit 139
  printf 'result: 0\n'; exit 0 ;;
cases)
  c="$4"
  echo "$c" >> {log}
  [ "$c" = "bad.fib" ] && fail "$c" "result: expected 0, got 1"
  [ "$c" = "u.fib" ] && pass "$c"
  [ "$(tail -c 1 "$m")" = "(" ] && fail "$c" "expected accept, but rejected: unclosed"
  if [ "$c" = "a.fib" ]; then
    has '(- a 1)' && fail "$c" "result: expected 3, got 4"
    has '(+ a 2)' && sleep 30
  fi
  if [ "$c" = "b.fib" ]; then
    has '(- a 1)' && fail "$c" "the run trapped: boom at $FIB_LIB/m.fib:2"
    has '(<= a 5)' && fail "$c" "the run trapped: boom at $FIB_LIB/m.fib:3"
  fi
  pass "$c" ;;
esac
exit 2
"#,
        log = log.display()
    )
}

pub struct Project {
    pub dir: TempDir,
    pub log: PathBuf,
}

impl Project {
    /// The module, the cases `a b u bad`, and the stand-in.
    pub fn new(tag: &str) -> Project {
        let dir = TempDir::new(tag).unwrap();
        let p = dir.path();
        fs::create_dir_all(p.join("lib")).unwrap();
        fs::create_dir_all(p.join("cases/stdlib")).unwrap();
        fs::write(p.join("lib/m.fib"), MODULE).unwrap();
        for case in ["a", "b", "u", "bad"] {
            fs::write(p.join(format!("cases/stdlib/{case}.fib")), ";; spec: x\n").unwrap();
        }
        let log = p.join("calls.log");
        let tool = p.join("tool.sh");
        fs::write(&tool, script(&log)).unwrap();
        fs::set_permissions(&tool, fs::Permissions::from_mode(0o755)).unwrap();
        Project { dir, log }
    }

    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    pub fn tool(&self, timeout_s: f64) -> Tool {
        Tool {
            kind: ToolKind::Fibref,
            program: self.path().join("tool.sh"),
            vmem_kb: 4_000_000,
            timeout_s,
        }
    }

    /// A configuration over the whole project with every mutant chosen.
    pub fn config(&self, timeout_s: f64) -> Config {
        Config {
            module: self.path().join("lib/m.fib"),
            lib: self.path().join("lib"),
            cases: self.path().join("cases/stdlib"),
            only: Vec::new(),
            max: 1000,
            seed: 1,
            ops: Vec::new(),
            lines: None,
            tool: self.tool(timeout_s),
        }
    }

    /// The cases the stand-in was asked to run, in order.
    pub fn calls(&self) -> Vec<String> {
        fs::read_to_string(&self.log)
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    /// Every file under the project's `lib` and `cases` with its bytes.
    pub fn snapshot(&self) -> Vec<(PathBuf, Vec<u8>)> {
        fn walk(dir: &Path, out: &mut Vec<(PathBuf, Vec<u8>)>) {
            let mut entries: Vec<_> = fs::read_dir(dir)
                .unwrap()
                .map(|e| e.unwrap().path())
                .collect();
            entries.sort();
            for p in entries {
                if p.is_dir() {
                    walk(&p, out);
                } else {
                    out.push((p.clone(), fs::read(&p).unwrap()));
                }
            }
        }
        let mut out = Vec::new();
        walk(&self.path().join("lib"), &mut out);
        walk(&self.path().join("cases"), &mut out);
        out
    }
}

impl Project {
    /// Replaces the module under test.
    pub fn set_module(&self, text: &str) {
        fs::write(self.path().join("lib/m.fib"), text).unwrap();
    }
}
