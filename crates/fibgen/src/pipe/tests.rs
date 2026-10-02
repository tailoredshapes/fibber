use super::fixtures::*;
use super::*;
use crate::ast::Kind;
use crate::ty::Ty;

fn full() -> Pipe {
    let lazy = Chain {
        src: src(&[7]),
        stages: vec![Stage::Map(lam(x()))],
    };
    let stages = vec![
        Stage::Map(lam(x())),
        Stage::Take(n(2)),
        Stage::Concat {
            other: Operand::Lazy(Box::new(lazy)),
            first: true,
        },
        Stage::Filter(plam(cmp("<", x(), n(5)))),
    ];
    pipe(
        &[1, 2, 3],
        stages,
        Term::Every(plam(cmp("<", x(), n(9)))),
        Shape::Thread,
    )
}

#[test]
fn the_expressions_are_given_in_one_order_by_both_accessors() {
    let mut p = full();
    let before: Vec<Expr> = p.exprs().into_iter().cloned().collect();
    // source, map, take, the operand's source and map, filter, every?
    assert_eq!(before.len(), 7);
    for (i, e) in p.exprs_mut().into_iter().enumerate() {
        *e = Expr::int(i as i64);
    }
    let after: Vec<i64> = p
        .exprs()
        .into_iter()
        .map(|e| match e.kind {
            Kind::Int(v) => v,
            _ => -1,
        })
        .collect();
    assert_eq!(after, [0, 1, 2, 3, 4, 5, 6]);
}

#[test]
fn a_pipeline_is_a_node_of_the_tree_and_the_shrinker_sees_its_parts() {
    let e = Expr::new(Ty::Int, Kind::Pipe(Box::new(full())));
    assert_eq!(e.children().len(), 7);
    assert_eq!(
        e.size(),
        1 + e.children().iter().map(|c| c.size()).sum::<usize>()
    );
}

#[test]
fn variants_drop_a_stage_a_terminal_or_the_shape() {
    let p = full();
    let vs = variants(&p);
    // four stages dropped, one stage of the operand's chain dropped, the shape made plain
    assert_eq!(vs.len(), 4 + 1 + 1);
    assert!(vs
        .iter()
        .any(|v| v.chain.stages.len() == 3 && v.shape == Shape::Thread));
    assert!(vs
        .iter()
        .any(|v| v.shape == Shape::Nested && v.chain.stages.len() == 4));
    let inner = vs.iter().find_map(|v| match &v.chain.stages[2] {
        Stage::Concat {
            other: Operand::Lazy(c),
            ..
        } if c.stages.is_empty() => Some(()),
        _ => None,
    });
    assert!(inner.is_some());
    let two = Pipe {
        terms: vec![Term::Count, Term::First],
        ..pipe(&[1], vec![], Term::Count, Shape::Bound)
    };
    let ts: Vec<Vec<Term>> = variants(&two).into_iter().map(|v| v.terms).collect();
    assert!(ts.contains(&vec![Term::Count]) && ts.contains(&vec![Term::First]));
}

#[test]
fn names_are_the_librarys() {
    assert_eq!(Stage::TakeWhile(n(0)).name(), "take-while");
    assert_eq!(Term::Empty.name(), "empty?");
    assert_eq!(Term::FindFirst(n(0)).name(), "find-first");
    assert_eq!(Src::Range(n(1)).name(), "range");
}
