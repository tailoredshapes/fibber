//! A library pipeline as a node of the generated tree (stdlib §8.1 item
//! 3): a source `Vec`, `List` or `Range`, stages of the lazy adaptors
//! (`map filter remove take drop take-while concat mapcat`), and one or
//! two terminals that read the result. Its type is `i64` (a terminal
//! that yields a collection, an `Option` or a `bool` is folded to a
//! number by the form that prints it, [`crate::print`]).
//!
//! The node keeps its parts as [`Expr`]s (the stages' functions are
//! `fn` literals, the sizes and the operands of `concat` ordinary
//! expressions), so that [`Pipe::exprs`] hands them to the shrinker and
//! the printer and the model read the same tree. The model
//! ([`crate::model`]) walks it as Clojure's lazy seqs are walked, one
//! demand at a time, and is Rust: no line of the library is used to
//! compute what the library must answer.

use crate::ast::Expr;

#[cfg(test)]
pub(crate) mod fixtures;
mod shrink;

pub use shrink::variants;

/// A pipeline and what reads it.
#[derive(Clone, Debug, PartialEq)]
pub struct Pipe {
    /// The source and the stages.
    pub chain: Chain,
    /// One terminal, or two that read one bound seq in turn
    /// ([`Pipe::twice`]): the second finds the first's nodes realised.
    pub terms: Vec<Term>,
    /// How the call is written; ignored when [`Pipe::twice`].
    pub shape: Shape,
}

/// A source and stages: a lazy seq.
#[derive(Clone, Debug, PartialEq)]
pub struct Chain {
    /// Where the elements come from.
    pub src: Src,
    /// The adaptors, first to last.
    pub stages: Vec<Stage>,
}

/// A source of `i64` elements.
#[derive(Clone, Debug, PartialEq)]
pub enum Src {
    /// A `(Vec i64)` expression: a literal, a `def`, a `let` variable.
    Vec(Expr),
    /// A `(List i64)` expression: `(list ..)` or `Empty`.
    List(Expr),
    /// `(range n)`: `0 .. n`, `n` an `i64` expression.
    Range(Expr),
}

/// One adaptor. The `Expr` of a function stage is a `fn` literal; of
/// `take` and `drop` an `i64` expression.
#[derive(Clone, Debug, PartialEq)]
pub enum Stage {
    /// `(map f s)`, `f: (fn (i64) i64)`.
    Map(Expr),
    /// `(filter p s)`, `p: (fn (i64) bool)`.
    Filter(Expr),
    /// `(remove p s)`.
    Remove(Expr),
    /// `(take n s)`.
    Take(Expr),
    /// `(drop n s)`.
    Drop(Expr),
    /// `(take-while p s)`.
    TakeWhile(Expr),
    /// `(mapcat f s)`, `f: (fn (i64) (Vec i64))`.
    Mapcat(Expr),
    /// `(concat other s)` when `first`, else `(concat s other)`: the
    /// threaded form `->>` can only write the first.
    Concat {
        /// The second collection.
        other: Operand,
        /// Whether `other` is the first argument.
        first: bool,
    },
}

/// The other collection of a `concat`.
#[derive(Clone, Debug, PartialEq)]
pub enum Operand {
    /// A `(Vec i64)` expression.
    Vec(Expr),
    /// A chain, left lazy: `(map f w)`.
    Lazy(Box<Chain>),
    /// A chain realised where it is written: `(vec (map f w))`.
    Eager(Box<Chain>),
}

/// What reads a chain.
#[derive(Clone, Debug, PartialEq)]
pub enum Term {
    /// `(reduce f init s)`, `f: (fn (i64 i64) i64)`. With a `stop`, the
    /// literal `f` is written `(fn (a x) (if (> a stop) (reduced a) body))`:
    /// the walk ends at the first element met while the accumulator is
    /// above it.
    Reduce {
        /// The step (its body is the `else` of a `reduced` test).
        f: Expr,
        /// The initial value.
        init: Expr,
        /// The accumulator above which the step answers `(reduced a)`.
        stop: Option<i64>,
    },
    /// `(reduce f s)`: the first element is the start; traps
    /// `reduce: empty collection` on an empty seq.
    Reduce1(Expr),
    /// `(count s)`.
    Count,
    /// `(vec s)`, folded by `digest`.
    Vec,
    /// `(first s)`, `nil` as 1000001.
    First,
    /// `(last s)`, `nil` as 1000001.
    Last,
    /// `(empty? s)` as 1 or 0.
    Empty,
    /// `(sum s)`.
    Sum,
    /// `(vec (sort s))`, folded by `digest`.
    Sort,
    /// `(every? p s)` as 1 or 0.
    Every(Expr),
    /// `(find-first p s)`, `nil` as 1000001, a hit as its value plus one.
    FindFirst(Expr),
    /// `(vec (sort-by key s))`, folded by `digest`; `key: (fn (i64) i64)`.
    SortBy(Expr),
    /// `(nth s i)`: traps `nth: index out of range` past the end.
    Nth(Expr),
}

/// How a pipeline is written (all four mean the same).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    /// `(->> src (map f) (filter p) (reduce g 0))`.
    Thread,
    /// `(reduce g 0 (filter p (map f src)))`.
    Nested,
    /// Every stage bound to a name by a `let`: no fusion, memoised nodes.
    Bound,
    /// The first `k` stages bound, the rest nested over the name.
    Split(usize),
}

/// What `first`, `last`, `find-first` and `nth` give for `nil` (an
/// element of the generated sources is far below it).
pub const NONE: i64 = 1_000_001;

/// The factor that puts the call count beside the result.
pub const COUNT_SCALE: i64 = 100_003;

impl Pipe {
    /// Whether one bound seq is read by two terminals.
    pub fn twice(&self) -> bool {
        self.terms.len() == 2
    }

    /// Every expression the node holds, in the order [`Pipe::exprs_mut`]
    /// gives them.
    pub fn exprs(&self) -> Vec<&Expr> {
        let mut out = self.chain.exprs();
        out.extend(self.terms.iter().flat_map(Term::exprs));
        out
    }

    /// [`Pipe::exprs`], mutably.
    pub fn exprs_mut(&mut self) -> Vec<&mut Expr> {
        let mut out = self.chain.exprs_mut();
        out.extend(self.terms.iter_mut().flat_map(Term::exprs_mut));
        out
    }
}

impl Chain {
    /// The source's expressions, then each stage's.
    pub fn exprs(&self) -> Vec<&Expr> {
        let mut out = self.src.exprs();
        for s in &self.stages {
            out.extend(s.exprs());
        }
        out
    }

    /// [`Chain::exprs`], mutably.
    pub fn exprs_mut(&mut self) -> Vec<&mut Expr> {
        let mut out = self.src.exprs_mut();
        for s in &mut self.stages {
            out.extend(s.exprs_mut());
        }
        out
    }
}

impl Src {
    fn exprs(&self) -> Vec<&Expr> {
        match self {
            Src::Vec(e) | Src::List(e) | Src::Range(e) => vec![e],
        }
    }

    fn exprs_mut(&mut self) -> Vec<&mut Expr> {
        match self {
            Src::Vec(e) | Src::List(e) | Src::Range(e) => vec![e],
        }
    }

    /// The label of the source for the coverage table.
    pub fn name(&self) -> &'static str {
        match self {
            Src::Vec(_) => "vec",
            Src::List(_) => "list",
            Src::Range(_) => "range",
        }
    }
}

impl Stage {
    fn exprs(&self) -> Vec<&Expr> {
        match self {
            Stage::Map(e)
            | Stage::Filter(e)
            | Stage::Remove(e)
            | Stage::Take(e)
            | Stage::Drop(e)
            | Stage::TakeWhile(e)
            | Stage::Mapcat(e) => vec![e],
            Stage::Concat { other, .. } => other.exprs(),
        }
    }

    fn exprs_mut(&mut self) -> Vec<&mut Expr> {
        match self {
            Stage::Map(e)
            | Stage::Filter(e)
            | Stage::Remove(e)
            | Stage::Take(e)
            | Stage::Drop(e)
            | Stage::TakeWhile(e)
            | Stage::Mapcat(e) => vec![e],
            Stage::Concat { other, .. } => other.exprs_mut(),
        }
    }

    /// The adaptor's name, as the library spells it.
    pub fn name(&self) -> &'static str {
        match self {
            Stage::Map(_) => "map",
            Stage::Filter(_) => "filter",
            Stage::Remove(_) => "remove",
            Stage::Take(_) => "take",
            Stage::Drop(_) => "drop",
            Stage::TakeWhile(_) => "take-while",
            Stage::Mapcat(_) => "mapcat",
            Stage::Concat { .. } => "concat",
        }
    }
}

impl Operand {
    fn exprs(&self) -> Vec<&Expr> {
        match self {
            Operand::Vec(e) => vec![e],
            Operand::Lazy(c) | Operand::Eager(c) => c.exprs(),
        }
    }

    fn exprs_mut(&mut self) -> Vec<&mut Expr> {
        match self {
            Operand::Vec(e) => vec![e],
            Operand::Lazy(c) | Operand::Eager(c) => c.exprs_mut(),
        }
    }
}

impl Term {
    fn exprs(&self) -> Vec<&Expr> {
        match self {
            Term::Reduce { f, init, .. } => vec![f, init],
            Term::Reduce1(e)
            | Term::Every(e)
            | Term::FindFirst(e)
            | Term::SortBy(e)
            | Term::Nth(e) => vec![e],
            _ => Vec::new(),
        }
    }

    fn exprs_mut(&mut self) -> Vec<&mut Expr> {
        match self {
            Term::Reduce { f, init, .. } => vec![f, init],
            Term::Reduce1(e)
            | Term::Every(e)
            | Term::FindFirst(e)
            | Term::SortBy(e)
            | Term::Nth(e) => vec![e],
            _ => Vec::new(),
        }
    }

    /// The terminal's name, as the library spells it.
    pub fn name(&self) -> &'static str {
        match self {
            Term::Reduce { stop: None, .. } => "reduce",
            Term::Reduce { stop: Some(_), .. } => "reduce with reduced",
            Term::Reduce1(_) => "reduce of two arguments",
            Term::Count => "count",
            Term::Vec => "vec",
            Term::First => "first",
            Term::Last => "last",
            Term::Empty => "empty?",
            Term::Sum => "sum",
            Term::Sort => "sort",
            Term::Every(_) => "every?",
            Term::FindFirst(_) => "find-first",
            Term::SortBy(_) => "sort-by",
            Term::Nth(_) => "nth",
        }
    }
}

#[cfg(test)]
mod tests;
