//! Callees and their parameter positions (types §6.3, §6.4): the
//! kinds and summaries of a `defun`, the declared kinds of a protocol
//! method, the escape table of a builtin, constructors, externs and
//! closure values.

use crate::types::ast::{Expr, ExprKind, FunId, GlobalRef};
use crate::types::builtins::{Escape, BUILTINS};

use super::super::program::Callee;
use super::{Class, FrameKind, Walker};

/// Whether a method's `self` escapes by default. types §2.7 and §6.4
/// say every method parameter defaults to escaping; syntax §3.10 says
/// every parameter *other than* `self` does. The two disagree, and the
/// prelude's `(conj (self x) (cons x self))` for `List` stores `self`,
/// which a non-escaping `self` would reject. This follows types.md
/// (the checker's specification) until the owner decides; see the
/// report.
pub(in super::super) const SELF_ESCAPES_BY_DEFAULT: bool = true;

/// One parameter position of a callee.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in super::super) struct Param {
    pub class: Class,
    pub escapes: bool,
    pub amp: bool,
    /// The operand of `weak` (no count, but an escape).
    pub weak: bool,
    /// The operand of `raw-retained`.
    pub raw_retained: bool,
}

pub(super) const fn param(class: Class, escapes: bool) -> Param {
    Param {
        class,
        escapes,
        amp: false,
        weak: false,
        raw_retained: false,
    }
}

/// The builtins whose object operand is stored or handed to a thread:
/// stores, not calls (they are never tail calls, like constructors).
pub(super) fn builtin_stores(b: usize) -> bool {
    BUILTINS[b]
        .escapes
        .iter()
        .any(|e| matches!(e, Escape::Store | Escape::Thread))
}

/// Whether `e` allocates its object in this frame (§6.11): a
/// constructor call of a non-`Option` type, `cell`, `atom`, a `fn`.
pub(super) fn allocates(w: &Walker<'_>, e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Fn(_) => true,
        ExprKind::Call(h, _) => match h.kind {
            ExprKind::Global(GlobalRef::Ctor(t, _)) => t != w.cx.p.globals.option,
            ExprKind::Global(GlobalRef::Builtin(b)) => {
                matches!(BUILTINS[b.0 as usize].name, "cell" | "atom")
            }
            _ => false,
        },
        _ => false,
    }
}

impl Walker<'_> {
    /// What `head` calls.
    pub(super) fn callee_of(&self, head: &Expr) -> Callee {
        match head.kind {
            ExprKind::Global(GlobalRef::Fun(f))
                if self.cx.all_owned && self.cx.scc.contains(&f) =>
            {
                Callee::AllOwned(f)
            }
            ExprKind::Global(GlobalRef::Fun(f)) => Callee::Fun(f),
            ExprKind::Global(GlobalRef::Method(p, i)) => Callee::Method(p, i),
            ExprKind::Global(GlobalRef::Builtin(b)) => Callee::Builtin(b),
            ExprKind::Global(GlobalRef::Ctor(..)) => Callee::Ctor,
            ExprKind::Global(GlobalRef::Extern(_)) => Callee::Extern,
            _ => Callee::Value,
        }
    }

    /// The `defun` of the current SCC that a `defun` body calls, if so.
    pub(super) fn scc_callee(&self, c: Callee) -> Option<FunId> {
        let f = match c {
            Callee::Fun(f) | Callee::AllOwned(f) => f,
            _ => return None,
        };
        (self.frame().kind == FrameKind::Defun && self.cx.scc.contains(&f)).then_some(f)
    }

    /// Every parameter position of `c` for `n` arguments.
    pub(super) fn params_of(&self, c: Callee, n: usize) -> Vec<Param> {
        match c {
            Callee::Fun(f) | Callee::AllOwned(f) => self.fun_params(f, n, c != Callee::Fun(f)),
            Callee::Method(p, i) => {
                let md = &self.cx.p.globals.proto(p).methods[i];
                (0..n)
                    .map(|j| {
                        let mp = md.params.get(j);
                        let owned = mp.is_some_and(|m| m.owned);
                        let borrow = mp.is_some_and(|m| m.borrow);
                        let escapes = !borrow && (j > 0 || SELF_ESCAPES_BY_DEFAULT);
                        param(if owned { Class::Owned } else { Class::Borrow }, escapes)
                    })
                    .collect()
            }
            Callee::Builtin(b) => BUILTINS[b.0 as usize]
                .escapes
                .iter()
                .map(|e| builtin_param(b.0 as usize, *e))
                .collect(),
            Callee::Ctor => vec![param(Class::Store, true); n],
            Callee::Extern => vec![param(Class::Borrow, false); n],
            Callee::Value => vec![param(Class::Owned, true); n],
        }
    }

    fn fun_params(&self, f: FunId, n: usize, all_owned: bool) -> Vec<Param> {
        let amps = self.cx.p.fun_schemes[f.0 as usize]
            .as_ref()
            .map(|s| s.amps.clone())
            .unwrap_or_default();
        let sum = self.summary_of(f).unwrap_or_default();
        (0..n)
            .map(|i| {
                let (owned, escapes) = sum.params.get(i).copied().unwrap_or((false, true));
                let class = if owned || all_owned {
                    Class::Owned
                } else {
                    Class::Borrow
                };
                Param {
                    amp: amps.get(i).copied().unwrap_or(false),
                    ..param(class, escapes)
                }
            })
            .collect()
    }
}

fn builtin_param(b: usize, e: Escape) -> Param {
    match e {
        Escape::Scalar | Escape::Borrow | Escape::Raw => param(Class::Borrow, false),
        Escape::Store => Param {
            raw_retained: BUILTINS[b].name == "raw-retained",
            ..param(Class::Store, true)
        },
        Escape::Thread => param(Class::Thread, true),
        Escape::InOut => Param {
            amp: true,
            ..param(Class::Borrow, false)
        },
        Escape::Weak => Param {
            weak: true,
            ..param(Class::Borrow, true)
        },
    }
}
