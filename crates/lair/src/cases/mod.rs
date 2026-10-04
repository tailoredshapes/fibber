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

/// Whether the header of a case says `;; stage: 2`: only the fibber `lair` has the forms it uses (spec/lir.md §13), so this one leaves it out.
fn is_stage_two(src: &str) -> bool {
    src.lines()
        .take_while(|l| l.starts_with(";;"))
        .filter_map(|l| {
            l.trim_start_matches(';')
                .trim_start()
                .strip_prefix("stage:")
        })
        .any(|v| v.trim() == "2")
}

fn walk(p: &Path, out: &mut Vec<PathBuf>) {
    if p.is_file() && p.extension().is_some_and(|e| e == "lir") {
        if !std::fs::read_to_string(p).is_ok_and(|s| is_stage_two(&s)) {
            out.push(p.to_path_buf());
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stage_two_case_is_left_out() {
        assert!(is_stage_two(";; expect: accept\n;; stage: 2\n(define)"));
        assert!(is_stage_two(";; stage:  2  \n"));
        assert!(!is_stage_two(
            ";; expect: accept\n(define (main i32) () (block entry (ret (i32 0))))"
        ));
        assert!(!is_stage_two(";; stage: 1\n"));
        // only the header counts: a later comment line is not a header line
        assert!(!is_stage_two(";; expect: accept\n(define)\n;; stage: 2\n"));
    }

    #[test]
    fn collect_skips_stage_two_files_and_keeps_the_others() {
        let dir = std::env::temp_dir().join(format!("lair-stage-two-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        std::fs::write(dir.join("old.lir"), ";; expect: accept\n").expect("write");
        std::fs::write(dir.join("new.lir"), ";; expect: accept\n;; stage: 2\n").expect("write");
        let found = collect(std::slice::from_ref(&dir));
        std::fs::remove_dir_all(&dir).expect("cleanup");
        assert_eq!(found.len(), 1);
        assert!(found[0].ends_with("old.lir"));
    }
}
