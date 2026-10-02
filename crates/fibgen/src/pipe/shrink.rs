//! What the minimiser may change in a pipeline besides its expressions
//! (`crate::shrink` replaces those): a stage dropped, one terminal of
//! two dropped, the shape made the plainest.

use super::{Chain, Operand, Pipe, Shape, Stage};

/// The pipelines one step smaller than `p`, smallest changes first.
pub fn variants(p: &Pipe) -> Vec<Pipe> {
    let mut out = Vec::new();
    if p.twice() {
        for t in &p.terms {
            out.push(Pipe {
                terms: vec![t.clone()],
                ..p.clone()
            });
        }
    }
    for chain in chain_variants(&p.chain) {
        out.push(Pipe { chain, ..p.clone() });
    }
    if p.shape != Shape::Nested {
        out.push(Pipe {
            shape: Shape::Nested,
            ..p.clone()
        });
    }
    out
}

/// `c` with one stage dropped, or with one stage of an operand chain
/// dropped, or an operand chain replaced by the vector it starts from.
fn chain_variants(c: &Chain) -> Vec<Chain> {
    let mut out = Vec::new();
    for i in 0..c.stages.len() {
        let mut stages = c.stages.clone();
        stages.remove(i);
        out.push(Chain {
            src: c.src.clone(),
            stages,
        });
    }
    for (i, s) in c.stages.iter().enumerate() {
        let Stage::Concat { other, first } = s else {
            continue;
        };
        let (Operand::Lazy(inner) | Operand::Eager(inner)) = other else {
            continue;
        };
        let wrap = |inner: Chain| match other {
            Operand::Lazy(_) => Operand::Lazy(Box::new(inner)),
            _ => Operand::Eager(Box::new(inner)),
        };
        for v in chain_variants(inner) {
            let mut stages = c.stages.clone();
            stages[i] = Stage::Concat {
                other: wrap(v),
                first: *first,
            };
            out.push(Chain {
                src: c.src.clone(),
                stages,
            });
        }
    }
    out
}
