//! phi: placement now, predecessors and incoming values once every
//! block is typed (spec/lir.md §5.4).

use std::collections::HashSet;

use super::expr::is_constant;
use super::fcx::Fcx;
use crate::ast::{Binding, Callee, Expr, Kind};
use crate::diag::{err, Pos, Result};
use crate::types::Type;

/// A phi whose incoming values are checked after the whole function.
pub struct PendingPhi<'a> {
    pub block: usize,
    pub ty: Type,
    pub incoming: &'a [Binding],
    pub pos: Pos,
}

impl<'a> Fcx<'a> {
    pub fn phi(&mut self, t: &Type, inc: &'a [Binding], p: Pos) -> Result<Type> {
        if self.emitted {
            return err(
                p,
                format!(
                    "phi after the first instruction of block {}",
                    self.label(self.cur)
                ),
            );
        }
        self.valid(t, p)?;
        self.phis.push(PendingPhi {
            block: self.cur,
            ty: t.clone(),
            incoming: inc,
            pos: p,
        });
        Ok(t.clone())
    }

    pub fn finish_phis(&mut self) -> Result<()> {
        let phis = std::mem::take(&mut self.phis);
        phis.iter().try_for_each(|ph| self.finish_phi(ph))
    }

    fn finish_phi(&mut self, ph: &PendingPhi<'a>) -> Result<()> {
        let here = self.label(ph.block);
        let preds = &self.cfg.preds[ph.block];
        let mut seen = HashSet::new();
        for b in ph.incoming {
            let Some(&m) = self.labels.get(&b.name) else {
                return err(b.pos, format!("undefined block {}", b.name));
            };
            if !preds.contains(&m) {
                return err(
                    b.pos,
                    format!("phi in {here} names {}, which is not a predecessor", b.name),
                );
            }
            if !seen.insert(m) {
                return err(
                    b.pos,
                    format!("phi in {here} has two entries for {}", b.name),
                );
            }
            let t = self.incoming(&b.value, m)?;
            if t != ph.ty {
                return err(
                    b.pos,
                    format!(
                        "phi: incoming value from {} has type {t}, expected {}",
                        b.name, ph.ty
                    ),
                );
            }
        }
        if let Some(&m) = preds.iter().find(|m| !seen.contains(*m)) {
            return err(
                ph.pos,
                format!(
                    "phi in {here} has no entry for predecessor {}",
                    self.label(m)
                ),
            );
        }
        Ok(())
    }

    /// The type of an incoming value, used at the end of block `m`.
    fn incoming(&mut self, v: &'a Expr, m: usize) -> Result<Type> {
        let void = || err(v.pos, "void value used as an operand");
        match &v.kind {
            k if k.is_terminator() => err(v.pos, "terminator used as a value"),
            Kind::Local(n) => self.lookup_at_end(n, m, v.pos),
            Kind::Store { .. } | Kind::AtomicStore(..) | Kind::Fence(..) | Kind::Trap => void(),
            Kind::Call {
                callee: Callee::Direct(f),
                ..
            } if self.env.function(f, v.pos).is_ok_and(|t| t.ret.is_none()) => void(),
            _ if is_constant(v) => {
                let saved = (self.cur, self.emitted);
                self.cur = m;
                let t = self.val(v);
                (self.cur, self.emitted) = saved;
                t
            }
            _ => err(
                v.pos,
                "phi incoming value must be a name, a global or a constant",
            ),
        }
    }
}
