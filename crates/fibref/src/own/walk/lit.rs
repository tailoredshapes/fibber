//! Bodies: entering a `defun`, method or `def` body, `fn` and `async`
//! literals (§6.5, §6.9), and the exit of a body (the E1 row of §6.3).

use std::collections::HashSet;

use crate::types::ast::{BindingId, Expr, ExprId, FnLit};

use super::super::program::{
    Alloc, BindKind, CaptureOwn, ClosureOwn, Mode, OpKind, ParamKind, ParamOwn, Pass, Site, Why,
};
use super::{At, Frame, FrameKind, ScopeKind, Val, Walker};

/// A parameter as the driver gives it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in super::super) enum ParamIn {
    /// A scalar.
    Scalar,
    /// An `&` parameter.
    Amp,
    /// An object parameter, owned or borrowed.
    Obj { owned: bool },
}

/// One body to walk.
pub(in super::super) struct BodySpec<'p> {
    pub kind: FrameKind,
    pub params: Vec<(BindingId, ParamIn)>,
    /// Per parameter, whether `:borrow` is declared.
    pub declared_borrow: Vec<bool>,
    pub body: &'p Expr,
}

impl Walker<'_> {
    fn open_frame(&mut self, kind: FrameKind) {
        self.frames.push(Frame {
            kind,
            env: Default::default(),
            scopes: Vec::new(),
            loops: Vec::new(),
            owned_params: Vec::new(),
            amp_params: Vec::new(),
            amp_carriers: Default::default(),
            self_name: None,
        });
    }

    fn param(&mut self, b: BindingId, p: ParamIn) {
        match p {
            ParamIn::Scalar => self.bind(b, Mode::Scalar, BindKind::Scalar),
            ParamIn::Amp => {
                self.frame_mut().amp_params.push(b);
                self.out.bindings.insert(
                    b,
                    super::super::program::BindingOwn {
                        kind: BindKind::AmpParam,
                        scope_local: false,
                    },
                );
            }
            ParamIn::Obj { owned } => {
                let owned = owned || self.cx.facts.owned.contains_key(&b);
                self.bind(b, Walker::owning(b), super::kind_of_param(owned));
                if owned {
                    let d = self.depth();
                    self.site_frame.insert(Site::Bind(b), d);
                    self.frame_mut().owned_params.push(Site::Bind(b));
                }
            }
        }
    }

    /// Walks a `defun`, method or `def` body.
    pub fn walk_body(&mut self, spec: &BodySpec<'_>) {
        self.open_frame(spec.kind);
        for (b, p) in &spec.params {
            self.param(*b, *p);
        }
        let amps = self.frame().amp_params.clone();
        for v in amps {
            let c = super::super::captured::carriers(spec.body, v);
            self.frame_mut().amp_carriers.insert(v, c);
        }
        self.run_body(spec.body, true);
        self.frames.pop();
    }

    /// Walks a body in the current (fresh) frame and applies its exit.
    fn run_body(&mut self, body: &Expr, tail: bool) {
        self.push_scope(ScopeKind::Step);
        let m = self.expr(body, tail);
        self.finish_body(body, m);
    }

    /// The exit of a body with value `m` (§6.3, the E1 row): consume
    /// the value (an owned parameter that is the value is moved out),
    /// then release the final step's temporaries, then the owned
    /// parameters not moved out.
    fn finish_body(&mut self, body: &Expr, m: Mode) {
        let root = self.pop_scope();
        let mut moved = HashSet::new();
        let val = self.val(body, m);
        self.note(val, At::Return);
        match m {
            Mode::Borrowed(s)
                if self.frame().owned_params.contains(&s)
                    || root.sites.iter().any(|x| x.0 == s) =>
            {
                moved.insert(s);
            }
            Mode::Borrowed(_) | Mode::Derived(_) => {
                self.push_op(body.id, OpKind::Retain, Site::Value(body.id), Why::Return);
            }
            _ => {}
        }
        for (s, _) in root.sites.iter().rev() {
            if !moved.contains(s) {
                let op = self.end_op(*s, Why::ScopeExit);
                self.add_after(body.id, op);
            }
        }
        for op in self.release_params(&moved, false, Why::ParamExit) {
            self.add_after(body.id, op);
        }
    }

    /// The value `e` with mode `m` is, for the events: a literal, or
    /// `Borrowed`/`Derived` of its site; `None` for `Owned` and scalars.
    pub(super) fn val(&self, e: &Expr, m: Mode) -> Option<Val> {
        if matches!(e.kind, crate::types::ast::ExprKind::Fn(_)) {
            return Some(Val::Lit(e.id));
        }
        match m {
            Mode::Borrowed(s) => Some(Val::B(s)),
            Mode::Derived(s) => Some(Val::D(s)),
            _ => None,
        }
    }

    /// The captures of literal `lit`: how each is taken, with their
    /// modes inside the body.
    fn captures(&mut self, lit: ExprId, caps: &[BindingId], heap: bool) -> Vec<(CaptureOwn, Mode)> {
        let mut out = Vec::new();
        for &c in caps {
            if super::super::syntactic::is_amp(&self.cx.p.globals, c) {
                out.push((
                    CaptureOwn {
                        binding: c,
                        pass: Pass::OwnCell,
                    },
                    Mode::Scalar,
                ));
                continue;
            }
            let outer = self.read(c);
            if outer == Mode::Scalar {
                out.push((
                    CaptureOwn {
                        binding: c,
                        pass: Pass::Scalar,
                    },
                    Mode::Scalar,
                ));
                continue;
            }
            let val = match outer {
                Mode::Borrowed(s) => Some(Val::B(s)),
                Mode::Derived(s) => Some(Val::D(s)),
                _ => None,
            };
            self.note(val, At::Capture { lit, heap });
            let (pass, inner) = if heap {
                (Pass::Retain, Mode::Borrowed(Site::Capture(lit, c)))
            } else {
                (Pass::Alias, outer)
            };
            out.push((CaptureOwn { binding: c, pass }, inner));
        }
        out
    }

    /// Opens the frame of literal `lit` with its captures.
    fn enter_literal(
        &mut self,
        lit: ExprId,
        kind: FrameKind,
        caps: Vec<(CaptureOwn, Mode)>,
        heap: bool,
    ) {
        self.open_frame(kind);
        let d = self.depth();
        for (c, inner) in caps {
            if c.pass == Pass::OwnCell {
                continue;
            }
            self.frame_mut().env.insert(c.binding, inner);
            if heap {
                self.site_frame.insert(Site::Capture(lit, c.binding), d);
            }
        }
    }

    /// A `fn` literal (§6.5): Owned; its body walked as a frame of its own.
    pub(super) fn fn_lit(&mut self, e: &Expr, lit: &FnLit) -> Mode {
        let heap = self.cx.facts.heap.contains_key(&e.id);
        let caps = self.captures(e.id, &lit.captures, heap);
        let mut params = Vec::new();
        for (b, _) in &lit.params {
            let obj = self.binding_is_object(*b);
            params.push(ParamOwn {
                binding: *b,
                kind: if obj {
                    ParamKind::Owned(vec![super::super::program::OwnedWhy::Declared])
                } else {
                    ParamKind::Scalar
                },
                escapes: self.cx.facts.escapes.contains_key(b),
                declared_borrow: false,
            });
        }
        self.record_closure(e.id, false, &caps, params);
        self.enter_literal(e.id, FrameKind::Fn, caps, heap);
        for (b, _) in &lit.params {
            let p = if self.binding_is_object(*b) {
                ParamIn::Obj { owned: true }
            } else {
                ParamIn::Scalar
            };
            self.param(*b, p);
        }
        let d = self.depth();
        self.site_frame.insert(Site::Env(e.id), d);
        self.frame_mut().owned_params.push(Site::Env(e.id));
        if let Some(g) = lit.name {
            self.frame_mut().self_name = Some(g);
            self.bind(
                g,
                Mode::Borrowed(Site::Env(e.id)),
                BindKind::AliasOf(Site::Env(e.id)),
            );
        }
        self.run_body(&lit.body, true);
        self.frames.pop();
        Mode::Owned { immortal: false }
    }

    /// An `async` literal (§6.9): an escaping heap closure with no
    /// parameters whose body has no tail calls.
    pub(super) fn async_lit(&mut self, e: &Expr, body: &Expr, caps: &[BindingId]) -> Mode {
        let caps = self.captures(e.id, caps, true);
        self.record_closure(e.id, true, &caps, Vec::new());
        self.enter_literal(e.id, FrameKind::Async, caps, true);
        self.run_body(body, true);
        self.frames.pop();
        Mode::Owned { immortal: false }
    }

    fn record_closure(
        &mut self,
        lit: ExprId,
        is_async: bool,
        caps: &[(CaptureOwn, Mode)],
        params: Vec<ParamOwn>,
    ) {
        let f = self.cx.facts;
        let (escaping, heap) = if is_async {
            (Some("async".to_string()), Some("async".to_string()))
        } else {
            (f.escaping.get(&lit).cloned(), f.heap.get(&lit).cloned())
        };
        let alloc = if heap.is_some() {
            Alloc::Heap
        } else {
            Alloc::Stack
        };
        self.out.allocs.insert(lit, alloc);
        self.out.closures.insert(
            lit,
            ClosureOwn {
                is_async,
                escaping,
                heap,
                captures: caps.iter().map(|(c, _)| *c).collect(),
                params,
            },
        );
    }
}
