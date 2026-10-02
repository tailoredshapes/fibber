//! A run: generate, check and classify many programs on several threads,
//! then minimise one exemplar of each distinct failure.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use crate::ast::Program;
use crate::coverage::Coverage;
use crate::gen::{generate, generate_pipeline};
use crate::model::{expected, expected_traced, ModelError, Trace};
use crate::print;
use crate::run::{classify, run_with_limit_in, Class, Verdict};
use crate::shrink::shrink;
use fibref::roots::Roots;

/// Which programs a run generates.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GenKind {
    /// The general generator: every construct of the language.
    #[default]
    Programs,
    /// Library pipelines (stdlib §8.1 item 3).
    Pipelines,
}

impl GenKind {
    /// The program for `seed` at `size`.
    pub fn generate(self, seed: u64, size: u32) -> Program {
        match self {
            GenKind::Programs => generate(seed, size),
            GenKind::Pipelines => generate_pipeline(seed, size),
        }
    }
}

/// What a run does.
#[derive(Clone, Debug)]
pub struct Config {
    /// The first seed; program `i` uses `seed + i`.
    pub seed: u64,
    /// How many programs.
    pub count: usize,
    /// A fixed size, or `None` to cycle through 1..=6.
    pub size: Option<u32>,
    /// Worker threads.
    pub jobs: usize,
    /// The time limit per program.
    pub timeout: Duration,
    /// Whether to minimise findings.
    pub shrink: bool,
    /// The most interpreter runs one minimisation may use.
    pub shrink_budget: usize,
    /// How many programs to keep (and minimise) per distinct failure.
    pub exemplars: usize,
    /// Which generator.
    pub kind: GenKind,
    /// Where library modules are looked for before the built-in copy.
    pub roots: Roots,
}

/// One checked program.
#[derive(Clone, Debug)]
pub struct Sample {
    /// Its seed.
    pub seed: u64,
    /// Its size.
    pub size: u32,
    /// The program.
    pub program: Program,
    /// How it fared.
    pub verdict: Verdict,
    /// The model's result.
    pub expected: Result<i64, ModelError>,
    /// What the model's run did (for the coverage table).
    pub trace: Trace,
}

/// A run's results.
#[derive(Debug, Default)]
pub struct Summary {
    /// Programs per class.
    pub tallies: BTreeMap<Class, usize>,
    /// Per (class, key): how many, and the first programs that did it
    /// (lowest seeds first).
    pub groups: BTreeMap<(Class, String), (usize, Vec<Sample>)>,
    /// Constructs exercised.
    pub coverage: Coverage,
    /// Wall time of the checking phase.
    pub elapsed: Duration,
}

impl Summary {
    fn add(&mut self, s: Sample, keep: usize) {
        *self.tallies.entry(s.verdict.class).or_default() += 1;
        self.coverage.add(&s.program);
        self.coverage.add_trace(&s.trace);
        if s.verdict.class == Class::Ok {
            return;
        }
        let key = (s.verdict.class, s.verdict.key.clone());
        let e = self.groups.entry(key).or_insert((0, Vec::new()));
        e.0 += 1;
        if e.1.len() < keep {
            e.1.push(s);
        }
    }

    fn merge(&mut self, other: Summary, keep: usize) {
        for (c, n) in other.tallies {
            *self.tallies.entry(c).or_default() += n;
        }
        self.coverage.merge(&other.coverage);
        for (k, (n, samples)) in other.groups {
            let e = self.groups.entry(k).or_insert((0, Vec::new()));
            e.0 += n;
            e.1.extend(samples);
            e.1.sort_by_key(|s| s.seed);
            e.1.truncate(keep);
        }
    }
}

/// Checks one program: its model result and how the interpreter fared.
pub fn check(p: &Program, timeout: Duration) -> (Verdict, Result<i64, ModelError>) {
    check_in(p, timeout, &Roots::default())
}

/// [`check`] with library modules looked for under `roots` first.
pub fn check_in(
    p: &Program,
    timeout: Duration,
    roots: &Roots,
) -> (Verdict, Result<i64, ModelError>) {
    let (v, e, _) = check_traced(p, timeout, roots);
    (v, e)
}

/// [`check`], and the model's trace.
fn check_traced(
    p: &Program,
    timeout: Duration,
    roots: &Roots,
) -> (Verdict, Result<i64, ModelError>, Trace) {
    let (exp, trace) = expected_traced(p);
    let obs = run_with_limit_in(&print::program(p), timeout, roots);
    (classify(&obs, &exp), exp, trace)
}

/// The size of program `i`.
fn size_of(cfg: &Config, i: usize) -> u32 {
    cfg.size.unwrap_or(1 + (i % 6) as u32)
}

/// Generates and checks `cfg.count` programs on `cfg.jobs` threads.
pub fn run_batch(cfg: &Config) -> Summary {
    let start = Instant::now();
    let next = AtomicUsize::new(0);
    let mut total = Summary::default();
    std::thread::scope(|scope| {
        let workers: Vec<_> = (0..cfg.jobs.max(1))
            .map(|_| {
                scope.spawn(|| {
                    let mut part = Summary::default();
                    loop {
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        if i >= cfg.count {
                            return part;
                        }
                        let (seed, size) = (cfg.seed + i as u64, size_of(cfg, i));
                        let program = cfg.kind.generate(seed, size);
                        let (verdict, expected, trace) =
                            check_traced(&program, cfg.timeout, &cfg.roots);
                        part.add(
                            Sample {
                                seed,
                                size,
                                program,
                                verdict,
                                expected,
                                trace,
                            },
                            cfg.exemplars.max(1),
                        );
                    }
                })
            })
            .collect();
        for w in workers {
            match w.join() {
                Ok(part) => total.merge(part, cfg.exemplars.max(1)),
                Err(_) => {
                    eprintln!("fibgen: a worker thread panicked; its programs are not counted")
                }
            }
        }
    });
    total.elapsed = start.elapsed();
    total
}

/// Minimises `s`, keeping its class and key and a result the model can
/// compute; returns the smaller program and its verdict.
pub fn minimise(s: &Sample, cfg: &Config) -> (Program, Verdict, i64) {
    let want = (s.verdict.class, s.verdict.key.clone());
    let mut still_fails = |p: &Program| {
        if expected(p).is_err() {
            return false;
        }
        let (v, _) = check_in(p, cfg.timeout, &cfg.roots);
        (v.class, v.key) == want
    };
    let small = if cfg.shrink {
        shrink(&s.program, cfg.shrink_budget, &mut still_fails)
    } else {
        s.program.clone()
    };
    let (v, e) = check_in(&small, cfg.timeout, &cfg.roots);
    let fallback = s.expected.clone().unwrap_or_default();
    (small, v, e.unwrap_or(fallback))
}
