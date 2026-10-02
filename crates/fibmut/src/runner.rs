//! Running a tool on one case, one process at a time, and reading what it
//! says.
//!
//! The tool is `fibref` or `fibc`; a case is judged by the tool's own
//! `cases DIR --only CASE` (so the verdict is the harness's, never a second
//! reading of the header). Every process runs under `ulimit -v`, `timeout`
//! and `nice`, in the working copy's directory, with `FIB_LIB` naming the
//! working copy's library, so that nothing a mutant does reaches the real
//! `lib/` and a case without a `roots` header still loads the copy.

use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

/// Applies the limits and then becomes the tool: `$1` kilobytes of address
/// space (0 for none), `$2` seconds, then the program and its arguments.
const WRAPPER: &str = r#"[ "$1" = 0 ] || ulimit -v "$1" || exit 125
t="$2"; shift 2
exec timeout -k 2 "$t" nice -n 10 "$@""#;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolKind {
    Fibref,
    Fibc,
}

impl ToolKind {
    pub fn name(self) -> &'static str {
        match self {
            ToolKind::Fibref => "fibref",
            ToolKind::Fibc => "fibc",
        }
    }
}

/// The program to run and the limits to run it under.
#[derive(Debug, Clone)]
pub struct Tool {
    pub kind: ToolKind,
    pub program: PathBuf,
    pub vmem_kb: u64,
    pub timeout_s: f64,
}

/// Where a run happens: the working copy's root, which is the current
/// directory of every process, its library, and the cases directory
/// below the root.
#[derive(Debug, Clone)]
pub struct Sandbox {
    pub root: PathBuf,
    pub lib: PathBuf,
    pub cases: PathBuf,
}

/// How a case ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ending {
    Pass,
    /// Listed, and neither a pass nor a failure: an `open-` case, or pending.
    NotPass(String),
    /// The verdict did not hold. `class` says how (see `classify`), `detail`
    /// is the harness's own words.
    Fail {
        class: &'static str,
        detail: String,
    },
    Timeout,
    /// The tool died or answered in no way the harness does.
    Crash(String),
    /// The header of the case does not parse: it cannot judge anything.
    BadHeader(String),
}

/// How the compile check of a mutant ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Check {
    Compiles,
    Invalid(String),
    Timeout,
    Crash(String),
}

struct Exec {
    code: Option<i32>,
    stdout: String,
    stderr: String,
    timed_out: bool,
}

impl Tool {
    fn exec(&self, sb: &Sandbox, args: &[&str]) -> io::Result<Exec> {
        let started = Instant::now();
        let out = Command::new("sh")
            .arg("-c")
            .arg(WRAPPER)
            .arg("sh")
            .arg(self.vmem_kb.to_string())
            .arg(self.timeout_s.to_string())
            .arg(&self.program)
            .args(args)
            .current_dir(&sb.root)
            .env("FIB_LIB", &sb.lib)
            .stdin(Stdio::null())
            .output()?;
        let code = out.status.code();
        // 124 is `timeout`'s own answer; 137 is the KILL it sends after the
        // grace period, which is a timeout only when the time was up.
        let late = started.elapsed().as_secs_f64() >= self.timeout_s;
        Ok(Exec {
            code,
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
            timed_out: code == Some(124) || (code == Some(137) && late),
        })
    }

    /// Runs the case with the file name or directory name `arg`. An `Err` is a
    /// failure of the setup (the tool cannot be run, it was misused), which no
    /// mutant explains.
    pub fn run_case(&self, sb: &Sandbox, arg: &str) -> Result<Ending, String> {
        let cases = sb.cases.to_string_lossy();
        let e = self
            .exec(sb, &["cases", &cases, "--only", arg])
            .map_err(|e| self.spawn_error(&e))?;
        classify_case(&e)
    }

    /// Runs a program that only loads the module under test: a module that
    /// does not compile is invalid, whatever the cases would have said.
    pub fn check(&self, sb: &Sandbox, main: &Path) -> Result<Check, String> {
        let main = main.to_string_lossy();
        let e = self
            .exec(sb, &["run", &main])
            .map_err(|e| self.spawn_error(&e))?;
        Ok(match e.code {
            _ if e.timed_out => Check::Timeout,
            Some(0) => Check::Compiles,
            Some(1) => Check::Invalid(first_lines(
                e.stdout.strip_prefix("rejected:").unwrap_or(&e.stdout),
                2,
            )),
            Some(125..=127) | Some(2) => return Err(setup_error(&e)),
            _ => Check::Crash(crash_text(&e)),
        })
    }

    fn spawn_error(&self, e: &io::Error) -> String {
        format!("cannot run {}: {e}", self.program.display())
    }
}

fn setup_error(e: &Exec) -> String {
    let said = if e.stderr.trim().is_empty() {
        &e.stdout
    } else {
        &e.stderr
    };
    format!(
        "the tool did not run (exit {:?}): {}",
        e.code,
        first_lines(said, 2)
    )
}

fn crash_text(e: &Exec) -> String {
    format!("exit {:?}: {}", e.code, first_lines(&e.stderr, 2))
}

/// The first `n` non-empty lines of `text`, joined by ` | `.
fn first_lines(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .take(n)
        .collect();
    lines.join(" | ")
}

/// What the line `N cases: P pass, F fail, ..` of a run says.
#[derive(Debug, Default, PartialEq, Eq)]
struct Counts {
    total: usize,
    pass: usize,
}

fn parse_counts(stdout: &str) -> Option<Counts> {
    let line = stdout.lines().find(|l| l.contains(" cases: "))?;
    let (total, rest) = line.split_once(" cases: ")?;
    let pass = rest
        .split(", ")
        .find_map(|part| part.strip_suffix(" pass"))
        .and_then(|n| n.trim().parse().ok())?;
    Some(Counts {
        total: total.trim().parse().ok()?,
        pass,
    })
}

/// The row of the table, which is its second line, on one line.
fn row(stdout: &str) -> String {
    let row = stdout.lines().nth(1).unwrap_or_default();
    row.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The detail of the row that says FAIL.
fn fail_detail(stdout: &str) -> String {
    stdout
        .lines()
        .find_map(|l| {
            l.split_once("  FAIL  ")
                .map(|(_, detail)| detail.trim().to_string())
        })
        .unwrap_or_else(|| first_lines(stdout, 3))
}

fn classify_case(e: &Exec) -> Result<Ending, String> {
    if e.timed_out {
        return Ok(Ending::Timeout);
    }
    match e.code {
        Some(0) => match parse_counts(&e.stdout) {
            Some(Counts { total: 1, pass: 1 }) => Ok(Ending::Pass),
            Some(Counts { total: 1, .. }) => Ok(Ending::NotPass(row(&e.stdout))),
            _ => Err(format!(
                "the tool did not run exactly one case: {}",
                first_lines(&e.stdout, 3)
            )),
        },
        Some(1) => {
            let detail = fail_detail(&e.stdout);
            if e.stdout.contains("  HEADER") {
                return Ok(Ending::BadHeader(row(&e.stdout)));
            }
            Ok(Ending::Fail {
                class: classify(&detail),
                detail,
            })
        }
        Some(2) | Some(125..=127) => Err(setup_error(e)),
        _ => Ok(Ending::Crash(crash_text(e))),
    }
}

/// How a failed verdict failed, from the harness's words
/// (`crates/fibref/src/cases/verdict.rs`): the result (or the verdict
/// itself), an audit, an allocation bound, a trap, the program being
/// refused, or the run failing. The start of the detail says which kind of
/// failure it is, so a message quoted inside it cannot change the class.
pub fn classify(detail: &str) -> &'static str {
    let starts = |s: &str| detail.starts_with(s);
    if starts("expected accept, but rejected") {
        "compile"
    } else if starts("the run trapped") || starts("expected a trap") || starts("trapped, but") {
        "trap"
    } else if starts("the run failed")
        || starts("expected reject with") && detail.contains("run failed")
    {
        "failed"
    } else if starts("expected reject")
        || starts("rejected, but")
        || detail.contains("result: expected")
    {
        "result"
    } else if detail.contains("audit: expected") {
        "audit"
    } else if detail.contains("allocs:") {
        "allocs"
    } else {
        "other"
    }
}

#[cfg(test)]
mod tests;
