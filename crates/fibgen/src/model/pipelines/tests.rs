//! The model of a pipeline against numbers worked out by hand: what each
//! adaptor and terminal gives, and how often each function runs (the
//! `counting` programs return the answer times 1000 plus the weighted
//! call count).

use crate::ast::{Expr, Kind, Program};
use crate::model::{expected, ModelError};
use crate::pipe::fixtures::*;
use crate::pipe::{Chain, Operand, Pipe, Shape, Src, Stage, Term, NONE};
use crate::ty::Ty;

const SIX: [i64; 6] = [1, 2, 3, 4, 5, 6];

fn answer(stages: Vec<Stage>, term: Term) -> Result<i64, ModelError> {
    expected(&program(pipe(&SIX, stages, term, Shape::Nested)))
}

fn calls(stages: Vec<Stage>, term: Term) -> Result<i64, ModelError> {
    expected(&counting(pipe(&SIX, stages, term, Shape::Nested)))
}

fn even() -> crate::ast::Expr {
    cmp("=", op("rem", x(), n(2)), n(0))
}

#[test]
fn each_adaptor_gives_the_elements_clojure_gives() {
    let double = Stage::Map(lam(op("*", x(), n(2))));
    assert_eq!(answer(vec![double], Term::Sum), Ok(42));
    assert_eq!(answer(vec![Stage::Filter(plam(even()))], Term::Sum), Ok(12));
    assert_eq!(answer(vec![Stage::Remove(plam(even()))], Term::Sum), Ok(9));
    assert_eq!(answer(vec![Stage::Take(n(2))], Term::Sum), Ok(3));
    assert_eq!(answer(vec![Stage::Take(n(-1))], Term::Sum), Ok(0));
    assert_eq!(answer(vec![Stage::Take(n(99))], Term::Sum), Ok(21));
    assert_eq!(answer(vec![Stage::Drop(n(2))], Term::Sum), Ok(18));
    assert_eq!(answer(vec![Stage::Drop(n(-3))], Term::Sum), Ok(21));
    assert_eq!(answer(vec![Stage::Drop(n(99))], Term::Sum), Ok(0));
    let below4 = Stage::TakeWhile(plam(cmp("<", x(), n(4))));
    assert_eq!(answer(vec![below4], Term::Sum), Ok(6));
    let twice = Stage::Mapcat(vlam(vec![x(), x()]));
    assert_eq!(answer(vec![twice], Term::Sum), Ok(42));
}

#[test]
fn concat_puts_its_operand_where_the_flag_says() {
    let cat = |first| Stage::Concat {
        other: Operand::Vec(vecs(&[10, 20])),
        first,
    };
    assert_eq!(answer(vec![cat(true)], Term::First), Ok(10));
    assert_eq!(answer(vec![cat(true)], Term::Last), Ok(6));
    assert_eq!(answer(vec![cat(false)], Term::First), Ok(1));
    assert_eq!(answer(vec![cat(false)], Term::Last), Ok(20));
    assert_eq!(answer(vec![cat(true)], Term::Vec), Ok(257013));
    assert_eq!(answer(vec![cat(false)], Term::Vec), Ok(621360));
}

#[test]
fn each_terminal_folds_its_answer_as_the_program_does() {
    assert_eq!(answer(vec![], Term::Count), Ok(6));
    assert_eq!(answer(vec![], Term::Vec), Ok(76609));
    assert_eq!(answer(vec![], Term::First), Ok(1));
    assert_eq!(answer(vec![], Term::Last), Ok(6));
    assert_eq!(answer(vec![], Term::Empty), Ok(0));
    assert_eq!(answer(vec![Stage::Take(n(0))], Term::Empty), Ok(1));
    assert_eq!(answer(vec![Stage::Take(n(0))], Term::First), Ok(NONE));
    assert_eq!(answer(vec![Stage::Take(n(0))], Term::Last), Ok(NONE));
    let square_sum = Term::Reduce {
        f: step(op("+", a(), op("*", x(), x()))),
        init: n(0),
        stop: None,
    };
    assert_eq!(answer(vec![], square_sum), Ok(91));
    assert_eq!(
        answer(vec![], Term::Every(plam(cmp("<", x(), n(7))))),
        Ok(1)
    );
    assert_eq!(
        answer(vec![], Term::Every(plam(cmp("<", x(), n(3))))),
        Ok(0)
    );
    assert_eq!(
        answer(vec![], Term::FindFirst(plam(cmp(">", x(), n(3))))),
        Ok(5)
    );
    assert_eq!(
        answer(vec![], Term::FindFirst(plam(cmp(">", x(), n(9))))),
        Ok(NONE)
    );
    assert_eq!(answer(vec![], Term::Nth(n(2))), Ok(3));
}

#[test]
fn a_sort_is_stable_and_nth_past_the_end_traps() {
    // keys 1 0 1 0 1 0: the evens first, each group in input order
    let by_parity = Term::SortBy(lam(op("rem", x(), n(2))));
    assert_eq!(answer(vec![], by_parity), Ok(639139));
    let backwards = Stage::Map(lam(op("-", n(7), x())));
    assert_eq!(answer(vec![backwards], Term::Sort), Ok(76609));
    let past = answer(vec![], Term::Nth(n(9)));
    assert_eq!(
        past,
        Err(ModelError::Trap("nth: index out of range".into()))
    );
}

#[test]
fn a_function_runs_once_per_element_the_walk_demands() {
    let f = |w| Stage::Map(lam(counted(w, x())));
    let p = |w| Stage::Filter(plam(counted(w, even())));
    // take 2 of a map: two calls, and the source is not asked for a third
    assert_eq!(
        calls(vec![f(2), Stage::Take(n(2))], Term::Count),
        Ok(2 * 1000 + 4)
    );
    // first of a filter over a map: one element through each, then stop
    let odd_first = vec![f(1), Stage::Remove(plam(counted(10, even())))];
    assert_eq!(calls(odd_first, Term::First), Ok(1000 + 11));
    // the even filter sees 1 (miss) and 2 (hit): two calls of each
    assert_eq!(calls(vec![f(1), p(10)], Term::First), Ok(2 * 1000 + 22));
    // drop skips on the first demand and runs the map on what it skips
    assert_eq!(
        calls(vec![f(1), Stage::Drop(n(2))], Term::First),
        Ok(3 * 1000 + 3)
    );
    // take-while asks the element that stops it, and no other
    let below3 = Stage::TakeWhile(plam(counted(1, cmp("<", x(), n(3)))));
    assert_eq!(calls(vec![below3], Term::Count), Ok(2 * 1000 + 3));
    // every? and find-first stop at the element that decides
    let big = Term::FindFirst(plam(counted(1, cmp(">", x(), n(3)))));
    assert_eq!(calls(vec![], big), Ok(5 * 1000 + 4));
    let small = Term::Every(plam(counted(1, cmp("<", x(), n(3)))));
    assert_eq!(calls(vec![], small), Ok(3));
}

#[test]
fn take_of_nothing_never_touches_its_source_and_mapcat_skips_empty_results() {
    let f = Stage::Map(lam(counted(1, x())));
    assert_eq!(calls(vec![f, Stage::Take(n(0))], Term::Count), Ok(0));
    // (fn (x) (if (even? x) [x] [])): the odd elements give nothing and the walk goes on
    let keep_even = Expr::new(
        Ty::vec(Ty::Int),
        Kind::If(
            Box::new(even()),
            Box::new(vec_of(vec![x()])),
            Box::new(vec_of(vec![])),
        ),
    );
    let f = Expr::new(
        Ty::Func(vec![Ty::Int], Box::new(Ty::vec(Ty::Int))),
        Kind::Fn(vec![("x".into(), Ty::Int)], Box::new(keep_even)),
    );
    assert_eq!(answer(vec![Stage::Mapcat(f)], Term::Sum), Ok(12));
}

#[test]
fn reduce_stops_at_reduced_and_the_two_argument_form_starts_from_the_first() {
    let sum = |w| step(counted(w, op("+", a(), x())));
    // the accumulator is 1, 3, 6 after three elements; the fourth is pulled (ten calls
    // of the map) and the test sees 6 > 5 before the step runs again
    let upstream = vec![Stage::Map(lam(counted(10, x())))];
    let r = Term::Reduce {
        f: sum(1),
        init: n(0),
        stop: Some(5),
    };
    assert_eq!(calls(upstream.clone(), r), Ok(6 * 1000 + 40 + 3));
    let r = Term::Reduce {
        f: sum(1),
        init: n(0),
        stop: None,
    };
    assert_eq!(calls(upstream, r), Ok(21 * 1000 + 60 + 6));
    // (reduce f s): five steps, the first element is the start
    assert_eq!(calls(vec![], Term::Reduce1(sum(1))), Ok(21 * 1000 + 5));
    let none = answer(vec![Stage::Take(n(0))], Term::Reduce1(sum(1)));
    assert_eq!(
        none,
        Err(ModelError::Trap("reduce: empty collection".into()))
    );
}

#[test]
fn a_bound_seq_is_realised_once_whoever_reads_it() {
    let map = Stage::Map(lam(counted(1, x())));
    let chain = Chain {
        src: src(&[1, 2, 3]),
        stages: vec![map],
    };
    let twice = |terms| Pipe {
        chain: chain.clone(),
        terms,
        shape: Shape::Bound,
    };
    // first realises one node, count the other two: three calls, not four
    let p = twice(vec![Term::First, Term::Count]);
    assert_eq!(expected(&counting(p)), Ok((7 + 3) * 1000 + 3));
    // read in the other order, the same three
    let p = twice(vec![Term::Count, Term::First]);
    assert_eq!(expected(&counting(p)), Ok((3 * 7 + 1) * 1000 + 3));
}

#[test]
fn an_operand_chain_is_lazy_or_realised_where_it_is_written() {
    let other = |eager: bool| {
        let c = Box::new(Chain {
            src: src(&[7, 8]),
            stages: vec![Stage::Map(lam(counted(1, x())))],
        });
        let other = if eager {
            Operand::Eager(c)
        } else {
            Operand::Lazy(c)
        };
        vec![Stage::Concat {
            other,
            first: false,
        }]
    };
    // (first (concat s other)) is 1 either way; the realised one ran its map on both
    assert_eq!(calls(other(false), Term::First), Ok(1000));
    assert_eq!(calls(other(true), Term::First), Ok(1000 + 2));
}

#[test]
fn mapcat_asks_for_the_next_source_element_when_the_last_result_is_used() {
    let fan = Stage::Mapcat(vlam(vec![counted(1, x()), x()]));
    let first_two = vec![fan, Stage::Take(n(3))];
    // elements 1 1 2: the function ran for 1 and 2 only
    assert_eq!(calls(first_two, Term::Count), Ok(3 * 1000 + 2));
    let none = Stage::Mapcat(vlam(vec![]));
    assert_eq!(answer(vec![none], Term::Count), Ok(0));
}

#[test]
fn sources_are_vectors_lists_and_ranges() {
    let run = |src: Src| {
        let p = Pipe {
            chain: Chain {
                src,
                stages: vec![],
            },
            terms: vec![Term::Vec],
            shape: Shape::Nested,
        };
        expected(&program(p))
    };
    // ((7 * 31 + 0) * 31 + 1) * 31 + 2
    assert_eq!(run(Src::Range(n(3))), Ok(208570));
    assert_eq!(run(Src::Range(n(-2))), Ok(7));
    let list = Expr2::list(&[1, 2, 3]);
    assert_eq!(run(Src::List(list)), Ok(209563));
}

/// `(list ..)` for the tests.
struct Expr2;

impl Expr2 {
    fn list(items: &[i64]) -> crate::ast::Expr {
        let args = items.iter().map(|v| n(*v)).collect();
        crate::ast::Expr::call(crate::ty::Ty::List, "list", args)
    }
}

#[test]
fn a_program_without_a_pipeline_is_untouched() {
    let p: Program = program(pipe(&SIX, vec![], Term::Count, Shape::Thread));
    assert!(p.uses_library());
}
