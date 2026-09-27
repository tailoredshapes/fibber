//! Solving closure colours (spec/types.md §5.4), after the unit's type
//! constraints:
//!
//! 1. each `ς ⊒ Caps{T}` is rewritten: `Send T` false gives `local ⊑ ς`
//!    remembering the capture; a captured closure's colour `κ` gives
//!    `κ ⊑ ς`; a type variable keeps the symbolic `ς ⊒ send-of(a)`;
//! 2. least fixpoint: every colour variable starts `send`, and `local`
//!    propagates along `⊑`;
//! 3. every `κ ⊑ send` with `κ` local is the error of §5.3, its witness
//!    found by walking the `⊑` chain back to the forcing capture;
//! 4. a variable that must be `send` (it reaches `send` along `⊑`) turns
//!    its symbolic constraints into `Send a` bounds; the others stay for
//!    generalisation.

use std::collections::{HashMap, HashSet};

use crate::syntax::Pos;
use crate::types::error::TResult;
use crate::types::ty::{Colour, CvId, Ty};

use super::cx::{ColourCon, Cx, DKind, Witness};
use super::send::{send_error, send_eval};

/// The solution of a unit's colours.
#[derive(Clone, Debug, Default)]
pub struct Colours {
    /// Variables forced `local`.
    pub local: HashSet<CvId>,
    /// Variables that must be `send`.
    pub must_send: HashSet<CvId>,
    /// `ς ⊒ send-of(T)` left symbolic: `ς` is neither.
    pub symbolic: Vec<(Colour, Ty)>,
    /// Every flow after rewriting, for generalisation.
    pub flows: Vec<(Colour, Colour)>,
}

impl Colours {
    /// The value of a colour under the solution: `Local`, `Send`, or the
    /// variable itself when it is still free.
    pub fn value(&self, k: Colour) -> Colour {
        match k {
            Colour::Var(v) if self.local.contains(&v) => Colour::Local,
            Colour::Var(v) if self.must_send.contains(&v) => Colour::Send,
            other => other,
        }
    }
}

impl Cx<'_> {
    /// Solves the unit's colour constraints; adds `Send a` constraints
    /// for the symbolic captures of closures that must be `send`.
    pub fn solve_colours(&mut self) -> TResult<Colours> {
        let symbolic = self.rewrite_caps();
        let cons = self.u.colours.clone();
        let pred = propagate_local(&cons);
        for c in &cons {
            if c.to == Colour::Send && is_local(c.from, &pred) {
                let w = witness(&cons, &pred, c);
                return Err(send_error(self.g, &w, &c.pos, &self.u.rigid_names));
            }
        }
        let must_send = must_send(&cons);
        let mut out = Colours {
            local: pred.keys().copied().collect(),
            must_send,
            symbolic: Vec::new(),
            flows: Vec::new(),
        };
        for (k, v, path, pos) in symbolic {
            match out.value(k) {
                Colour::Local => {}
                Colour::Send => self.defer(DKind::Send(v, path), &pos, None),
                _ => out.symbolic.push((k, v)),
            }
        }
        out.flows = cons.iter().map(|c| (c.from, c.to)).collect();
        Ok(out)
    }
}

impl Cx<'_> {
    /// Step 1: each `ς ⊒ Caps{T}` becomes `local ⊑ ς` (with its
    /// witness) when `Send T` fails, `κ ⊑ ς` for each colour variable in
    /// `T`; what depends on a type variable is returned as symbolic.
    fn rewrite_caps(&mut self) -> Vec<(Colour, Ty, Vec<String>, Pos)> {
        let mut symbolic = Vec::new();
        for c in std::mem::take(&mut self.u.caps) {
            let needs = match send_eval(self.g, self.st, &c.ty, std::slice::from_ref(&c.label)) {
                Ok(needs) => needs,
                Err(w) => {
                    let (to, pos) = (c.colour, c.pos);
                    self.u.colours.push(ColourCon {
                        from: Colour::Local,
                        to,
                        label: None,
                        origin: Some(w),
                        pos,
                    });
                    continue;
                }
            };
            for (k, path) in needs.colours {
                let label = Some(path.join(", "));
                let (to, pos) = (c.colour, c.pos.clone());
                self.u.colours.push(ColourCon {
                    from: k,
                    to,
                    label,
                    origin: None,
                    pos,
                });
            }
            for (v, path) in needs.vars {
                symbolic.push((c.colour, v, path, c.pos.clone()));
            }
        }
        symbolic
    }
}

fn is_local(k: Colour, pred: &HashMap<CvId, usize>) -> bool {
    match k {
        Colour::Local => true,
        Colour::Var(v) => pred.contains_key(&v),
        _ => false,
    }
}

/// Least fixpoint: for each variable forced `local`, the constraint that
/// forced it.
fn propagate_local(cons: &[ColourCon]) -> HashMap<CvId, usize> {
    let mut pred = HashMap::new();
    loop {
        let mut changed = false;
        for (i, c) in cons.iter().enumerate() {
            if let Colour::Var(t) = c.to {
                if is_local(c.from, &pred) && !pred.contains_key(&t) {
                    pred.insert(t, i);
                    changed = true;
                }
            }
        }
        if !changed {
            return pred;
        }
    }
}

/// Variables from which `send` is reachable along `⊑`.
fn must_send(cons: &[ColourCon]) -> HashSet<CvId> {
    let mut set = HashSet::new();
    loop {
        let mut changed = false;
        for c in cons {
            let to_send = match c.to {
                Colour::Send => true,
                Colour::Var(v) => set.contains(&v),
                _ => false,
            };
            if let (true, Colour::Var(f)) = (to_send, c.from) {
                changed |= set.insert(f);
            }
        }
        if !changed {
            return set;
        }
    }
}

/// Walks the `⊑` chain back from `c` to the constraint that made its
/// source `local`, collecting path steps.
fn witness(cons: &[ColourCon], pred: &HashMap<CvId, usize>, c: &ColourCon) -> Witness {
    let mut path = Vec::new();
    let mut cur = c;
    for _ in 0..=cons.len() {
        if let Some(l) = &cur.label {
            path.push(l.clone());
        }
        match cur.from {
            Colour::Var(v) => match pred.get(&v) {
                Some(i) => cur = &cons[*i],
                None => break,
            },
            _ => break,
        }
    }
    match &cur.origin {
        Some(w) => {
            path.extend(w.path.iter().cloned());
            Witness {
                path,
                offending: w.offending.clone(),
            }
        }
        None => Witness {
            path,
            offending: Ty::Fn(Colour::Local, Vec::new(), Box::new(Ty::unit())),
        },
    }
}
