use super::*;
use crate::pipe::fixtures::*;
use crate::pipe::{Operand, Shape, Term};

fn labelled(p: &Pipe) -> Vec<String> {
    let mut out = Vec::new();
    labels(p, &mut out);
    out
}

#[test]
fn a_pipeline_is_counted_by_stage_terminal_source_and_shape() {
    let inner = Chain {
        src: src(&[1]),
        stages: vec![Stage::Take(n(1))],
    };
    let stages = vec![
        Stage::Map(lam(counted(1, x()))),
        Stage::Concat {
            other: Operand::Eager(Box::new(inner)),
            first: false,
        },
    ];
    let p = pipe(&[1], stages, Term::Sum, Shape::Bound);
    let got = labelled(&p);
    for want in [
        "pipeline source: vec",
        "pipeline stage: map",
        "pipeline stage: concat",
        "pipeline stage: take",
        "pipeline terminal: sum",
        "pipeline shape: every stage bound",
        "pipeline concat operand: realised chain",
        "pipeline concat order: seq first",
        "pipeline functions count their calls",
    ] {
        assert!(
            got.iter().any(|l| l == want),
            "{want} is missing from {got:?}"
        );
    }
    // the operand's source is counted too
    assert_eq!(
        got.iter().filter(|l| *l == "pipeline source: vec").count(),
        2
    );
}

#[test]
fn two_terminals_and_the_plain_shapes_are_told_apart() {
    let mut p = pipe(&[1], vec![], Term::Count, Shape::Thread);
    assert!(labelled(&p).contains(&"pipeline shape: threaded with ->>".to_string()));
    assert!(!labelled(&p).iter().any(|l| l.contains("count their calls")));
    // a function that does not count is not a counting one
    let plain = pipe(&[1], vec![Stage::Map(lam(x()))], Term::Count, Shape::Nested);
    assert!(!labelled(&plain)
        .iter()
        .any(|l| l.contains("count their calls")));
    p.terms.push(Term::First);
    assert!(labelled(&p).contains(&"pipeline shape: one seq bound, two terminals".to_string()));
    assert_eq!(
        labelled(&p)
            .iter()
            .filter(|l| l.starts_with("pipeline terminal"))
            .count(),
        2
    );
}

#[test]
fn the_program_level_count_reaches_the_table() {
    let mut c = crate::coverage::Coverage::default();
    c.add(&program(pipe(
        &[1],
        vec![Stage::Drop(n(0))],
        Term::Last,
        Shape::Nested,
    )));
    assert_eq!(c.counts.get("pipeline stage: drop"), Some(&(1, 1)));
    assert_eq!(c.counts.get("pipeline terminal: last"), Some(&(1, 1)));
}
