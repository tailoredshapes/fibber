//! A mutation fuzzer for the checker (spec/lir.md §10, method.md rule
//! 7): every accept case of a suite, deformed at random, must be
//! rejected with a message or run, never crash the checker, the
//! lowering or LLVM. Each mutant runs in a process of its own, through
//! `lair fuzz-one`, which marks on stderr how far it got.

mod deep;
mod grammar;
mod mutate;
mod rng;
mod tree;
mod verdict;

pub use verdict::{classify, Verdict};

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use mutate::Case;
use rng::Rng;

use crate::cases::exec::run_within;
use crate::cases::parse_header;

/// The stage marks `lair fuzz-one` prints on stderr.
pub const CHECKED: &str = "fuzz-one: checked";
pub const COMPILED: &str = "fuzz-one: compiled";

pub struct Config {
    pub seed: u64,
    pub count: usize,
    /// Per mutant, for the checker, the backend and the run together.
    pub timeout: Duration,
    /// Where a finding's mutant is kept.
    pub out_dir: PathBuf,
    /// One line per mutant on standard output.
    pub verbose: bool,
    /// LLVM's optimisation level for the worker (0 to 3).
    pub opt_level: u8,
}

/// One mutant the checker or the backend mishandled.
pub struct Finding {
    pub index: usize,
    pub case: String,
    pub ops: Vec<&'static str>,
    pub what: String,
    /// Where the mutant was written.
    pub path: PathBuf,
}

#[derive(Default)]
pub struct Summary {
    pub mutants: usize,
    pub rejected: usize,
    pub ran: usize,
    pub runtime_crashes: usize,
    pub runtime_timeouts: usize,
    pub findings: Vec<Finding>,
}

/// Read the accept cases under `paths`.
pub fn corpus(paths: &[PathBuf]) -> Result<Vec<Case>, String> {
    let mut out = Vec::new();
    for p in crate::cases::collect(paths) {
        let src = std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?;
        let h = parse_header(&src).map_err(|e| format!("{}: {e}", p.display()))?;
        if !matches!(h.expect, crate::cases::Expect::Accept { .. }) {
            continue;
        }
        let forms = lir::sexp::read(&src).map_err(|e| format!("{}: {e}", p.display()))?;
        out.push(Case::new(p.to_string_lossy().into_owned(), forms));
    }
    if out.is_empty() {
        return Err("no accept cases found".into());
    }
    Ok(out)
}

/// Mutant `i` of the run: fixed by the seed and `i` alone, so a finding
/// is reproducible whatever the scheduling.
fn mutant<'c>(cfg: &Config, corpus: &'c [Case], i: usize) -> (String, &'c Case, Vec<&'static str>) {
    let mut rng = Rng::new(cfg.seed ^ (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));
    let case = &corpus[rng.below(corpus.len())];
    let (forms, ops) = mutate::mutate(&mut rng, corpus, case);
    (tree::print(&forms), case, ops)
}

/// The source of mutant `i`, to reproduce it by hand.
pub fn show(corpus: &[Case], cfg: &Config, i: usize) -> String {
    let (src, case, ops) = mutant(cfg, corpus, i);
    format!(
        ";; mutant {i} of {} by {}\n{src}",
        case.name,
        ops.join(", ")
    )
}

/// Run `cfg.count` mutants of `corpus` through `lair`, in parallel.
pub fn run(lair: &Path, corpus: &[Case], cfg: &Config) -> Result<Summary, String> {
    let scratch = std::env::temp_dir().join(format!("lair-fuzz-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&cfg.out_dir).map_err(|e| e.to_string())?;
    let next = AtomicUsize::new(0);
    let summary = Mutex::new(Summary::default());
    let workers = std::thread::available_parallelism()
        .map_or(4, |n| n.get())
        .min(8);
    std::thread::scope(|s| {
        for _ in 0..workers {
            s.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::SeqCst);
                if i >= cfg.count {
                    break;
                }
                let v = one(lair, corpus, cfg, &scratch, i);
                if let Ok(mut sm) = summary.lock() {
                    sm.record(v);
                }
            });
        }
    });
    let _ = std::fs::remove_dir_all(&scratch);
    let mut sm = summary.into_inner().unwrap_or_default();
    sm.findings.sort_by_key(|f| f.index);
    Ok(sm)
}

/// Mutant `i`: written, run, judged; a finding's file is kept.
fn one(
    lair: &Path,
    corpus: &[Case],
    cfg: &Config,
    scratch: &Path,
    i: usize,
) -> Result<Verdict, Finding> {
    let (src, case, ops) = mutant(cfg, corpus, i);
    let file = scratch.join(format!("m{i}.lir"));
    let finding = |what: String, path: PathBuf| Finding {
        index: i,
        case: case.name.clone(),
        ops: ops.clone(),
        what,
        path,
    };
    if let Err(e) = std::fs::write(&file, &src) {
        return Err(finding(format!("cannot write mutant: {e}"), file));
    }
    let file_s = file.to_string_lossy().into_owned();
    let lair_s = lair.to_string_lossy().into_owned();
    // A shell caps the address space and drops the program's output,
    // which a mutant may produce without end.
    let script = "ulimit -v 4194304; exec \"$0\" fuzz-one -O \"$1\" \"$2\" >/dev/null";
    let opt = cfg.opt_level.to_string();
    let out = run_within(
        Path::new("sh"),
        &["-c", script, &lair_s, &opt, &file_s],
        cfg.timeout,
    );
    let v = classify(&out);
    let _ = std::fs::remove_file(&file);
    if cfg.verbose {
        println!("mutant {i} of {} by {}: {v:?}", case.name, ops.join(", "));
    }
    let Verdict::Finding(what) = v else {
        return Ok(v);
    };
    let path = cfg.out_dir.join(format!("seed{}-m{i}.lir", cfg.seed));
    let header = format!(
        ";; lair fuzz finding: seed {} mutant {i} of {}\n;; mutations: {}\n;; {what}\n",
        cfg.seed,
        case.name,
        ops.join(", ")
    );
    let _ = std::fs::write(&path, header + &src);
    Err(finding(what, path))
}

impl Summary {
    fn record(&mut self, v: Result<Verdict, Finding>) {
        self.mutants += 1;
        match v {
            Ok(Verdict::Rejected) => self.rejected += 1,
            Ok(Verdict::Ran) => self.ran += 1,
            Ok(Verdict::RuntimeCrash) => self.runtime_crashes += 1,
            Ok(Verdict::RuntimeTimeout) => self.runtime_timeouts += 1,
            // `one` turns every Verdict::Finding into an Err.
            Ok(Verdict::Finding(_)) => {}
            Err(f) => self.findings.push(f),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mutants_are_fixed_by_seed_and_index() {
        let corpus = vec![Case::new(
            "t".into(),
            lir::sexp::read(
                "(define (main i32) () (block entry (let ((x (add (i32 1) (i32 2)))) (ret x))))",
            )
            .unwrap(),
        )];
        let cfg = Config {
            seed: 3,
            count: 10,
            timeout: Duration::from_secs(1),
            out_dir: PathBuf::new(),
            verbose: false,
            opt_level: 0,
        };
        let (a, _, ops) = mutant(&cfg, &corpus, 5);
        let (b, _, _) = mutant(&cfg, &corpus, 5);
        assert_eq!(a, b);
        assert!(!ops.is_empty());
        assert_ne!(a, mutant(&cfg, &corpus, 6).0);
    }
}
