//! The control forms: `let`, `if`, `do`, `match`, `loop` and `recur`
//! (types §8.3 for `match`, §8.9 for `recur`).

use fibref::own::program::Pass;
use fibref::types::ast::{BindingId, Clause, Expr, Pattern};

use super::{Cx, Flow, Local, LoopCx, R};
use crate::compile::Unsupported;
use crate::ir::{LirTy, V};

impl<'a> Cx<'_, 'a> {
    pub fn let_form(&mut self, bs: &'a [(Pattern, Expr)], body: &'a Expr) -> R<Flow> {
        for (pat, init) in bs {
            let v = self.value(init)?;
            let t = self.ty(init)?;
            self.bind_irrefutable(pat, &v, &t)?;
        }
        self.expr(body)
    }

    pub fn do_form(&mut self, es: &'a [Expr]) -> R<Flow> {
        let Some((last, steps)) = es.split_last() else {
            return Ok(Flow::Val(V::Unit));
        };
        for s in steps {
            if let Flow::Jump = self.expr(s)? {
                return Ok(Flow::Jump);
            }
        }
        self.expr(last)
    }

    pub fn if_form(&mut self, e: &Expr, c: &'a Expr, t: &'a Expr, f: &'a Expr) -> R<Flow> {
        let cond = self.value(c)?;
        let (lt, lf, lj) = (
            self.b.label("then"),
            self.b.label("else"),
            self.b.label("join"),
        );
        self.b.term(&format!("(br {} {lt} {lf})", cond.text()));
        let rt = self.ty(e)?;
        let ty = self.p.lir(&rt)?;
        let mut incoming = Vec::new();
        for (label, branch) in [(lt, t), (lf, f)] {
            self.b.open(&label);
            if let Flow::Val(v) = self.expr(branch)? {
                incoming.push((self.b.current(), v));
                self.b.term(&format!("(br {lj})"));
            }
        }
        self.join_branches(&lj, ty, incoming)
    }

    /// The join block of branches that fell through, with a `phi` of
    /// their values; `Jump` when none did.
    pub fn join_branches(
        &mut self,
        label: &str,
        ty: Option<LirTy>,
        incoming: Vec<(String, V)>,
    ) -> R<Flow> {
        if incoming.is_empty() {
            return Ok(Flow::Jump);
        }
        self.b.open(label);
        let Some(t) = ty else {
            return Ok(Flow::Val(V::Unit));
        };
        let pairs: Vec<(String, String)> = incoming
            .into_iter()
            .map(|(l, v)| (l, v.text().to_string()))
            .collect();
        Ok(Flow::Val(self.b.phi(t, &pairs)))
    }

    pub fn match_form(&mut self, e: &Expr, s: &'a Expr, cls: &'a [Clause]) -> R<Flow> {
        let v = self.value(s)?;
        let st = self.ty(s)?;
        let rt = self.ty(e)?;
        let ty = self.p.lir(&rt)?;
        let lj = self.b.label("mjoin");
        let mut incoming = Vec::new();
        let mut next = self.b.label("clause");
        self.b.term(&format!("(br {next})"));
        for c in cls {
            self.b.open(&next);
            next = self.b.label("clause");
            self.match_pattern(&c.pat, &v, &st, &next)?;
            self.build_rests()?;
            if let Some(g) = &c.guard {
                let gv = self.value(g)?;
                let ok = self.b.label("guard");
                let fail = self.b.label("gfail");
                self.b.term(&format!("(br {} {ok} {fail})", gv.text()));
                self.b.open(&fail);
                let ops = self.own.guard_fail.get(&g.id).cloned().unwrap_or_default();
                self.run_ops(&ops)?;
                self.b.term(&format!("(br {next})"));
                self.b.open(&ok);
            }
            if let Flow::Val(bv) = self.expr(&c.body)? {
                incoming.push((self.b.current(), bv));
                self.b.term(&format!("(br {lj})"));
            }
        }
        self.b.open(&next);
        self.trap_c("no match clause matched");
        self.join_branches(&lj, ty, incoming)
    }

    pub fn loop_form(&mut self, _e: &Expr, vs: &'a [(BindingId, Expr)], body: &'a Expr) -> R<Flow> {
        let mut vars = Vec::new();
        for (b, init) in vs {
            let v = self.value(init)?;
            let t = v.ty();
            let slot = self.b.entry_alloca(t.map_or("i64", LirTy::text));
            self.store(&v, &slot);
            self.locals.insert(*b, Local::Slot(slot.clone(), t));
            vars.push((*b, slot, t));
        }
        let head = self.b.label("loop");
        self.b.term(&format!("(br {head})"));
        self.b.open(&head);
        self.loops.push(LoopCx { vars, head });
        let flow = self.expr(body);
        self.loops.pop();
        flow
    }

    /// `recur` (§6.10): the arguments consumed into the loop variables,
    /// the jump's releases, the new values stored, back to the head.
    pub fn recur(&mut self, e: &Expr, args: &'a [Expr]) -> R<Flow> {
        let own = self
            .own
            .recurs
            .get(&e.id)
            .cloned()
            .ok_or_else(|| Unsupported("no plan for a recur".into()))?;
        let mut vals = Vec::with_capacity(args.len());
        for (i, x) in args.iter().enumerate() {
            let v = self.value(x)?;
            if own.args.get(i) == Some(&Pass::Retain) {
                self.retain(&v);
            }
            vals.push(v);
        }
        self.run_ops(&own.jump)?;
        let lp = self
            .loops
            .last()
            .cloned()
            .ok_or_else(|| Unsupported("recur outside a loop".into()))?;
        for ((_, slot, _), v) in lp.vars.iter().zip(vals) {
            self.store(&v, slot);
        }
        self.b.term(&format!("(br {})", lp.head));
        Ok(Flow::Jump)
    }
}
