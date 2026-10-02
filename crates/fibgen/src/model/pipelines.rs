//! The model of a library pipeline ([`crate::pipe`]): Clojure's lazy seqs
//! walked one demand at a time, written from what each adaptor means and
//! from nothing in the library.
//!
//! A [`Node`] is a stage with the state it needs; `next` makes it give one
//! element, pulling its source only as far as that takes. So a function
//! runs exactly as often as Clojure's lazy seq would run it: `map` once
//! per element pulled, `filter` until a hit, `take n` never pulls the
//! source again once it has given `n`, `take-while` stops at the first
//! miss and does not look again, `drop n` skips on its first demand,
//! `concat` goes on into its second part in the demand that finds the
//! first one empty, `mapcat` takes the next source element when the
//! last result is used up. A terminal reads the chain through a [`Memo`]:
//! a seq realised once, as a bound seq is, so a second terminal finds the
//! first's nodes and runs a function only for the nodes it adds.
//!
//! The functions of the stages are `fn` values of the model's evaluator;
//! the programs count their calls in a cell, which is how a run's call
//! counts reach its result.

use crate::pipe::{Chain, Operand, Pipe, Src, Stage, Term, NONE};

use super::eval::{trap, unsupported, Env, Machine, Res, Stop};
use super::value::V;

/// A stage and its state.
enum Node {
    Items {
        items: Vec<i64>,
        at: usize,
    },
    Map {
        src: Box<Node>,
        f: V,
    },
    Filter {
        src: Box<Node>,
        p: V,
        keep: bool,
    },
    Take {
        src: Box<Node>,
        left: i64,
    },
    Drop {
        src: Box<Node>,
        n: i64,
        started: bool,
    },
    TakeWhile {
        src: Box<Node>,
        p: V,
    },
    Concat {
        a: Box<Node>,
        b: Box<Node>,
        in_b: bool,
    },
    Mapcat {
        src: Box<Node>,
        f: V,
        inner: Vec<i64>,
        at: usize,
    },
}

type Pulled = Result<Option<i64>, Stop>;

/// The `i64` a function of the generated program gave.
fn int_of(v: V) -> Result<i64, Stop> {
    match v {
        V::Int(n) => Ok(n),
        other => Err(unsupported(format!("a pipeline function gave {other:?}"))),
    }
}

fn bool_of(v: V) -> Result<bool, Stop> {
    match v {
        V::Bool(b) => Ok(b),
        other => Err(unsupported(format!("a pipeline predicate gave {other:?}"))),
    }
}

fn ints_of(v: V) -> Result<Vec<i64>, Stop> {
    match v {
        V::Vector(xs) | V::List(xs) => xs.iter().cloned().map(int_of).collect(),
        other => Err(unsupported(format!("a pipeline source is {other:?}"))),
    }
}

impl Node {
    fn items(items: Vec<i64>) -> Node {
        Node::Items { items, at: 0 }
    }

    /// The next element, or `None` at the end.
    fn next(&mut self, m: &mut Machine) -> Pulled {
        match self {
            Node::Items { items, at } => {
                let x = items.get(*at).copied();
                *at += usize::from(x.is_some());
                Ok(x)
            }
            Node::Map { src, f } => match src.next(m)? {
                Some(x) => Ok(Some(int_of(m.apply(f, vec![V::Int(x)])?)?)),
                None => Ok(None),
            },
            Node::Filter { src, p, keep } => loop {
                let Some(x) = src.next(m)? else {
                    return Ok(None);
                };
                if bool_of(m.apply(p, vec![V::Int(x)])?)? == *keep {
                    return Ok(Some(x));
                }
            },
            Node::Take { src, left } => {
                if *left <= 0 {
                    return Ok(None);
                }
                *left -= 1;
                src.next(m)
            }
            Node::Drop { src, n, started } => skip_then_next(src, *n, started, m),
            Node::TakeWhile { src, p } => match src.next(m)? {
                Some(x) if bool_of(m.apply(p, vec![V::Int(x)])?)? => Ok(Some(x)),
                _ => Ok(None),
            },
            Node::Concat { a, b, in_b } => concat_next(a, b, in_b, m),
            Node::Mapcat { src, f, inner, at } => mapcat_next(src, f, inner, at, m),
        }
    }

    /// Every remaining element.
    fn drain(&mut self, m: &mut Machine) -> Result<Vec<i64>, Stop> {
        let mut out = Vec::new();
        while let Some(x) = self.next(m)? {
            out.push(x);
        }
        Ok(out)
    }
}

/// `drop`: the first demand skips `n` elements, which runs the stages
/// below it for each, and then gives the next.
fn skip_then_next(src: &mut Node, n: i64, started: &mut bool, m: &mut Machine) -> Pulled {
    if !*started {
        *started = true;
        for _ in 0..n {
            if src.next(m)?.is_none() {
                return Ok(None);
            }
        }
    }
    src.next(m)
}

/// `concat`: the first part, and in the demand that finds it empty, the second.
fn concat_next(a: &mut Node, b: &mut Node, in_b: &mut bool, m: &mut Machine) -> Pulled {
    if !*in_b {
        match a.next(m)? {
            Some(x) => return Ok(Some(x)),
            None => *in_b = true,
        }
    }
    b.next(m)
}

/// `mapcat`: the rest of the last result, or the result for the next source
/// element that has one.
fn mapcat_next(
    src: &mut Node,
    f: &V,
    inner: &mut Vec<i64>,
    at: &mut usize,
    m: &mut Machine,
) -> Pulled {
    loop {
        if let Some(x) = inner.get(*at).copied() {
            *at += 1;
            return Ok(Some(x));
        }
        let Some(x) = src.next(m)? else {
            return Ok(None);
        };
        *inner = ints_of(m.apply(f, vec![V::Int(x)])?)?;
        *at = 0;
    }
}

/// A seq realised once: what a bound name holds.
struct Memo {
    node: Node,
    seen: Vec<i64>,
    ended: bool,
}

impl Memo {
    /// Element `at`, realising nodes as needed.
    fn get(&mut self, at: usize, m: &mut Machine) -> Pulled {
        while at >= self.seen.len() && !self.ended {
            match self.node.next(m)? {
                Some(x) => self.seen.push(x),
                None => self.ended = true,
            }
        }
        Ok(self.seen.get(at).copied())
    }

    fn all(&mut self, m: &mut Machine) -> Result<Vec<i64>, Stop> {
        let mut at = 0;
        while self.get(at, m)?.is_some() {
            at += 1;
        }
        Ok(self.seen.clone())
    }
}

/// The fold `digest` makes of a `Vec`: every element and its place.
pub fn digest(xs: &[i64]) -> i64 {
    xs.iter().fold(7, |h, x| (h * 31 + x) % 1_000_003)
}

fn add(a: i64, b: i64) -> Result<i64, Stop> {
    a.checked_add(b)
        .ok_or_else(|| trap("integer overflow in + at i64"))
}

impl Machine<'_> {
    /// The value of a pipeline node.
    pub(super) fn ev_pipe(&mut self, p: &Pipe, env: &Env) -> Res {
        let node = self.build(&p.chain, env)?;
        let mut memo = Memo {
            node,
            seen: Vec::new(),
            ended: false,
        };
        let mut total = 0;
        for (i, t) in p.terms.iter().enumerate() {
            let v = self.run_term(t, &mut memo, env)?;
            total = if i == 0 {
                v
            } else {
                add(
                    total
                        .checked_mul(7)
                        .ok_or_else(|| trap("integer overflow in * at i64"))?,
                    v,
                )?
            };
        }
        Ok(V::Int(total))
    }

    /// The nodes of `c`, with the sizes, functions and operands of its
    /// stages evaluated in the order they are applied.
    fn build(&mut self, c: &Chain, env: &Env) -> Result<Node, Stop> {
        let mut node = match &c.src {
            Src::Vec(e) | Src::List(e) => Node::items(ints_of(self.ev(e, env)?)?),
            Src::Range(n) => Node::items((0..int_of(self.ev(n, env)?)?).collect()),
        };
        for s in &c.stages {
            node = self.stage(s, node, env)?;
        }
        Ok(node)
    }

    fn stage(&mut self, s: &Stage, src: Node, env: &Env) -> Result<Node, Stop> {
        let src = Box::new(src);
        Ok(match s {
            Stage::Map(e) => Node::Map {
                src,
                f: self.ev(e, env)?,
            },
            Stage::Filter(e) => Node::Filter {
                src,
                p: self.ev(e, env)?,
                keep: true,
            },
            Stage::Remove(e) => Node::Filter {
                src,
                p: self.ev(e, env)?,
                keep: false,
            },
            Stage::TakeWhile(e) => Node::TakeWhile {
                src,
                p: self.ev(e, env)?,
            },
            Stage::Mapcat(e) => Node::Mapcat {
                src,
                f: self.ev(e, env)?,
                inner: Vec::new(),
                at: 0,
            },
            Stage::Take(e) => Node::Take {
                src,
                left: int_of(self.ev(e, env)?)?,
            },
            Stage::Drop(e) => Node::Drop {
                src,
                n: int_of(self.ev(e, env)?)?,
                started: false,
            },
            Stage::Concat { other, first } => {
                let o = Box::new(self.operand(other, env)?);
                let (a, b) = if *first { (o, src) } else { (src, o) };
                Node::Concat { a, b, in_b: false }
            }
        })
    }

    fn operand(&mut self, o: &Operand, env: &Env) -> Result<Node, Stop> {
        match o {
            Operand::Vec(e) => Ok(Node::items(ints_of(self.ev(e, env)?)?)),
            Operand::Lazy(c) => self.build(c, env),
            Operand::Eager(c) => {
                let mut n = self.build(c, env)?;
                Ok(Node::items(n.drain(self)?))
            }
        }
    }

    /// What `t` makes of the seq, as the number the program folds it to.
    fn run_term(&mut self, t: &Term, memo: &mut Memo, env: &Env) -> Result<i64, Stop> {
        match t {
            Term::Reduce { f, init, stop } => {
                let (f, mut acc) = (self.ev(f, env)?, int_of(self.ev(init, env)?)?);
                let mut at = 0;
                while let Some(x) = memo.get(at, self)? {
                    if stop.is_some_and(|t| acc > t) {
                        break;
                    }
                    acc = int_of(self.apply(&f, vec![V::Int(acc), V::Int(x)])?)?;
                    at += 1;
                }
                Ok(acc)
            }
            Term::Reduce1(f) => {
                let f = self.ev(f, env)?;
                let mut acc = memo
                    .get(0, self)?
                    .ok_or_else(|| trap("reduce: empty collection"))?;
                let mut at = 1;
                while let Some(x) = memo.get(at, self)? {
                    acc = int_of(self.apply(&f, vec![V::Int(acc), V::Int(x)])?)?;
                    at += 1;
                }
                Ok(acc)
            }
            Term::Count => Ok(memo.all(self)?.len() as i64),
            Term::Vec => Ok(digest(&memo.all(self)?)),
            Term::First => Ok(memo.get(0, self)?.unwrap_or(NONE)),
            Term::Last => Ok(memo.all(self)?.last().copied().unwrap_or(NONE)),
            Term::Empty => Ok(i64::from(memo.get(0, self)?.is_none())),
            Term::Sum => memo.all(self)?.into_iter().try_fold(0, add),
            Term::Sort => {
                let mut xs = memo.all(self)?;
                xs.sort();
                Ok(digest(&xs))
            }
            _ => self.run_term_with(t, memo, env),
        }
    }

    /// The terminals that take a function or an index.
    fn run_term_with(&mut self, t: &Term, memo: &mut Memo, env: &Env) -> Result<i64, Stop> {
        match t {
            Term::Every(p) | Term::FindFirst(p) => {
                let (p, mut at) = (self.ev(p, env)?, 0);
                let every = matches!(t, Term::Every(_));
                while let Some(x) = memo.get(at, self)? {
                    let hit = bool_of(self.apply(&p, vec![V::Int(x)])?)?;
                    match (every, hit) {
                        (true, false) => return Ok(0),
                        (false, true) => return Ok(x + 1),
                        _ => at += 1,
                    }
                }
                Ok(if every { 1 } else { NONE })
            }
            Term::SortBy(k) => {
                let key = self.ev(k, env)?;
                let mut keyed = Vec::new();
                for x in memo.all(self)? {
                    keyed.push((int_of(self.apply(&key, vec![V::Int(x)])?)?, x));
                }
                keyed.sort_by_key(|(k, _)| *k);
                let xs: Vec<i64> = keyed.into_iter().map(|(_, x)| x).collect();
                Ok(digest(&xs))
            }
            Term::Nth(i) => {
                let i = int_of(self.ev(i, env)?)?;
                let hit = match usize::try_from(i) {
                    Ok(at) => memo.get(at, self)?,
                    Err(_) => memo.all(self).map(|_| None)?,
                };
                hit.ok_or_else(|| trap("nth: index out of range"))
            }
            other => Err(unsupported(format!("terminal {other:?}"))),
        }
    }
}

#[cfg(test)]
mod tests;
