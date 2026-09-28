//! The generator: well-typed programs by construction, from a
//! type-directed grammar (`expr` produces an expression of a requested
//! type in a context), deterministic in its seed.
//!
//! Every production keeps the program inside the rules the spec states
//! for acceptance: `&` arguments name distinct cell variables (syntax
//! §3.13 rule 1), closures that capture an `&` parameter do not escape
//! (rule 2), `async` functions have no `&` parameters (rule 3), code that
//! runs on another thread or as a task captures only sendable values
//! (types §5), `recur` only in tail position (§3.18), exhaustive and
//! non-redundant matches (§3.6). Programs are bounded: every loop and
//! recursion runs a literal number of times, and nothing traps.

mod arrays;
mod consts;
mod control;
mod ctx;
mod derive;
mod effects;
mod funcs;
mod gadgets;
mod gadgets2;
mod helpers;
mod hooks;
mod inout;
mod jobs;
mod mcalls;
mod nums;
mod objects;
mod observe;
mod protos;
mod protos2;
mod scalar;
mod tasks;
mod vpat;
mod vpat2;

pub use ctx::{Ctx, Region, Var, VarKind};

use crate::ast::{Expr, FunDef, ImplDef, Kind, Param, Program};
use crate::rng::Rng;
use crate::ty::{universe, Ty};

/// Generator state for one program.
pub struct Gen {
    /// The random source.
    pub rng: Rng,
    /// The next fresh-name number.
    next: u32,
    /// The helper functions made so far.
    pub funs: Vec<FunDef>,
    /// The most helpers a program may have.
    max_funs: usize,
    /// Nodes produced so far; past `node_budget` only leaves are made.
    nodes: usize,
    node_budget: usize,
    /// The types that `let`s bind and programs fold.
    universe: Vec<Ty>,
    /// The `def` constants, as variables every body sees.
    defs: Vec<Var>,
    /// The program's `impl`s (empty when it declares no protocols).
    pub impls: Vec<ImplDef>,
    /// Whether protocol method calls and `dyn` values may be made: not
    /// in a program without `impl`s, and not while the `impl` bodies
    /// themselves are made (a method body that called a method could
    /// recurse without end).
    pub methods_ok: bool,
}

/// The program for `seed` at `size` (1 = tiny; the default run uses 1..=6).
pub fn generate(seed: u64, size: u32) -> Program {
    let size = size.max(1);
    let mut g = Gen {
        rng: Rng::new(seed),
        next: 0,
        funs: Vec::new(),
        max_funs: 1 + 2 * size as usize,
        nodes: 0,
        node_budget: 60 * size as usize,
        universe: universe(),
        defs: Vec::new(),
        impls: Vec::new(),
        methods_ok: false,
    };
    let (defs, vars) = consts::defs(&mut g, (size as usize).min(3));
    g.defs = vars;
    g.impls = protos::impls(&mut g);
    g.methods_ok = !g.impls.is_empty();
    let depth = size + 2;
    let main = g.expr(&g.body_ctx(&[]), &Ty::Int, depth);
    Program {
        defs,
        impls: g.impls,
        funs: g.funs,
        main,
    }
}

impl Gen {
    /// The context of a top-level body: the `def`s, then the parameters.
    pub fn body_ctx(&self, params: &[Param]) -> Ctx {
        let mut vars = self.defs.clone();
        vars.extend(params.iter().map(|p| Var {
            name: p.name.clone(),
            ty: p.ty.clone(),
            kind: if p.inout {
                VarKind::InOut
            } else {
                VarKind::Param
            },
            send_fn: false,
        }));
        Ctx::top(vars, false)
    }

    /// A fresh name with `prefix`.
    pub fn fresh(&mut self, prefix: &str) -> String {
        self.next += 1;
        format!("{prefix}{}", self.next)
    }

    /// A random type from the universe that `cx` may bind: atoms only
    /// where they may be read.
    pub fn any_type(&mut self, cx: &Ctx) -> Ty {
        loop {
            let i = self.rng.below(self.universe.len());
            let t = self.universe[i].clone();
            if (cx.atoms_readable() || !t.is_atom()) && self.may_make(&t) {
                return t;
            }
        }
    }

    /// A random sendable, non-atom type (for `plet` and task results).
    pub fn send_type(&mut self) -> Ty {
        loop {
            let i = self.rng.below(self.universe.len());
            let t = self.universe[i].clone();
            if t.is_send() && !t.is_atom() && t != Ty::Bool && self.may_make(&t) {
                return t;
            }
        }
    }

    /// Whether values of `t` may be made here: `dyn` types only where
    /// method calls may be.
    pub fn may_make(&self, t: &Ty) -> bool {
        self.methods_ok || !t.needs_protocols()
    }

    /// Whether `t` is in the universe (and may be made here).
    pub fn in_universe(&self, t: &Ty) -> bool {
        self.universe.contains(t) && self.may_make(t)
    }

    /// A small integer literal.
    pub fn small(&mut self) -> i64 {
        self.rng.range(-3, 9)
    }

    /// An expression of type `ty` in `cx`, at most `d` levels deep.
    pub fn expr(&mut self, cx: &Ctx, ty: &Ty, d: u32) -> Expr {
        self.nodes += 1;
        if d == 0 || self.nodes > self.node_budget {
            return self.leaf(cx, ty);
        }
        if ty.is_atom() && !cx.atoms_readable() {
            return self.leaf_value(cx, ty);
        }
        let has_var = !cx.vars_of(ty).is_empty();
        let can_task = ty.is_send() && *ty != Ty::Unit && self.in_universe(&Ty::task(ty.clone()));
        let weights = [
            if has_var { 3 } else { 0 },                 // variable
            3,                                           // let
            2,                                           // if
            1,                                           // do
            2,                                           // match
            if *ty == Ty::Unit { 1 } else { 2 },         // loop
            2,                                           // helper call
            9,                                           // type-specific
            if can_task { 1 } else { 0 },                // join a task
            if can_task && cx.in_async { 2 } else { 0 }, // await a task
        ];
        let e = match self.rng.weighted(&weights) {
            Some(0) => self.var_of(cx, ty),
            Some(1) => control::let_form(self, cx, ty, d),
            Some(2) => control::if_form(self, cx, ty, d),
            Some(3) => control::do_form(self, cx, ty, d),
            Some(4) => control::match_form(self, cx, ty, d),
            Some(5) => control::loop_form(self, cx, ty, d),
            Some(6) => funcs::helper_call(self, cx, ty, d),
            Some(8) => tasks::join(self, cx, ty, d),
            Some(9) => tasks::await_form(self, cx, ty, d),
            _ => None,
        };
        match e {
            Some(e) => e,
            None => self.specific(cx, ty, d),
        }
    }

    /// A production particular to `ty`.
    fn specific(&mut self, cx: &Ctx, ty: &Ty, d: u32) -> Expr {
        let e = match ty {
            Ty::Int => Some(scalar::int(self, cx, d)),
            Ty::Bool => Some(scalar::boolean(self, cx, d)),
            Ty::Str => Some(scalar::string(self, cx, d)),
            Ty::Unit => Some(effects::stmt(self, cx, d)),
            Ty::Func(..) => Some(funcs::function(self, cx, ty, d)),
            Ty::Task(t) => Some(tasks::task(self, cx, t, d)),
            Ty::Dyn(p, send) => Some(protos::dyn_value(self, cx, *p, *send, d)),
            Ty::Hook(send) => Some(hooks::hook(self, cx, *send, d)),
            Ty::Job(send) => Some(jobs::job(self, cx, *send, d)),
            _ => objects::object(self, cx, ty, d),
        };
        e.unwrap_or_else(|| self.leaf(cx, ty))
    }

    /// A variable of type `ty` (a leaf if there is none).
    pub fn var_of(&mut self, cx: &Ctx, ty: &Ty) -> Option<Expr> {
        let vs = cx.vars_of(ty);
        let v = self.rng.pick(&vs)?;
        Some(Expr::var(&v.name, ty.clone()))
    }

    /// A small expression of type `ty`: a variable or a literal-like value.
    pub fn leaf(&mut self, cx: &Ctx, ty: &Ty) -> Expr {
        if (cx.atoms_readable() || !ty.is_atom()) && self.rng.chance(60) {
            if let Some(v) = self.var_of(cx, ty) {
                return v;
            }
        }
        self.leaf_value(cx, ty)
    }

    /// A literal-like value of type `ty` that mentions no variable.
    pub fn leaf_value(&mut self, cx: &Ctx, ty: &Ty) -> Expr {
        let lit = |k: Kind| Expr::new(ty.clone(), k);
        match ty {
            Ty::Int => Expr::int(self.small()),
            Ty::Bool => lit(Kind::Bool(self.rng.chance(50))),
            Ty::Str => lit(Kind::Str(scalar::literal_text(self))),
            Ty::Unit => lit(Kind::Unit),
            Ty::Opt(_) | Ty::Vec(_) | Ty::List => objects::empty(self, cx, ty),
            Ty::Func(ps, _) => funcs::const_fn(self, ps.len()),
            Ty::Task(t) => {
                let body = self.leaf_value(cx, t);
                lit(Kind::Async(Box::new(body)))
            }
            Ty::Dyn(p, send) => protos::dyn_leaf(self, *p, *send),
            Ty::Hook(send) => hooks::hook_leaf(self, *send),
            Ty::Job(send) => jobs::job_leaf(self, *send),
            Ty::Weak(t) => {
                let n = self.fresh("dead");
                let target = objects::fresh_object(self, cx, t);
                lit(Kind::WeakDead(n, Box::new(target)))
            }
            _ => objects::construct_leaf(self, cx, ty),
        }
    }
}

#[cfg(test)]
mod tests;
