//! Elaboration after inference (stdlib §7 L20): the forms the checker
//! typed by truthiness (`Guarded`, `And`, `Or`, `Elided`) and the tests
//! that are an `(Option T)` become plain `if`, `match`, `some` and `nil`,
//! with the types the checker found, so that the ownership pass, the
//! interpreter and the compiler read only the core forms.
//!
//! An `(Option T)` is truthy when it is `(some _)`; `(Option bool)` when it
//! is `(some true)`, as Clojure's `false` is falsy. The payload is read
//! from the type as the definition has it: a type variable is never
//! `bool`, whatever it is instantiated at.

use crate::syntax::Pos;

use super::ast::{
    Arg, BindingId, BindingInfo, BindingKind, Clause, Expr, ExprId, ExprKind, GlobalRef, Lit,
    PatKind, Pattern, Place,
};
use super::decls::Globals;
use super::infer::{Instantiation, Tables};
use super::ty::{Colour, Con, Scalar, Ty};

/// Elaborates every body of the program.
pub(crate) fn elaborate(g: &mut Globals, t: &mut Tables) {
    let mut el = Elab { g, t };
    for i in 0..el.g.funs.len() {
        let mut body = el.take(|g| &mut g.funs[i].body);
        el.expr(&mut body);
        el.g.funs[i].body = body;
    }
    for i in 0..el.g.defs.len() {
        let mut init = el.take(|g| &mut g.defs[i].init);
        el.expr(&mut init);
        el.g.defs[i].init = init;
    }
    for i in 0..el.g.instances.len() {
        for m in 0..el.g.instances[i].methods.len() {
            let mut body = el.take(|g| &mut g.instances[i].methods[m].body);
            el.expr(&mut body);
            el.g.instances[i].methods[m].body = body;
        }
    }
}

struct Elab<'a> {
    g: &'a mut Globals,
    t: &'a mut Tables,
}

/// How a test of this type is read.
enum Truth {
    /// A `bool`.
    Plain,
    /// An `(Option T)`: truthy when `(some _)`.
    Some,
    /// An `(Option bool)`: truthy when `(some true)`.
    SomeTrue,
}

impl Elab<'_> {
    /// Moves a body out of the globals, leaving a unit in its place.
    fn take(&mut self, at: impl Fn(&mut Globals) -> &mut Expr) -> Expr {
        let id = ExprId(0);
        let slot = at(self.g);
        let pos = slot.pos.clone();
        let unit = Expr {
            id,
            pos,
            kind: ExprKind::Lit(Lit::Unit),
        };
        std::mem::replace(slot, unit)
    }

    fn ty(&self, e: &Expr) -> Ty {
        self.t
            .expr_types
            .get(&e.id)
            .cloned()
            .unwrap_or_else(Ty::unit)
    }

    fn payload(&self, t: &Ty) -> Option<Ty> {
        match t {
            Ty::Con(Con::Nominal(id), args) if *id == self.g.option => args.first().cloned(),
            _ => None,
        }
    }

    fn truth(&self, t: &Ty) -> Truth {
        match self.payload(t) {
            Some(Ty::Con(Con::Scalar(Scalar::Bool), _)) => Truth::SomeTrue,
            Some(_) => Truth::Some,
            None => Truth::Plain,
        }
    }

    fn is_unit(t: &Ty) -> bool {
        matches!(t, Ty::Con(Con::Scalar(Scalar::Unit), _))
    }

    fn expr(&mut self, e: &mut Expr) {
        children_mut(e, &mut |c| self.expr(c));
        let kind = std::mem::replace(&mut e.kind, ExprKind::Lit(Lit::Unit));
        e.kind = match kind {
            ExprKind::If(c, t, f) => {
                let c = self.test(*c);
                ExprKind::If(Box::new(c), t, f)
            }
            ExprKind::Guarded(cs) => self.guarded(e, cs),
            ExprKind::And(ops) => self.and(e, ops),
            ExprKind::Or(ops) => self.or(e, ops),
            ExprKind::Elided => {
                let t = self.ty(e);
                if self.payload(&t).is_some() {
                    self.instantiate(e.id, &t);
                    ExprKind::Global(GlobalRef::Ctor(self.g.option, Some(0)))
                } else {
                    ExprKind::Lit(Lit::Unit)
                }
            }
            other => other,
        };
    }

    fn node(&mut self, pos: &Pos, kind: ExprKind, ty: &Ty) -> Expr {
        let id = self.g.next_expr();
        self.t.expr_types.insert(id, ty.clone());
        Expr {
            id,
            pos: pos.clone(),
            kind,
        }
    }

    fn lit_bool(&mut self, b: bool, pos: &Pos) -> Expr {
        self.node(pos, ExprKind::Lit(Lit::Bool(b)), &Ty::bool())
    }

    /// `nil` of the type `ty`, an `(Option T)`.
    fn nil(&mut self, ty: &Ty, pos: &Pos) -> Expr {
        let kind = ExprKind::Global(GlobalRef::Ctor(self.g.option, Some(0)));
        let e = self.node(pos, kind, ty);
        self.instantiate(e.id, ty);
        e
    }

    fn instantiate(&mut self, site: ExprId, option: &Ty) {
        let tys = self.payload(option).into_iter().collect();
        let inst = Instantiation {
            tys,
            colours: Vec::new(),
        };
        self.t.instantiations.insert(site, inst);
    }

    /// `(some x)` of the type `ty`, an `(Option T)`.
    fn some(&mut self, x: Expr, ty: &Ty) -> Expr {
        let pos = x.pos.clone();
        let payload = self.payload(ty).unwrap_or_else(|| self.ty(&x));
        let fty = Ty::Fn(Colour::Send, vec![payload], Box::new(ty.clone()));
        let kind = ExprKind::Global(GlobalRef::Ctor(self.g.option, Some(1)));
        let head = self.node(&pos, kind, &fty);
        self.instantiate(head.id, ty);
        let call = ExprKind::Call(Box::new(head), vec![Arg::Expr(x)]);
        self.node(&pos, call, ty)
    }

    /// The pattern `(some p)`, `p` being `true` for an `(Option bool)`.
    fn some_pattern(&self, truth: &Truth, pos: &Pos, inner: Option<Pattern>) -> Pattern {
        let inner = inner.unwrap_or_else(|| Pattern {
            pos: pos.clone(),
            kind: match truth {
                Truth::SomeTrue => PatKind::Lit(Lit::Bool(true)),
                _ => PatKind::Wild,
            },
        });
        Pattern {
            pos: pos.clone(),
            kind: PatKind::Ctor(self.g.option, Some(1), vec![inner]),
        }
    }

    fn wild(pos: &Pos) -> Pattern {
        Pattern {
            pos: pos.clone(),
            kind: PatKind::Wild,
        }
    }

    /// A test as a `bool` expression.
    fn test(&mut self, c: Expr) -> Expr {
        let ty = self.ty(&c);
        let truth = self.truth(&ty);
        if matches!(truth, Truth::Plain) {
            return c;
        }
        let pos = c.pos.clone();
        let yes = self.lit_bool(true, &pos);
        let no = self.lit_bool(false, &pos);
        let clauses = vec![
            clause(self.some_pattern(&truth, &pos, None), yes),
            clause(Self::wild(&pos), no),
        ];
        self.node(&pos, ExprKind::Match(Box::new(c), clauses), &Ty::bool())
    }

    /// The node that stands for `e`: the expression `top` built for it,
    /// whose own id and type are not wanted.
    fn finish(&mut self, top: Expr) -> ExprKind {
        self.t.expr_types.remove(&top.id);
        self.t.instantiations.remove(&top.id);
        top.kind
    }

    /// The clauses `(test body)` as nested `if`s: unit, or `some` around
    /// each body and `nil` at the end, as the type of the whole says.
    fn guarded(&mut self, e: &Expr, cs: Vec<(Expr, Expr)>) -> ExprKind {
        let ty = self.ty(e);
        let unit = Self::is_unit(&ty);
        let mut rest = if unit {
            self.node(&e.pos, ExprKind::Lit(Lit::Unit), &ty)
        } else {
            self.nil(&ty, &e.pos)
        };
        for (test, body) in cs.into_iter().rev() {
            let test = self.test(test);
            let body = if unit { body } else { self.some(body, &ty) };
            let kind = ExprKind::If(Box::new(test), Box::new(body), Box::new(rest));
            rest = self.node(&e.pos, kind, &ty);
        }
        self.finish(rest)
    }

    /// `(and a b)` is `(if a b <false>)`, the false being `false` or `nil`.
    fn and(&mut self, e: &Expr, ops: Vec<Expr>) -> ExprKind {
        let ty = self.ty(e);
        let mut it = ops.into_iter().rev();
        let Some(mut rest) = it.next() else {
            return ExprKind::Lit(Lit::Bool(true));
        };
        for op in it {
            let falsy = self.falsy(&ty, &e.pos);
            let test = self.test(op);
            let kind = ExprKind::If(Box::new(test), Box::new(rest), Box::new(falsy));
            rest = self.node(&e.pos, kind, &ty);
        }
        self.finish(rest)
    }

    /// `(or a b)` is the value of `a` when it is truthy, else `b`.
    fn or(&mut self, e: &Expr, ops: Vec<Expr>) -> ExprKind {
        let ty = self.ty(e);
        let mut it = ops.into_iter().rev();
        let Some(mut rest) = it.next() else {
            return ExprKind::Lit(Lit::Bool(false));
        };
        for op in it {
            let kind = self.or_step(op, rest, &ty, &e.pos);
            rest = self.node(&e.pos, kind, &ty);
        }
        self.finish(rest)
    }

    fn falsy(&mut self, ty: &Ty, pos: &Pos) -> Expr {
        if self.payload(ty).is_some() {
            self.nil(ty, pos)
        } else {
            self.lit_bool(false, pos)
        }
    }

    fn or_step(&mut self, op: Expr, rest: Expr, ty: &Ty, pos: &Pos) -> ExprKind {
        let op_ty = self.ty(&op);
        let truth = self.truth(&op_ty);
        let result_is_bool = matches!(ty, Ty::Con(Con::Scalar(Scalar::Bool), _));
        if matches!(truth, Truth::Plain) || result_is_bool {
            let yes = self.lit_bool(true, pos);
            let test = self.test(op);
            return ExprKind::If(Box::new(test), Box::new(yes), Box::new(rest));
        }
        // The value of an `(Option T)` operand: itself when the result is
        // an `Option` (`s@(some _)`), its payload when it is a `T`.
        let payload = self.payload(&op_ty).unwrap_or_else(Ty::unit);
        let keep_whole = self.payload(ty).is_some();
        let b = self.binding(if keep_whole { &op_ty } else { &payload }, pos);
        let inner = (!keep_whole).then(|| Pattern {
            pos: pos.clone(),
            kind: PatKind::Bind(b),
        });
        let mut pat = self.some_pattern(&truth, pos, inner);
        if keep_whole {
            pat = Pattern {
                pos: pos.clone(),
                kind: PatKind::As(Box::new(pat), b),
            };
        }
        let local_ty = if keep_whole { &op_ty } else { &payload };
        let local = self.node(pos, ExprKind::Local(b), local_ty);
        let clauses = vec![clause(pat, local), clause(Self::wild(pos), rest)];
        ExprKind::Match(Box::new(op), clauses)
    }

    fn binding(&mut self, ty: &Ty, pos: &Pos) -> BindingId {
        let id = BindingId(self.g.bindings.len() as u32);
        self.g.bindings.push(BindingInfo {
            name: "or-value".to_string(),
            kind: BindingKind::Pattern,
            pos: pos.clone(),
            ann: None,
        });
        self.t.binding_types.insert(id, ty.clone());
        id
    }
}

fn clause(pat: Pattern, body: Expr) -> Clause {
    Clause {
        pat,
        guard: None,
        body,
        fallback: false,
    }
}

/// The direct sub-expressions of `e`, mutably.
fn children_mut(e: &mut Expr, f: &mut dyn FnMut(&mut Expr)) {
    match &mut e.kind {
        ExprKind::Lit(_)
        | ExprKind::Local(_)
        | ExprKind::Global(_)
        | ExprKind::Quote(_)
        | ExprKind::Elided => {}
        ExprKind::Guarded(cs) => {
            for (t, b) in cs {
                f(t);
                f(b);
            }
        }
        ExprKind::And(es) | ExprKind::Or(es) => es.iter_mut().for_each(f),
        ExprKind::Call(h, args) => {
            f(h);
            for a in args {
                if let Arg::Expr(x) = a {
                    f(x);
                }
            }
        }
        ExprKind::Fn(lit) => f(&mut lit.body),
        ExprKind::Let(bs, body) => {
            bs.iter_mut().for_each(|(_, x)| f(x));
            f(body);
        }
        ExprKind::If(c, t, x) => {
            f(c);
            f(t);
            f(x);
        }
        ExprKind::Do(es) | ExprKind::Recur(es) | ExprKind::Concat(es) => es.iter_mut().for_each(f),
        ExprKind::Match(s, cls) => {
            f(s);
            for c in cls {
                if let Some(g) = &mut c.guard {
                    f(g);
                }
                f(&mut c.body);
            }
        }
        ExprKind::Loop(vs, body) => {
            vs.iter_mut().for_each(|(_, x)| f(x));
            f(body);
        }
        ExprKind::Deref(p, _) => place_child(p, f),
        ExprKind::Set(p, v) => {
            place_child(p, f);
            f(v);
        }
        ExprKind::Field(x, _, _)
        | ExprKind::SetField(_, _, x)
        | ExprKind::Async(x, _)
        | ExprKind::Await(x)
        | ExprKind::Unsafe(x)
        | ExprKind::Dyn(_, _, _, x)
        | ExprKind::Convert(_, _, x) => f(x),
    }
}

fn place_child(p: &mut Place, f: &mut dyn FnMut(&mut Expr)) {
    if let Place::Expr(e) = p {
        f(e);
    }
}
