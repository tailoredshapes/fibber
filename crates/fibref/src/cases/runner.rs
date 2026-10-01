//! Running every case in a directory and collecting the results.
//!
//! [`run_dir`] takes the `*.fib` files directly inside a directory in
//! file-name order, parses each header, runs the source through the
//! evaluator and judges the outcome. The [`Report`] keeps every result
//! and counts them; [`Report::ok`] is what the exit code follows: no
//! Fail and no HeaderError. Pending cases are counted separately and
//! never make a report pass.

use std::io;
use std::path::{Path, PathBuf};

use super::evaluator::Evaluator;
use super::header::{case_roots, parse_header, HeaderError, HeaderErrorKind};
use super::verdict::{judge_counted, Status};
use crate::roots::Roots;

/// The extension of a case file.
const CASE_EXTENSION: &str = "fib";

/// The status of one case file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaseResult {
    /// The case file.
    pub path: PathBuf,
    /// How it fared.
    pub status: Status,
}

impl CaseResult {
    /// The file name, for reports.
    pub fn name(&self) -> String {
        let file = self
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.path.display().to_string());
        // A program of several modules is its directory's main.fib.
        match self.path.parent().and_then(Path::file_name) {
            Some(dir) if file == "main.fib" => format!("{}/main.fib", dir.to_string_lossy()),
            _ => file,
        }
    }
}

/// How many cases ended in each status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Counts {
    pub pass: usize,
    pub fail: usize,
    pub pending: usize,
    pub header_error: usize,
}

impl Counts {
    fn add(&mut self, status: &Status) {
        match status {
            Status::Pass => self.pass += 1,
            Status::Fail(_) => self.fail += 1,
            Status::Pending(_) => self.pending += 1,
            Status::HeaderError(_) => self.header_error += 1,
        }
    }

    /// The number of cases counted.
    pub fn total(&self) -> usize {
        self.pass + self.fail + self.pending + self.header_error
    }
}

/// Every result from one run, in file-name order, with the counts.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Report {
    pub results: Vec<CaseResult>,
    pub counts: Counts,
}

impl Report {
    /// Builds a report from results, counting them.
    pub fn from_results(results: Vec<CaseResult>) -> Self {
        let mut counts = Counts::default();
        for result in &results {
            counts.add(&result.status);
        }
        Report { results, counts }
    }

    /// True when at least one case was found, nothing failed and every
    /// header parsed. Pending cases do not make this false, but they
    /// are not passes either. A run over zero cases is never ok: a
    /// wrong directory must not read as green.
    pub fn ok(&self) -> bool {
        self.counts.total() > 0 && self.counts.fail == 0 && self.counts.header_error == 0
    }
}

/// Runs one case file. An unreadable file or a bad header is a
/// [`Status::HeaderError`]; nothing here panics.
pub fn run_case(path: &Path, evaluator: &dyn Evaluator) -> CaseResult {
    let status = match std::fs::read_to_string(path) {
        Err(e) => Status::HeaderError(HeaderError {
            path: path.to_path_buf(),
            line: 0,
            kind: HeaderErrorKind::Unreadable(e.to_string()),
        }),
        Ok(source) => match parse_header(path, &source) {
            Err(e) => Status::HeaderError(e),
            Ok(header) => match case_roots(path, &source) {
                Err(e) => Status::HeaderError(e),
                Ok(dirs) => {
                    let roots = Roots::new(dirs);
                    let (outcome, allocs) = evaluator.run_counted_in(&source, path, &roots);
                    judge_counted(&header, &outcome, allocs)
                }
            },
        },
    };
    CaseResult {
        path: path.to_path_buf(),
        status,
    }
}

/// Runs every `*.fib` file directly inside `dir`, in file-name order.
/// Fails only when the directory itself cannot be listed.
pub fn run_dir(dir: &Path, evaluator: &dyn Evaluator) -> io::Result<Report> {
    let results = list_cases(dir)?
        .iter()
        .map(|path| run_case(path, evaluator))
        .collect();
    Ok(Report::from_results(results))
}

/// The `*.fib` files directly inside `dir`, and the `main.fib` of each
/// subdirectory that has one (a program of several modules, syntax
/// §5), sorted by name.
pub fn list_cases(dir: &Path) -> io::Result<Vec<PathBuf>> {
    let mut cases: Vec<PathBuf> = std::fs::read_dir(dir)?
        .map(|entry| entry.map(|e| e.path()))
        .filter_map(|path| match path {
            Err(e) => Some(Err(e)),
            Ok(p) if is_case(&p) => Some(Ok(p)),
            Ok(p) if p.is_dir() && p.join("main.fib").is_file() => Some(Ok(p.join("main.fib"))),
            Ok(_) => None,
        })
        .collect::<io::Result<_>>()?;
    cases.sort_by_key(|p| {
        let name = p.file_name().map(|n| n.to_os_string());
        if p.file_name().is_some_and(|n| n == "main.fib") {
            p.parent()
                .and_then(|d| d.file_name())
                .map(|n| n.to_os_string())
        } else {
            name
        }
    });
    Ok(cases)
}

/// Every `*.fib` file under `dir`, at any depth, sorted by path; a
/// directory with a `main.fib` is one case, that file, and its other
/// files and subdirectories are that program's modules (syntax §5).
pub fn list_cases_recursive(dir: &Path) -> io::Result<Vec<PathBuf>> {
    let mut cases = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(current) = pending.pop() {
        let main = current.join("main.fib");
        if main.is_file() {
            cases.push(main);
            continue;
        }
        for entry in std::fs::read_dir(&current)? {
            let path = entry?.path();
            if path.is_dir() {
                pending.push(path);
            } else if is_case(&path) {
                cases.push(path);
            }
        }
    }
    cases.sort();
    Ok(cases)
}

/// An entry named `*.fib` is a case unless it is a directory. This is
/// deliberately not `is_file()`: a dangling link, or any other entry
/// that cannot be read, must still be listed so that [`run_case`]
/// reports it as an unreadable header error, instead of the run passing
/// with one case fewer.
fn is_case(path: &Path) -> bool {
    !path.is_dir() && path.extension().is_some_and(|ext| ext == CASE_EXTENSION)
}

#[cfg(test)]
mod tests;
