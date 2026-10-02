//! The text of each shape, with the forms the expander reads.

use crate::ast::{Expr, Kind};
use crate::pipe::fixtures::*;
use crate::pipe::{Chain, Operand, Pipe, Shape, Src, Stage, Term};
use crate::print::{expr_flat, program as source};
use crate::ty::Ty;

fn text(p: Pipe) -> String {
    expr_flat(&Expr::new(Ty::Int, Kind::Pipe(Box::new(p))))
}

fn stages() -> Vec<Stage> {
    vec![
        Stage::Map(lam(op("+", x(), n(1)))),
        Stage::Filter(plam(cmp(">", x(), n(2)))),
        Stage::Take(n(3)),
    ]
}

const MAP: &str = "(map (fn (x: i64) (+ x 1))";
const FILTER: &str = "(filter (fn (x: i64) (> x 2))";

#[test]
fn threaded_nested_and_bound_say_the_same_chain() {
    let term = || Term::Count;
    let thread = text(pipe(&[1, 2], stages(), term(), Shape::Thread));
    assert_eq!(
        thread,
        "(->> [1 2] (map (fn (x: i64) (+ x 1))) (filter (fn (x: i64) (> x 2))) (take 3) (count))"
    );
    let nested = text(pipe(&[1, 2], stages(), term(), Shape::Nested));
    assert_eq!(nested, format!("(count (take 3 {FILTER} {MAP} [1 2]))))"));
    let bound = text(pipe(&[1, 2], stages(), term(), Shape::Bound));
    let want =
        format!("(let ((sq1 {MAP} [1 2])) (sq2 {FILTER} sq1)) (sq3 (take 3 sq2))) (count sq3))");
    assert_eq!(bound, want);
    let split = text(pipe(&[1, 2], stages(), term(), Shape::Split(1)));
    assert_eq!(
        split,
        format!("(let ((sq1 {MAP} [1 2]))) (count (take 3 {FILTER} sq1))))")
    );
}

#[test]
fn a_chain_with_no_stages_prints_its_source() {
    assert_eq!(
        text(pipe(&[4], vec![], Term::Count, Shape::Nested)),
        "(count [4])"
    );
    assert_eq!(
        text(pipe(&[4], vec![], Term::Count, Shape::Thread)),
        "(->> [4] (count))"
    );
    let range = Pipe {
        chain: Chain {
            src: Src::Range(n(5)),
            stages: vec![],
        },
        terms: vec![Term::Count],
        shape: Shape::Nested,
    };
    assert_eq!(text(range), "(count (range 5))");
}

#[test]
fn every_terminal_has_its_fold() {
    let t = |term| text(pipe(&[1], vec![], term, Shape::Nested));
    assert_eq!(t(Term::Vec), "(digest (vec [1]))");
    assert_eq!(t(Term::Sort), "(digest (vec (sort [1])))");
    assert_eq!(
        t(Term::SortBy(lam(x()))),
        "(digest (vec (sort-by (fn (x: i64) x) [1])))"
    );
    assert_eq!(
        t(Term::First),
        "(match (first [1]) (nil 1000001) ((some v) v))"
    );
    assert_eq!(t(Term::Last), "(unwrap-or (last [1]) 1000001)");
    assert_eq!(t(Term::Empty), "(if (empty? [1]) 1 0)");
    assert_eq!(t(Term::Sum), "(sum [1])");
    assert_eq!(
        t(Term::Every(plam(cmp("<", x(), n(2))))),
        "(if (every? (fn (x: i64) (< x 2)) [1]) 1 0)"
    );
    assert_eq!(
        t(Term::FindFirst(plam(cmp("<", x(), n(2))))),
        "(match (find-first (fn (x: i64) (< x 2)) [1]) (nil 1000001) ((some v) (+ v 1)))"
    );
    assert_eq!(t(Term::Nth(n(2))), "(nth [1] 2)");
    assert_eq!(
        t(Term::Reduce {
            f: step(op("+", a(), x())),
            init: n(0),
            stop: None
        }),
        "(reduce (fn (a: i64 x: i64) (+ a x)) 0 [1])"
    );
    assert_eq!(
        t(Term::Reduce {
            f: step(op("+", a(), x())),
            init: n(0),
            stop: Some(5)
        }),
        "(reduce (fn (a x) (if (> a 5) (reduced a) (+ a x))) 0 [1])"
    );
    assert_eq!(
        t(Term::Reduce1(step(op("+", a(), x())))),
        "(reduce (fn (a: i64 x: i64) (+ a x)) [1])"
    );
}

#[test]
fn a_threaded_nth_and_a_seq_first_concat_are_nested_instead() {
    let thread = |stages, term| text(pipe(&[1], stages, term, Shape::Thread));
    assert_eq!(
        thread(vec![Stage::Take(n(2))], Term::Nth(n(0))),
        "(nth (take 2 [1]) 0)"
    );
    let last = Stage::Concat {
        other: Operand::Vec(vecs(&[9])),
        first: false,
    };
    assert_eq!(thread(vec![last], Term::Count), "(count (concat [1] [9]))");
    let first = Stage::Concat {
        other: Operand::Vec(vecs(&[9])),
        first: true,
    };
    assert_eq!(
        thread(vec![first], Term::Count),
        "(->> [1] (concat [9]) (count))"
    );
}

#[test]
fn operands_are_vectors_lazy_chains_or_realised_chains() {
    let inner = || {
        Box::new(Chain {
            src: src(&[7]),
            stages: vec![Stage::Take(n(1))],
        })
    };
    let cat = |other| vec![Stage::Concat { other, first: true }];
    let t = |other| text(pipe(&[1], cat(other), Term::Count, Shape::Nested));
    assert_eq!(
        t(Operand::Lazy(inner())),
        "(count (concat (take 1 [7]) [1]))"
    );
    assert_eq!(
        t(Operand::Eager(inner())),
        "(count (concat (vec (take 1 [7])) [1]))"
    );
}

#[test]
fn two_terminals_read_one_bound_seq() {
    let p = Pipe {
        chain: Chain {
            src: src(&[1, 2]),
            stages: vec![Stage::Take(n(1))],
        },
        terms: vec![Term::First, Term::Count],
        shape: Shape::Thread,
    };
    assert_eq!(
        text(p),
        "(let ((sq0 (take 1 [1 2]))) (+ (* (match (first sq0) (nil 1000001) ((some v) v)) 7) (count sq0)))"
    );
}

#[test]
fn a_program_with_a_pipeline_names_the_facades_and_the_digest() {
    let src = source(&program(pipe(&[1], vec![], Term::Vec, Shape::Nested)));
    assert!(
        src.starts_with("(ns main (:use fib.core fib.seq fib.coll fib.print))\n\n(defun digest")
    );
    // the digest does not use the library the pipeline tests
    assert!(src.contains("vec-count") && src.contains("vec-nth"));
    let none = source(&program(pipe(&[1], vec![], Term::Count, Shape::Nested)));
    assert!(!none.contains("digest"));
}
