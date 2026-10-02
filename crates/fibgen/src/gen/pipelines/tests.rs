use super::*;
use crate::coverage::Coverage;
use crate::driver::check_in;
use crate::model::{expected, ModelError};
use crate::pipe::Shape;
use crate::print;
use crate::run::Class;
use fibref::roots::Roots;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// The seeds and sizes the driver gives `fibgen run --seed 1 --count 200`.
fn programs(count: u64) -> impl Iterator<Item = (u64, Program)> {
    (1..=count).map(|seed| (seed, generate_pipeline(seed, 1 + ((seed - 1) % 6) as u32)))
}

fn pipe_of(p: &Program) -> Pipe {
    let mut found = None;
    p.main.walk(&mut |e| {
        if let Kind::Pipe(pipe) = &e.kind {
            found.get_or_insert_with(|| (**pipe).clone());
        }
    });
    found.expect("the program holds a pipeline")
}

#[test]
fn same_seed_same_program_and_the_seeds_differ() {
    assert_eq!(generate_pipeline(42, 4), generate_pipeline(42, 4));
    let texts: std::collections::HashSet<String> =
        programs(40).map(|(_, p)| print::program(&p)).collect();
    assert_eq!(texts.len(), 40);
}

#[test]
fn the_model_evaluates_every_pipeline_it_is_given() {
    let mut traps = 0;
    for (seed, p) in programs(300) {
        match expected(&p) {
            Ok(_) => {}
            Err(ModelError::Trap(m))
                if m == "nth: index out of range" || m == "reduce: empty collection" =>
            {
                traps += 1
            }
            Err(e) => panic!("seed {seed}: {e:?}\n{}", print::program(&p)),
        }
    }
    // `nth` past the end and `reduce` of nothing are the only traps a pipeline makes
    assert!(traps > 0 && traps < 80, "{traps} traps in 300 programs");
}

#[test]
fn main_is_an_i64_and_the_program_names_the_facades() {
    for (_, p) in programs(30) {
        assert_eq!(p.main.ty, Ty::Int);
        assert!(p.uses_library());
        let text = print::program(&p);
        assert!(text.starts_with("(ns main (:use fib.core fib.seq fib.coll fib.print))"));
        assert!(text.contains("(defun main () -> i64"));
    }
}

#[test]
fn two_hundred_seeds_reach_every_adaptor_terminal_source_and_shape_ten_times() {
    let mut c = Coverage::default();
    for (_, p) in programs(200) {
        c.add(&p);
    }
    let need = [
        "stage: map",
        "stage: filter",
        "stage: remove",
        "stage: take",
        "stage: drop",
        "stage: take-while",
        "stage: mapcat",
        "stage: concat",
        "terminal: reduce",
        "terminal: count",
        "terminal: vec",
        "terminal: first",
        "terminal: last",
        "terminal: empty?",
        "terminal: sum",
        "terminal: sort",
        "terminal: every?",
        "terminal: find-first",
        "terminal: sort-by",
        "terminal: nth",
        "terminal: reduce with reduced",
        "terminal: reduce of two arguments",
        "source: vec",
        "source: list",
        "source: range",
        "shape: threaded with ->>",
        "shape: nested calls",
        "shape: every stage bound",
        "shape: first stages bound",
        "shape: one seq bound, two terminals",
        "concat operand: vector",
        "concat operand: lazy chain",
        "concat operand: realised chain",
        "concat order: operand first",
        "concat order: seq first",
        "functions count their calls",
    ];
    for label in need {
        let (progs, _) = c
            .counts
            .get(&format!("pipeline {label}"))
            .copied()
            .unwrap_or((0, 0));
        assert!(
            progs >= 10,
            "{label}: only {progs} of 200 programs\n{}",
            c.render()
        );
    }
}

#[test]
fn a_function_always_uses_its_argument() {
    for (seed, p) in programs(300) {
        let pipe = pipe_of(&p);
        for s in &pipe.chain.stages {
            let (Stage::Map(f) | Stage::Filter(f) | Stage::Remove(f) | Stage::TakeWhile(f)) = s
            else {
                continue;
            };
            let mut uses = false;
            f.walk(&mut |e| uses |= matches!(&e.kind, Kind::Var(v) if v == "x"));
            assert!(uses, "seed {seed}: {}", print::expr_text(f));
        }
    }
}

#[test]
fn a_threaded_pipeline_is_written_threaded() {
    let mut threaded = 0;
    for (seed, p) in programs(300) {
        let pipe = pipe_of(&p);
        let text = print::program(&p);
        let wants = pipe.shape == Shape::Thread && !pipe.twice();
        assert_eq!(text.contains("(->>"), wants, "seed {seed}\n{text}");
        threaded += usize::from(wants);
    }
    assert!(threaded > 30, "{threaded}");
}

#[test]
fn the_interpreter_agrees_with_the_model_on_generated_pipelines() {
    for (seed, p) in programs(30) {
        let (v, _) = check_in(&p, Duration::from_secs(120), &Roots::default());
        assert!(
            matches!(v.class, Class::Ok | Class::ExpectedTrap),
            "seed {seed}: {v:?}\n{}",
            print::program(&p)
        );
    }
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("a scratch directory");
    for entry in std::fs::read_dir(from).expect("the library is readable") {
        let entry = entry.expect("an entry");
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).expect("a copied file");
        }
    }
}

/// A scratch copy of `lib/` with `old` replaced by `new` in `file`.
fn faulty_library(tag: &str, file: &str, old: &str, new: &str) -> PathBuf {
    let lib = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../lib");
    let dir = std::env::temp_dir().join(format!("fibgen-canary-{tag}-{}", std::process::id()));
    copy_dir(&lib, &dir);
    let path = dir.join(file);
    let text = std::fs::read_to_string(&path).expect("the part is readable");
    assert_eq!(
        text.matches(old).count(),
        1,
        "the text to break is in {file} once"
    );
    std::fs::write(path, text.replace(old, new)).expect("the broken part is written");
    dir
}

/// How many of the first `count` pipelines that count calls (when
/// `counting`) or that do not (else) the library under `dir` gets wrong
/// (a finding), and how many it gets right.
fn judge_where(dir: &Path, count: u64, counting: bool) -> (usize, usize) {
    let roots = Roots::new(vec![dir.to_path_buf()]);
    let (mut bad, mut good) = (0, 0);
    for (_, p) in programs(count) {
        if print::program(&p).contains("(cell 0)") != counting {
            continue;
        }
        let (v, _) = check_in(&p, Duration::from_secs(120), &roots);
        match v.class {
            Class::Ok | Class::ExpectedTrap => good += 1,
            _ => bad += 1,
        }
    }
    (bad, good)
}

fn judge(dir: &Path, count: u64) -> (usize, usize) {
    let (b1, g1) = judge_where(dir, count, true);
    let (b2, g2) = judge_where(dir, count, false);
    (b1 + b2, g1 + g2)
}

/// The generator can fail: with `filter` broken in a copy of the
/// library, in the lazy form and in the fused form, it finds the fault;
/// with the library as it is, it finds nothing.
#[test]
fn a_filter_broken_in_a_copy_of_the_library_is_found() {
    let lazy = faulty_library(
        "lazy",
        "fib/seq/adaptors.fib",
        "(if (truthy? (p h)) (LCons h (lfilter p t)) (recur t))",
        "(if (truthy? (p h)) (recur t) (LCons h (lfilter p t)))",
    );
    let fused = faulty_library(
        "fused",
        "fib/seq/recipes.fib",
        "(fn (x) (if (truthy? (p x)) (k x) true))",
        "(fn (x) (if (truthy? (p x)) true (k x)))",
    );
    let sound = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../lib");
    assert_eq!(judge(&sound, 120), (0, 120));
    let (bad_lazy, _) = judge(&lazy, 120);
    let (bad_fused, _) = judge(&fused, 120);
    for dir in [&lazy, &fused] {
        let _ = std::fs::remove_dir_all(dir);
    }
    assert!(
        bad_lazy >= 5,
        "the lazy filter was found by {bad_lazy} of 120"
    );
    assert!(
        bad_fused >= 5,
        "the fused filter was found by {bad_fused} of 120"
    );
}

/// A `take` that realises the source's next node before it is asked for
/// gives the same answers and runs the source's functions once too often:
/// only a program that counts its calls can tell, and every one the
/// library gets wrong is such a program.
#[test]
fn a_take_that_looks_ahead_is_found_by_the_call_count_alone() {
    let ahead = faulty_library(
        "ahead",
        "fib/seq/adaptors.fib",
        "((LCons h t) (LCons h (ltake (- n 1) t))))))))\n\n;; the seq after",
        "((LCons h t) (do (lnode t) (LCons h (ltake (- n 1) t)))))))))\n\n;; the seq after",
    );
    let (bad_counting, _) = judge_where(&ahead, 300, true);
    let (bad_plain, good_plain) = judge_where(&ahead, 300, false);
    let _ = std::fs::remove_dir_all(&ahead);
    assert!(
        bad_counting >= 1,
        "no program that counts calls found it in 300"
    );
    assert_eq!(bad_plain, 0, "a program that does not count calls found it");
    assert!(good_plain > 50, "{good_plain}");
}

/// A `take` that gives one element too many changes the answers.
#[test]
fn a_take_that_gives_one_too_many_is_found() {
    let greedy = faulty_library(
        "greedy",
        "fib/seq/recipes.fib",
        "(if (k x) (< @i n) (do (set! refused true) false))",
        "(if (k x) (<= @i n) (do (set! refused true) false))",
    );
    let (bad, _) = judge(&greedy, 120);
    let _ = std::fs::remove_dir_all(&greedy);
    assert!(bad >= 3, "found by {bad} of 120");
}
