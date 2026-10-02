//! The functions of a generated pipeline: pure `fn` literals over `i64`,
//! small enough that no chain of them overflows (a term is at most six
//! times the larger of its leaves, a chain has at most six stages, and a
//! fold that multiplies reduces its accumulator), with an optional
//! statement that adds a weight to the program's call counter, so that
//! the number of times each function runs reaches the result.

use crate::ast::{Expr, Kind, Pat};
use crate::ty::Ty;

use super::pipelines::PGen;

/// The name of the call counter, a `(Cell i64)` that `main` binds.
pub const CALLS: &str = "calls";

/// The comparison heads of a generated predicate.
const COMPARES: [&str; 4] = ["<", "<=", ">", ">="];

impl PGen {
    /// A literal in the range every leaf draws from.
    fn lit(&mut self) -> Expr {
        Expr::int(self.rng.range(-9, 20))
    }

    /// A leaf: usually a parameter, else a captured number, the size of
    /// the captured vector, or a literal.
    fn leaf(&mut self, params: &[&str]) -> Expr {
        if !params.is_empty() && self.rng.chance(60) {
            return Expr::var(params[self.rng.below(params.len())], Ty::Int);
        }
        let mut pool: Vec<Expr> = self.ints.iter().map(|n| Expr::var(n, Ty::Int)).collect();
        if let Some(cv) = self.cv.clone() {
            pool.push(Expr::call(
                Ty::Int,
                "count",
                vec![Expr::var(&cv, Ty::vec(Ty::Int))],
            ));
        }
        if pool.is_empty() || self.rng.chance(40) {
            return self.lit();
        }
        let i = self.rng.below(pool.len());
        pool.swap_remove(i)
    }

    /// `e`, or a form of `e` and `x` when `e` never mentions `x`: a
    /// function that ignores its argument tests little.
    fn mention(&mut self, e: Expr, x: &str) -> Expr {
        let mut found = false;
        e.walk(&mut |n| found |= matches!(&n.kind, Kind::Var(v) if v == x));
        if found {
            return e;
        }
        let x = || Expr::var(x, Ty::Int);
        if e.ty == Ty::Bool {
            let (a, b) = (self.term(1, &[]), self.term(1, &[]));
            let (yes, no) = (
                Expr::call(Ty::Bool, "<", vec![x(), a]),
                Expr::call(Ty::Bool, ">", vec![x(), b]),
            );
            return Expr::new(Ty::Bool, Kind::If(Box::new(e), Box::new(yes), Box::new(no)));
        }
        Expr::call(Ty::Int, "+", vec![x(), e])
    }

    /// An `i64` term over `params`, at most `d` operators deep.
    pub(super) fn term(&mut self, d: u32, params: &[&str]) -> Expr {
        if d == 0 || self.rng.chance(25) {
            return self.leaf(params);
        }
        let call = |h: &str, a: Vec<Expr>| Expr::call(Ty::Int, h, a);
        match self.rng.below(6) {
            0 => call(
                "+",
                vec![self.term(d - 1, params), self.term(d - 1, params)],
            ),
            1 => call(
                "-",
                vec![self.term(d - 1, params), self.term(d - 1, params)],
            ),
            2 => {
                let k = Expr::int(self.rng.range(-3, 3));
                call("*", vec![self.term(d - 1, params), k])
            }
            3 => {
                let m = Expr::int(self.rng.range(2, 9));
                call("rem", vec![self.term(d - 1, params), m])
            }
            _ => {
                let c = self.pred(d - 1, params);
                let (a, b) = (self.term(d - 1, params), self.term(d - 1, params));
                Expr::new(Ty::Int, Kind::If(Box::new(c), Box::new(a), Box::new(b)))
            }
        }
    }

    /// A `bool` over `params`, at most `d` operators deep.
    pub(super) fn pred(&mut self, d: u32, params: &[&str]) -> Expr {
        let call = |h: &str, a: Vec<Expr>| Expr::call(Ty::Bool, h, a);
        if d > 0 && self.rng.chance(35) {
            let (p, q) = (self.pred(d - 1, params), self.pred(d - 1, params));
            let yes = |b| Expr::new(Ty::Bool, Kind::Bool(b));
            return match self.rng.below(3) {
                0 => call("not", vec![p]),
                1 => Expr::new(
                    Ty::Bool,
                    Kind::If(Box::new(p), Box::new(q), Box::new(yes(false))),
                ),
                _ => Expr::new(
                    Ty::Bool,
                    Kind::If(Box::new(p), Box::new(yes(true)), Box::new(q)),
                ),
            };
        }
        if self.rng.chance(40) {
            let m = self.rng.range(2, 5);
            let r = Expr::call(Ty::Int, "rem", vec![self.term(1, params), Expr::int(m)]);
            return call("=", vec![r, Expr::int(self.rng.range(0, m - 1))]);
        }
        let h = COMPARES[self.rng.below(COMPARES.len())];
        call(h, vec![self.term(1, params), self.term(1, params)])
    }

    /// `body`, preceded by the statement that adds a weight to the call
    /// counter when the program counts calls and `count` allows it.
    fn counting(&mut self, body: Expr, count: bool) -> Expr {
        if !(self.counted && count) {
            return body;
        }
        let w = Expr::int(self.rng.range(1, 7));
        let cell = || Expr::var(CALLS, Ty::cell(Ty::Int));
        let read = Expr::new(Ty::Int, Kind::Deref(Box::new(cell())));
        let bump = Expr::call(Ty::Int, "+", vec![read, w]);
        let set = Expr::new(Ty::Unit, Kind::Set(Box::new(cell()), Box::new(bump)));
        Expr::new(body.ty.clone(), Kind::Do(vec![set, body]))
    }

    fn fun(&mut self, params: &[&str], body: Expr, ret: Ty) -> Expr {
        let ps = params.iter().map(|p| (p.to_string(), Ty::Int)).collect();
        let ty = Ty::Func(vec![Ty::Int; params.len()], Box::new(ret));
        Expr::new(ty, Kind::Fn(ps, Box::new(body)))
    }

    /// `(fn (x: i64) term)`; `count` says whether it may count its calls.
    pub(super) fn int_fn(&mut self, count: bool) -> Expr {
        let t = self.term(2, &["x"]);
        let body = self.mention(t, "x");
        let body = self.counting(body, count);
        self.fun(&["x"], body, Ty::Int)
    }

    /// `(fn (x: i64) pred)`.
    pub(super) fn bool_fn(&mut self) -> Expr {
        let p = self.pred(2, &["x"]);
        let body = self.mention(p, "x");
        let body = self.counting(body, true);
        self.fun(&["x"], body, Ty::Bool)
    }

    /// `(fn (x: i64) (Vec i64))`: one to three elements, or one or none.
    pub(super) fn vec_fn(&mut self) -> Expr {
        let vt = Ty::vec(Ty::Int);
        let lit = |items: Vec<Expr>| Expr::new(Ty::vec(Ty::Int), Kind::VecLit(items));
        let n = 1 + self.rng.below(3);
        let items = (0..n).map(|_| self.term(1, &["x"])).collect();
        let body = if self.rng.chance(50) {
            lit(items)
        } else {
            let c = self.pred(1, &["x"]);
            let rest = if self.rng.chance(50) {
                lit(Vec::new())
            } else {
                lit(vec![self.term(1, &["x"])])
            };
            Expr::new(
                vt.clone(),
                Kind::If(Box::new(c), Box::new(lit(items)), Box::new(rest)),
            )
        };
        let body = self.counting(body, true);
        self.fun(&["x"], body, vt)
    }

    /// `(fn (a: i64 x: i64) ..)`: a fold step whose accumulator stays small.
    pub(super) fn step_fn(&mut self) -> Expr {
        let e = self.term(1, &["x"]);
        let a = || Expr::var("a", Ty::Int);
        let call = |h: &str, args: Vec<Expr>| Expr::call(Ty::Int, h, args);
        let body = match self.rng.below(4) {
            0 => call("+", vec![a(), e]),
            1 => call("-", vec![e, a()]),
            2 => {
                let p = self.pred(1, &["x"]);
                let sum = call("+", vec![a(), e]);
                Expr::new(Ty::Int, Kind::If(Box::new(p), Box::new(sum), Box::new(a())))
            }
            _ => {
                let m = Expr::int(self.rng.range(2, 9));
                let modulus = Expr::int(self.rng.range(97, 9973));
                let grown = call("+", vec![call("*", vec![a(), m]), e]);
                call("rem", vec![grown, modulus])
            }
        };
        let body = self.counting(body, true);
        self.fun(&["a", "x"], body, Ty::Int)
    }

    /// A sort key with ties: a term reduced modulo a small number. It
    /// never counts: how often a sort calls its key is the sort's own.
    pub(super) fn key_fn(&mut self) -> Expr {
        let t = self.term(1, &["x"]);
        let m = Expr::int(self.rng.range(2, 5));
        let body = Expr::call(Ty::Int, "rem", vec![t, m]);
        self.fun(&["x"], body, Ty::Int)
    }

    /// `(let ((name e) ..) body)` over plain names.
    pub(super) fn lets(binds: Vec<(String, Expr)>, body: Expr) -> Expr {
        let ty = body.ty.clone();
        let binds = binds.into_iter().map(|(n, e)| (Pat::Bind(n), e)).collect();
        Expr::new(ty, Kind::Let(binds, Box::new(body)))
    }
}
