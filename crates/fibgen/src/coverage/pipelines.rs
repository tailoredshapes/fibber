//! Coverage labels for library pipelines: a count per adaptor, per
//! terminal, per source and per shape (stdlib §8.1 item 3).

use crate::ast::Kind;
use crate::pipe::{Chain, Operand, Pipe, Shape, Stage};

/// The labels of one pipeline node.
pub fn labels(p: &Pipe, out: &mut Vec<String>) {
    chain_labels(&p.chain, out);
    for t in &p.terms {
        out.push(format!("pipeline terminal: {}", t.name()));
    }
    out.push(match (p.twice(), p.shape) {
        (true, _) => "pipeline shape: one seq bound, two terminals".to_string(),
        (false, Shape::Thread) => "pipeline shape: threaded with ->>".to_string(),
        (false, Shape::Nested) => "pipeline shape: nested calls".to_string(),
        (false, Shape::Bound) => "pipeline shape: every stage bound".to_string(),
        (false, Shape::Split(_)) => "pipeline shape: first stages bound".to_string(),
    });
    let counts = p.exprs().into_iter().any(|e| {
        let mut found = false;
        e.walk(&mut |n| found |= matches!(n.kind, Kind::Set(..)));
        found
    });
    if counts {
        out.push("pipeline functions count their calls".into());
    }
}

fn chain_labels(c: &Chain, out: &mut Vec<String>) {
    out.push(format!("pipeline source: {}", c.src.name()));
    for s in &c.stages {
        out.push(format!("pipeline stage: {}", s.name()));
        if let Stage::Concat { other, first } = s {
            out.push(format!(
                "pipeline concat operand: {}",
                match other {
                    Operand::Vec(_) => "vector",
                    Operand::Lazy(_) => "lazy chain",
                    Operand::Eager(_) => "realised chain",
                }
            ));
            out.push(format!(
                "pipeline concat order: {}",
                if *first { "operand first" } else { "seq first" }
            ));
            if let Operand::Lazy(inner) | Operand::Eager(inner) = other {
                chain_labels(inner, out);
            }
        }
    }
}

#[cfg(test)]
mod tests;
