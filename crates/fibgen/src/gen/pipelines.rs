//! Library pipelines (stdlib §8.1 item 3, method rule 5): a small source,
//! stages of the lazy adaptors with pure generated functions, and one or
//! two terminals, written one of several ways. The model
//! ([`crate::model`]) evaluates the pipeline as Clojure's lazy seqs are
//! evaluated and the program embeds nothing but its own result: `main`
//! returns a fold of the terminal's answer, and, when the program counts
//! calls, the weighted number of times its functions ran, so that a stage
//! that runs a function once too often or too early changes the result.
//!
//! The programs `:use` the four facades of the library, which is what a
//! program says before the library is implicit and after, and take the
//! lambdas as `fn` literals written where the stage is, which is what the
//! fusion rewrite (R9) reads.

use crate::ast::{Def, Expr, Kind, Program};
use crate::pipe::{Chain, Operand, Pipe, Shape, Src, Stage, Term, COUNT_SCALE};
use crate::rng::Rng;
use crate::ty::Ty;

use super::lambdas::CALLS;

/// Generator state for one pipeline program.
pub struct PGen {
    pub(super) rng: Rng,
    /// Whether the functions add to the call counter.
    pub(super) counted: bool,
    /// The captured `i64` names `main` binds.
    pub(super) ints: Vec<String>,
    /// The captured vector `main` binds, if any.
    pub(super) cv: Option<String>,
    defs: Vec<Def>,
    size: u32,
}

/// The program for `seed` at `size` (1 = a stage or two; 6 = five or six).
pub fn generate_pipeline(seed: u64, size: u32) -> Program {
    let mut g = PGen {
        rng: Rng::new(seed),
        counted: false,
        ints: Vec::new(),
        cv: None,
        defs: Vec::new(),
        size: size.max(1),
    };
    g.counted = g.rng.chance(60);
    let mut binds = Vec::new();
    if g.counted {
        let cell = Expr::call(Ty::cell(Ty::Int), "cell", vec![Expr::int(0)]);
        binds.push((CALLS.to_string(), cell));
    }
    for i in 0..g.rng.below(3) {
        let name = format!("k{}", i + 1);
        binds.push((name.clone(), Expr::int(g.rng.range(-2, 5))));
        g.ints.push(name);
    }
    if g.rng.chance(30) {
        let items = g.int_lits(1, 4);
        binds.push(("cv".to_string(), vec_lit(items)));
        g.cv = Some("cv".to_string());
    }
    let pipe = g.pipe();
    let body = g.with_calls(Expr::new(Ty::Int, Kind::Pipe(Box::new(pipe))));
    let main = if binds.is_empty() {
        body
    } else {
        PGen::lets(binds, body)
    };
    Program {
        defs: g.defs,
        impls: Vec::new(),
        funs: Vec::new(),
        main,
    }
}

fn vec_lit(items: Vec<Expr>) -> Expr {
    Expr::new(Ty::vec(Ty::Int), Kind::VecLit(items))
}

impl PGen {
    /// Between `lo` and `hi` literals.
    pub(super) fn int_lits(&mut self, lo: usize, hi: usize) -> Vec<Expr> {
        let n = lo + self.rng.below(hi - lo + 1);
        (0..n).map(|_| Expr::int(self.rng.range(-3, 12))).collect()
    }

    /// `r` of the pipeline, then the call counter beside it.
    fn with_calls(&mut self, pipe: Expr) -> Expr {
        if !self.counted {
            return pipe;
        }
        let r = Expr::var("r", Ty::Int);
        let calls = Expr::new(
            Ty::Int,
            Kind::Deref(Box::new(Expr::var(CALLS, Ty::cell(Ty::Int)))),
        );
        let scaled = Expr::call(Ty::Int, "*", vec![r, Expr::int(COUNT_SCALE)]);
        let sum = Expr::call(Ty::Int, "+", vec![scaled, calls]);
        PGen::lets(vec![("r".to_string(), pipe)], sum)
    }

    fn pipe(&mut self) -> Pipe {
        let chain = self.chain((1 + self.size as usize).min(6), true, true);
        let twice = self.rng.chance(25);
        let terms = if twice {
            vec![self.term_of(), self.term_of()]
        } else {
            vec![self.term_of()]
        };
        let n = chain.stages.len();
        let seq_last = |s: &Stage| matches!(s, Stage::Concat { first: false, .. });
        let threadable =
            !chain.stages.iter().any(seq_last) && !terms.iter().any(|t| matches!(t, Term::Nth(_)));
        let shape = match self.rng.below(100) {
            0..=24 if threadable => Shape::Thread,
            25..=44 => Shape::Nested,
            45..=69 => Shape::Bound,
            _ if n > 1 => Shape::Split(1 + self.rng.below(n - 1)),
            _ => Shape::Nested,
        };
        Pipe {
            chain,
            terms,
            shape,
        }
    }

    /// A source and up to `max` stages. `top` chains may take a source
    /// from a `def`; `concat` says whether a stage may be a `concat`.
    fn chain(&mut self, max: usize, top: bool, concat: bool) -> Chain {
        let src = self.source(top);
        let n = if max == 0 { 0 } else { 1 + self.rng.below(max) };
        let (mut stages, mut mapcats) = (Vec::new(), 0);
        for _ in 0..n {
            let s = self.stage(concat, mapcats < 2);
            mapcats += usize::from(matches!(s, Stage::Mapcat(_)));
            stages.push(s);
        }
        Chain { src, stages }
    }

    fn source(&mut self, top: bool) -> Src {
        let hi = 3 + 2 * self.size as usize;
        match self.rng.below(100) {
            0..=59 => {
                let items = self.int_lits(1, hi);
                let v = vec_lit(items);
                if top && self.rng.chance(25) {
                    return Src::Vec(self.hoist(v));
                }
                Src::Vec(v)
            }
            60..=79 => {
                let items = self.int_lits(1, hi);
                Src::List(Expr::call(Ty::List, "list", items))
            }
            _ => Src::Range(Expr::int(self.rng.range(0, 8))),
        }
    }

    /// A `def` of `v`, and the variable that names it.
    fn hoist(&mut self, v: Expr) -> Expr {
        let name = format!("src{}", self.defs.len() + 1);
        let ty = v.ty.clone();
        self.defs.push(Def {
            name: name.clone(),
            ty: ty.clone(),
            init: v,
        });
        Expr::var(&name, ty)
    }

    /// A size for `take` and `drop`: a literal, or a captured number.
    fn count_arg(&mut self) -> Expr {
        if !self.ints.is_empty() && self.rng.chance(30) {
            let i = self.rng.below(self.ints.len());
            return Expr::var(&self.ints[i], Ty::Int);
        }
        Expr::int(self.rng.range(-1, 7))
    }

    fn stage(&mut self, concat: bool, mapcat: bool) -> Stage {
        let weights = [
            3,
            3,
            2,
            2,
            2,
            2,
            usize::from(mapcat) * 2,
            usize::from(concat) * 3,
        ];
        match self.rng.weighted(&weights) {
            Some(0) => Stage::Map(self.int_fn(true)),
            Some(1) => Stage::Filter(self.bool_fn()),
            Some(2) => Stage::Remove(self.bool_fn()),
            Some(3) => Stage::Take(self.count_arg()),
            Some(4) => Stage::Drop(self.count_arg()),
            Some(5) => Stage::TakeWhile(self.bool_fn()),
            Some(6) => Stage::Mapcat(self.vec_fn()),
            _ => self.concat_stage(),
        }
    }

    fn concat_stage(&mut self) -> Stage {
        let other = match self.rng.below(100) {
            0..=39 => match self.cv.clone() {
                Some(cv) if self.rng.chance(40) => Operand::Vec(Expr::var(&cv, Ty::vec(Ty::Int))),
                _ => Operand::Vec(vec_lit(self.int_lits(0, 4))),
            },
            40..=69 => Operand::Lazy(Box::new(self.chain(2, false, false))),
            _ => Operand::Eager(Box::new(self.chain(2, false, false))),
        };
        Stage::Concat {
            other,
            first: self.rng.chance(55),
        }
    }

    fn term_of(&mut self) -> Term {
        let weights = [3, 2, 3, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2];
        match self.rng.weighted(&weights) {
            Some(0) => Term::Reduce {
                f: self.step_fn(),
                init: Expr::int(self.rng.range(-5, 5)),
                stop: None,
            },
            Some(12) => Term::Reduce {
                f: self.step_fn(),
                init: Expr::int(self.rng.range(-5, 5)),
                stop: Some(self.rng.range(0, 40)),
            },
            Some(13) => Term::Reduce1(self.step_fn()),
            Some(1) => Term::Count,
            Some(2) => Term::Vec,
            Some(3) => Term::First,
            Some(4) => Term::Last,
            Some(5) => Term::Empty,
            Some(6) => Term::Sum,
            Some(7) => Term::Sort,
            Some(8) => Term::Every(self.bool_fn()),
            Some(9) => Term::FindFirst(self.bool_fn()),
            Some(10) => Term::SortBy(self.key_fn()),
            _ => Term::Nth(Expr::int(self.rng.range(0, 5))),
        }
    }
}

#[cfg(test)]
mod tests;
