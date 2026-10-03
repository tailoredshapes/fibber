//! The mode of each form (§6.2) and the scope, step and join rules of
//! §6.3 for everything but calls and literals.

use crate::types::ast::{Expr, ExprKind, GlobalRef, Lit, PatKind, Pattern, Place};

use super::super::program::{BindKind, Mode, OpKind, Site, Why};
use super::callee::allocates;
use super::pattern::{kind_of, pattern_modes};
use super::{At, ScopeKind, Val, Walker};

const OWNED: Mode = Mode::Owned { immortal: false };
const IMMORTAL: Mode = Mode::Owned { immortal: true };

impl Walker<'_> {
    /// Walks `e`; `tail` is tail position of the frame's body. Records
    /// and returns its mode.
    pub fn expr(&mut self, e: &Expr, tail: bool) -> Mode {
        let m = self.form(e, tail);
        let m = if self.is_object(e) { m } else { Mode::Scalar };
        self.set_mode(e.id, m);
        m
    }

    fn form(&mut self, e: &Expr, tail: bool) -> Mode {
        match &e.kind {
            ExprKind::Lit(Lit::Str(_)) | ExprKind::Quote(_) => IMMORTAL,
            ExprKind::Lit(_) => Mode::Scalar,
            ExprKind::Local(b) => self.read(*b),
            ExprKind::Global(g) => self.global(e, *g),
            ExprKind::Call(h, args) => self.call(e, h, args, tail),
            ExprKind::Fn(lit) => self.fn_lit(e, lit),
            ExprKind::Let(bs, body) => self.let_form(bs, body, tail),
            ExprKind::If(c, t, f) => self.if_form(e, c, t, f, tail),
            ExprKind::Do(es) => self.do_form(es, tail),
            ExprKind::Match(s, cls) => self.match_form(e, s, cls, tail),
            ExprKind::Loop(vs, body) => self.loop_form(vs, body, tail),
            ExprKind::Recur(args) => {
                let r = self.recur(args);
                self.out.recurs.insert(e.id, r);
                Mode::Scalar
            }
            ExprKind::Field(x, _, _) => self.field(x),
            ExprKind::Deref(p, _) => {
                self.place(p);
                OWNED
            }
            ExprKind::Set(p, v) => {
                self.place(p);
                self.store(v);
                Mode::Scalar
            }
            ExprKind::SetField(_, _, v) => {
                self.store(v);
                Mode::Scalar
            }
            ExprKind::Async(b, caps) => self.async_lit(e, b, caps),
            ExprKind::Await(x) => {
                self.operand(x);
                OWNED
            }
            ExprKind::Unsafe(b) => self.expr(b, false),
            ExprKind::Dyn(_, _, _, x) => self.expr(x, false),
            ExprKind::Convert(_, _, x) => {
                self.operand(x);
                Mode::Scalar
            }
            ExprKind::Concat(es) => self.concat(e, es),
            ExprKind::Guarded(_) | ExprKind::And(_) | ExprKind::Or(_) | ExprKind::Elided => {
                Mode::Scalar
            }
        }
    }

    fn if_form(&mut self, e: &Expr, c: &Expr, t: &Expr, f: &Expr, tail: bool) -> Mode {
        self.step(c, false);
        let mt = self.step(t, tail);
        let mf = self.step(f, tail);
        self.join(e, &[(t, mt), (f, mf)])
    }

    /// The target of `deref` or `set!`: an `&` parameter's private cell,
    /// or an operand.
    fn place(&mut self, p: &Place) {
        if let Place::Expr(c) = p {
            self.operand(c);
        }
    }

    fn global(&mut self, e: &Expr, g: GlobalRef) -> Mode {
        match g {
            GlobalRef::Def(d) => Mode::Borrowed(Site::Global(d)),
            GlobalRef::Ctor(t, _) => {
                let alloc = if t == self.cx.p.globals.option {
                    self.option_alloc(e)
                } else {
                    Some(super::super::program::Alloc::Heap)
                };
                if let Some(alloc) = alloc {
                    self.out.allocs.insert(e.id, alloc);
                }
                OWNED
            }
            // A named function, method or builtin as a value: its
            // immortal closure (§8.2, §8.4).
            _ => IMMORTAL,
        }
    }

    /// Registers the `Owned` value of `x` as a temporary of the
    /// innermost scope, a §6.11 candidate if `x` allocates. An immortal
    /// value is none: its count operations are no-ops (§6.1).
    pub(super) fn temp(&mut self, x: &Expr) {
        if self
            .out
            .exprs
            .get(&x.id)
            .is_some_and(|o| o.mode == IMMORTAL)
        {
            return;
        }
        let s = Site::Value(x.id);
        self.own_site(s, true);
        if allocates(self, x) {
            self.candidates.insert(s, x.id);
        }
    }

    /// A step (syntax §2): its temporaries die at its end, by the
    /// scope-exit rule.
    pub(super) fn step(&mut self, e: &Expr, tail: bool) -> Mode {
        self.push_scope(ScopeKind::Step);
        let m = self.expr(e, tail);
        let scope = self.pop_scope();
        self.exit_scope(scope, e.id, m)
    }

    /// An operand that is only read (a `deref` place, an `await`, a
    /// conversion): an `Owned` one is a temporary of the step.
    fn operand(&mut self, x: &Expr) -> Mode {
        let m = self.expr(x, false);
        let val = if matches!(m, Mode::Owned { .. }) {
            self.temp(x);
            Some(self.val(x, m).unwrap_or(Val::B(Site::Value(x.id))))
        } else {
            self.val(x, m)
        };
        self.note(val, At::Other);
        m
    }

    /// E2: `x` is stored (a `set!` value, a `set-field!` value).
    fn store(&mut self, x: &Expr) {
        let m = self.expr(x, false);
        let val = self.val(x, m);
        self.note(val, At::Store);
        if matches!(m, Mode::Borrowed(_) | Mode::Derived(_)) {
            self.push_op(x.id, OpKind::Retain, Site::Value(x.id), Why::Store);
        }
    }

    fn field(&mut self, x: &Expr) -> Mode {
        match self.expr(x, false) {
            Mode::Borrowed(s) | Mode::Derived(s) => Mode::Derived(s),
            Mode::Owned { .. } => {
                self.temp(x);
                Mode::Derived(Site::Value(x.id))
            }
            Mode::Scalar => Mode::Scalar,
        }
    }

    fn concat(&mut self, e: &Expr, es: &[Expr]) -> Mode {
        self.push_scope(ScopeKind::Step);
        for x in es {
            self.operand(x);
        }
        let scope = self.pop_scope();
        self.exit_scope(scope, e.id, OWNED)
    }

    fn do_form(&mut self, es: &[Expr], tail: bool) -> Mode {
        let Some((last, steps)) = es.split_last() else {
            return Mode::Scalar;
        };
        for s in steps {
            let m = self.step(s, false);
            let val = self.val(s, m);
            if matches!(m, Mode::Owned { .. }) {
                let site = Site::Value(s.id);
                if allocates(self, s) {
                    self.candidates.insert(site, s.id);
                }
                let op = self.end_op(site, Why::Discard);
                self.add_after(s.id, op);
                self.note(Some(val.unwrap_or(Val::B(site))), At::Other);
            } else {
                self.note(val, At::Other);
            }
        }
        self.expr(last, tail)
    }

    /// The join of §6.3; retains go on the branches that are not
    /// `Owned`. A branch with no value (a `recur`) takes no part.
    pub(super) fn join(&mut self, e: &Expr, branches: &[(&Expr, Mode)]) -> Mode {
        if !self.is_object(e) {
            return Mode::Scalar;
        }
        let vals: Vec<(&Expr, Mode)> = branches
            .iter()
            .filter(|(_, m)| *m != Mode::Scalar)
            .map(|(x, m)| (*x, *m))
            .collect();
        let Some(&(_, first)) = vals.first() else {
            return Mode::Scalar;
        };
        let same = vals.iter().all(|(_, m)| *m == first);
        match first {
            Mode::Borrowed(_) | Mode::Derived(_) if same => return first,
            _ => {}
        }
        if vals.iter().all(|(_, m)| matches!(m, Mode::Owned { .. })) {
            let immortal = vals.iter().all(|(_, m)| *m == IMMORTAL);
            return Mode::Owned { immortal };
        }
        for (x, m) in vals {
            if !matches!(m, Mode::Owned { .. }) {
                self.push_op(x.id, OpKind::Retain, Site::Value(x.id), Why::Join);
                let val = self.val(x, m);
                self.note(val, At::Join);
            }
        }
        OWNED
    }

    fn let_form(&mut self, bs: &[(Pattern, Expr)], body: &Expr, tail: bool) -> Mode {
        self.push_scope(ScopeKind::Bind);
        for (pat, init) in bs {
            let m = self.step(init, false);
            match &pat.kind {
                PatKind::Bind(x) => self.bind_let(*x, init, m),
                _ => self.bind_scrutinee(pat, init, m),
            }
        }
        let m = self.expr(body, tail);
        let scope = self.pop_scope();
        self.exit_scope(scope, body.id, m)
    }

    fn bind_let(&mut self, x: crate::types::ast::BindingId, init: &Expr, m: Mode) {
        let val = self.val(init, m);
        self.note(val, At::LetInit { binding: x });
        if !self.binding_is_object(x) {
            self.bind(x, Mode::Scalar, BindKind::Scalar);
            return;
        }
        match m {
            Mode::Owned { .. } => {
                let s = Site::Bind(x);
                self.own_site(s, false);
                if allocates(self, init) {
                    self.candidates.insert(s, init.id);
                }
                self.bind(x, Walker::owning(x), BindKind::Owns);
            }
            _ => self.bind(x, m, kind_of(m)),
        }
    }

    /// A `let` pattern or a `match` scrutinee `s` of mode `m`: an
    /// `Owned` one is an implicit owning binding of the scope.
    fn bind_scrutinee(&mut self, pat: &Pattern, s: &Expr, m: Mode) {
        let (whole, inner) = self.scrutinee(s, m);
        self.bind_pattern(pat, whole, inner, true);
    }

    pub(super) fn scrutinee(&mut self, s: &Expr, m: Mode) -> (Mode, Mode) {
        let t = Site::Value(s.id);
        let val = self.val(s, m);
        if matches!(m, Mode::Owned { .. }) {
            self.own_site(t, false);
            if allocates(self, s) {
                self.candidates.insert(t, s.id);
            }
        }
        self.note(val, At::Other);
        pattern_modes(m, t)
    }

    fn loop_form(
        &mut self,
        vs: &[(crate::types::ast::BindingId, Expr)],
        body: &Expr,
        tail: bool,
    ) -> Mode {
        self.push_scope(ScopeKind::Bind);
        let scope = self.frame().scopes.len() - 1;
        let mut vars = Vec::new();
        for (v, init) in vs {
            let m = self.step(init, false);
            let val = self.val(init, m);
            self.note(val, At::LoopInit { var: *v });
            if matches!(m, Mode::Borrowed(_) | Mode::Derived(_)) {
                self.push_op(init.id, OpKind::Retain, Site::Value(init.id), Why::LoopInit);
            }
            if self.binding_is_object(*v) {
                self.own_site(Site::Bind(*v), false);
                self.bind(*v, Walker::owning(*v), BindKind::Owns);
            } else {
                self.bind(*v, Mode::Scalar, BindKind::Scalar);
            }
            vars.push(*v);
        }
        self.frame_mut().loops.push(super::LoopCtx { scope, vars });
        let m = self.step(body, tail);
        self.frame_mut().loops.pop();
        let sc = self.pop_scope();
        self.exit_scope(sc, body.id, m)
    }
}
