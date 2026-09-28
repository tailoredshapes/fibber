//! The case harness (spec/lir.md §13): every case through the JIT and
//! through AOT, each in its own process.

pub(crate) mod exec;
mod header;
mod verdict;

pub use header::{parse as parse_header, signal_name, Expect, Header, Paths, Signal};

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

/// One case's result.
pub struct Report {
    pub path: PathBuf,
    pub failure: Option<String>,
}

/// Every `.lir` file under `dirs`, sorted.
pub fn collect(dirs: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for d in dirs {
        walk(d, &mut out);
    }
    out.sort();
    out
}

fn walk(p: &Path, out: &mut Vec<PathBuf>) {
    if p.is_file() && p.extension().is_some_and(|e| e == "lir") {
        out.push(p.to_path_buf());
        return;
    }
    if let Ok(rd) = std::fs::read_dir(p) {
        for e in rd.flatten() {
            walk(&e.path(), out);
        }
    }
}

/// Run every case with `lair` (this program) as the tool, in parallel.
pub fn run_all(lair: &Path, cases: &[PathBuf], scratch: &Path) -> Vec<Report> {
    let next = AtomicUsize::new(0);
    let results = Mutex::new(Vec::new());
    let workers = std::thread::available_parallelism()
        .map_or(4, |n| n.get())
        .min(8);
    std::thread::scope(|s| {
        for _ in 0..workers {
            s.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::SeqCst);
                let Some(path) = cases.get(i) else { break };
                let failure = verdict::run_case(lair, path, scratch, i).err();
                if let Ok(mut r) = results.lock() {
                    r.push(Report {
                        path: path.clone(),
                        failure,
                    });
                }
            });
        }
    });
    let mut r = results.into_inner().unwrap_or_default();
    r.sort_by(|a, b| a.path.cmp(&b.path));
    r
}
