//! Solving closure colours (spec/types.md §5.4), after the unit's type
//! constraints:
//!
//! 1. each `ς ⊒ Caps{T}` is rewritten: `Send T` false gives `local ⊑ ς`
//!    remembering the capture; a captured closure's colour `κ` gives
//!    `κ ⊑ ς`; a type variable keeps the symbolic `ς ⊒ send-of(a)`;
//! 2. least fixpoint: every colour variable starts `send` and is raised
//!    to the join of what flows into it, in the lattice `send ⊑ k ⊑
//!    local` where `k` is a rigid colour of an `impl` head (§1.3; two
//!    distinct rigid colours join to `local`);
//! 3. every constraint whose right side is a constant is checked: `κ ⊑
//!    send` needs `κ` send (the error of §5.3), `κ ⊑ k` needs `κ` send
//!    or `k`; the witness is found by walking the `⊑` chain back to the
//!    forcing capture;
//! 4. a variable bounded above by a constant (it reaches `send` or a
//!    rigid colour along `⊑`) and not forced `local` turns its symbolic
//!    constraints into `Send a` bounds; the others stay for
//!    generalisation.

use std::collections::{HashMap, HashSet};

use crate::syntax::Pos;
use crate::types::decls::Globals;
use crate::types::display::Printer;
use crate::types::error::{ErrorKind, TResult, TypeError};
use crate::types::ty::{Colour, CvId, Ty};

use super::cx::{ColourCon, Cx, DKind, Witness};
use super::send::{send_error, send_eval};

/// The solution of a unit's colours.
#[derive(Clone, Debug, Default)]
pub struct Colours {
    /// Variables raised above `send` by the least fixpoint: to `local`
    /// or to a rigid colour.
    pub raised: HashMap<CvId, Colour>,
    /// Variables that must be `send`.
    pub must_send: HashSet<CvId>,
    /// `ς ⊒ send-of(T)` left symbolic: `ς` is neither.
    pub symbolic: Vec<(Colour, Ty)>,
    /// Every flow after rewriting, for generalisation.
    pub flows: Vec<(Colour, Colour)>,
}

impl Colours {
    /// The value of a colour under the solution: `Local`, a rigid
    /// colour, `Send`, or the variable itself when it is still free.
    pub fn value(&self, k: Colour) -> Colour {
        match k {
            Colour::Var(v) => match self.raised.get(&v) {
                Some(r) => *r,
                None if self.must_send.contains(&v) => Colour::Send,
                None => k,
            },
            other => other,
        }
    }
}

impl Cx<'_> {
    /// Solves the unit's colour constraints; adds `Send a` constraints
    /// for the symbolic captures of closures that must be `send` (or
    /// stay below a rigid colour).
    pub fn solve_colours(&mut self) -> TResult<Colours> {
        let symbolic = self.rewrite_caps();
        let cons = self.u.colours.clone();
        let raised = raise(&cons);
        self.check_bounds(&cons, &raised)?;
        let bounded = bounded_above(&cons);
        let mut out = Colours {
            raised: raised.iter().map(|(v, (k, _))| (*v, *k)).collect(),
            must_send: must_send(&cons),
            symbolic: Vec::new(),
            flows: Vec::new(),
        };
        for (k, v, path, pos) in symbolic {
            match (out.value(k), k) {
                (Colour::Local, _) => {}
                (Colour::Send, _) => self.defer(DKind::Send(v, path), &pos, None),
                (_, Colour::Var(c)) if bounded.contains(&c) => {
                    self.defer(DKind::Send(v, path), &pos, None)
                }
                _ => out.symbolic.push((k, v)),
            }
        }
        out.flows = cons.iter().map(|c| (c.from, c.to)).collect();
        Ok(out)
    }

    /// Step 3: every constraint into a constant holds.
    fn check_bounds(&self, cons: &[ColourCon], raised: &Raised) -> TResult<()> {
        for c in cons {
            let from = value_of(c.from, raised);
            if fits(from, c.to) {
                continue;
            }
            let w = witness(cons, raised, c);
            let names = &self.u.rigid_names;
            return Err(match (from, c.to) {
                (Colour::Local, Colour::Send) => send_error(self.g, &w, &c.pos, names),
                _ => colour_error(self.g, &w, from, c.to, &c.pos, names),
            });
        }
        Ok(())
    }
}

/// Each raised variable's value and the constraint that raised it last.
type Raised = HashMap<CvId, (Colour, usize)>;

/// The join in `send ⊑ k ⊑ local`, `k` any rigid colour.
fn join(a: Colour, b: Colour) -> Colour {
    match (a, b) {
        (Colour::Send, x) | (x, Colour::Send) => x,
        (x, y) if x == y => x,
        _ => Colour::Local,
    }
}

/// Whether a colour of value `from` may flow into `to`.
fn fits(from: Colour, to: Colour) -> bool {
    match to {
        Colour::Send | Colour::Rigid(_) => from == Colour::Send || from == to,
        _ => true,
    }
}

/// The value of `k` under the fixpoint: a constant, a raised
/// variable's value, `send` for any other variable.
fn value_of(k: Colour, raised: &Raised) -> Colour {
    match k {
        Colour::Var(v) => raised.get(&v).map_or(Colour::Send, |(k, _)| *k),
        Colour::Gen(_) => Colour::Send,
        other => other,
    }
}

/// Step 2, the least fixpoint.
fn raise(cons: &[ColourCon]) -> Raised {
    let mut raised = Raised::new();
    loop {
        let mut changed = false;
        for (i, c) in cons.iter().enumerate() {
            let Colour::Var(t) = c.to else { continue };
            let old = value_of(c.to, &raised);
            let new = join(old, value_of(c.from, &raised));
            if new != old {
                raised.insert(t, (new, i));
                changed = true;
            }
        }
        if !changed {
            return raised;
        }
    }
}

/// The error of a flow into a rigid colour, or of a rigid colour into
/// `send` (§1.3, §5.4).
fn colour_error(
    g: &Globals,
    w: &Witness,
    from: Colour,
    to: Colour,
    pos: &Pos,
    names: &[String],
) -> TypeError {
    let p = Printer::with_names(g, &[], names);
    let path = if w.path.is_empty() {
        "the value".to_string()
    } else {
        w.path.join(", ")
    };
    let msg = match (from, to) {
        (Colour::Local, _) => format!(
            "local closure where colour {} is required: {path} has type {}",
            p.colour(to),
            p.ty(&w.offending)
        ),
        (_, Colour::Send) => format!(
            "closure of colour {} cannot be shared between threads: {path}",
            p.colour(from)
        ),
        _ => format!(
            "closure of colour {} where colour {} is required: {path}",
            p.colour(from),
            p.colour(to)
        ),
    };
    TypeError::new(ErrorKind::RigidColour, pos, msg)
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

/// Variables from which a constant `send` or rigid colour is reachable
/// along `⊑`: bounded above, so not free to become `local`.
fn bounded_above(cons: &[ColourCon]) -> HashSet<CvId> {
    let mut set = HashSet::new();
    loop {
        let mut changed = false;
        for c in cons {
            let bounded = match c.to {
                Colour::Send | Colour::Rigid(_) => true,
                Colour::Var(v) => set.contains(&v),
                _ => false,
            };
            if let (true, Colour::Var(f)) = (bounded, c.from) {
                changed |= set.insert(f);
            }
        }
        if !changed {
            return set;
        }
    }
}

/// Walks the `⊑` chain back from `c` to the constraint that raised its
/// source, collecting path steps.
fn witness(cons: &[ColourCon], raised: &Raised, c: &ColourCon) -> Witness {
    let mut path = Vec::new();
    let mut cur = c;
    for _ in 0..=cons.len() {
        if let Some(l) = &cur.label {
            path.push(l.clone());
        }
        match cur.from {
            Colour::Var(v) => match raised.get(&v) {
                Some((_, i)) => cur = &cons[*i],
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
            offending: Ty::Fn(cur.from, Vec::new(), Box::new(Ty::unit())),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const K: Colour = Colour::Rigid(0);
    const J: Colour = Colour::Rigid(1);

    #[test]
    fn rigid_colours_lie_between_send_and_local() {
        assert_eq!(join(Colour::Send, K), K);
        assert_eq!(join(K, K), K);
        assert_eq!(join(K, J), Colour::Local);
        assert_eq!(join(K, Colour::Local), Colour::Local);
        assert!(fits(Colour::Send, K) && fits(K, K) && fits(K, Colour::Local));
        assert!(!fits(Colour::Local, K) && !fits(J, K) && !fits(K, Colour::Send));
    }

    #[test]
    fn the_fixpoint_raises_a_variable_to_the_join_of_its_sources() {
        let pos = crate::syntax::Pos {
            file: std::sync::Arc::from("t"),
            line: 1,
            col: 1,
            start: 0,
            end: 0,
        };
        let v = |i| Colour::Var(CvId(i));
        let con = |from, to| ColourCon {
            from,
            to,
            label: None,
            origin: None,
            pos: pos.clone(),
        };
        let cons = vec![
            con(Colour::Send, v(0)),
            con(K, v(1)),
            con(v(1), v(2)),
            con(J, v(2)),
        ];
        let r = raise(&cons);
        assert_eq!(value_of(v(0), &r), Colour::Send);
        assert_eq!(value_of(v(1), &r), K);
        assert_eq!(value_of(v(2), &r), Colour::Local);
        assert!(bounded_above(&[con(v(3), K)]).contains(&CvId(3)));
    }
}
