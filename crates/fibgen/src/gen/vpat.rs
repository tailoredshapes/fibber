//! Vector patterns and guarded clauses (syntax §3.6, types §2.6).
//!
//! Clause lists are exhaustive and non-redundant by construction. The
//! unguarded clauses cover lengths `0 .. k-1` exactly and then `[p.. &
//! r]` of length `k` (or end in a catch-all); a refutable twin (with a
//! literal or a nested vector pattern) may precede the full clause of
//! its length. A guarded clause covers nothing (types §2.6), so it is
//! placed only where the full unguarded clauses above it do not already
//! cover every length it matches, which is exactly the redundancy rule.

use crate::ast::{Clause, Expr, Kind, Pat, Rest};
use crate::ty::Ty;

use super::control::irrefutable;
use super::vpat2::{clause_for, Lens, Planned, Site, Wrapper};
use super::{Ctx, Gen, Var, VarKind};

pub use super::vpat2::{counted_match, guarded_match};

/// Whether the full (irrefutable-element) patterns covering `above`
/// together match every vector of the lengths `l`.
pub fn covered(above: &[Lens], l: Lens) -> bool {
    let has = |n: usize| {
        above.iter().any(|a| match *a {
            Lens::All => true,
            Lens::Exact(k) => k == n,
            Lens::AtLeast(k) => n >= k,
        })
    };
    match l {
        Lens::All => above.contains(&Lens::All),
        Lens::Exact(n) => has(n),
        Lens::AtLeast(m) => {
            let least = above.iter().filter_map(|a| match *a {
                Lens::All => Some(0),
                Lens::AtLeast(k) => Some(k),
                Lens::Exact(_) => None,
            });
            match least.min() {
                Some(k) => (m..k).all(has),
                None => false,
            }
        }
    }
}

/// `[p1 .. pn]` or `[p1 .. pn & r]` with irrefutable elements for
/// `elem`, and its variables.
pub fn full(g: &mut Gen, elem: &Ty, l: Lens) -> (Pat, Vec<Var>) {
    let (n, rest) = match l {
        Lens::Exact(n) => (n, false),
        Lens::AtLeast(k) => (k, true),
        Lens::All => {
            let n = g.fresh("whole");
            let v = Var::new(n.clone(), Ty::vec(elem.clone()), VarKind::Pattern);
            return (Pat::Bind(n), vec![v]);
        }
    };
    let mut pats = Vec::new();
    let mut vars = Vec::new();
    for _ in 0..n {
        let (p, vs) = irrefutable(g, elem, 1);
        pats.push(p);
        vars.extend(vs);
    }
    let rest = rest.then(|| {
        if g.rng.chance(25) {
            return Rest::Wild;
        }
        let r = g.fresh("r");
        vars.push(Var::new(r.clone(), Ty::vec(elem.clone()), VarKind::Pattern));
        Rest::Bind(r)
    });
    (Pat::Vector(pats, rest), vars)
}

/// A refutable twin of `full(l)`: one element replaced by a literal
/// (for `i64`) or by a nested vector pattern inside a `Wrap` pattern.
/// A guarded twin uses literals and shapes no unguarded twin does, so
/// that it is never covered by one above it.
fn twin(g: &mut Gen, elem: &Ty, l: Lens, guarded: bool) -> Option<(Pat, Vec<Var>)> {
    let (Pat::Vector(mut ps, rest), mut vars) = full(g, elem, l) else {
        return None;
    };
    if ps.is_empty() {
        return None;
    }
    let i = g.rng.below(ps.len());
    let names = super::control::pat_names(&ps[i]);
    vars.retain(|v| !names.contains(&v.name));
    ps[i] = match elem {
        Ty::Int if guarded => Pat::Lit(g.rng.range(5, 7)),
        Ty::Int => Pat::Lit(g.rng.range(-1, 3)),
        Ty::Wrap => {
            let s = g.fresh("s");
            vars.push(Var::new(s.clone(), Ty::Str, VarKind::Pattern));
            let inner = if guarded {
                let (p, vs) = full(g, &Ty::Int, Lens::AtLeast(1));
                vars.extend(vs);
                p
            } else {
                Pat::Vector(Vec::new(), None)
            };
            Pat::Ctor("Wrap".into(), vec![Pat::Bind(s), inner])
        }
        _ => return None,
    };
    Some((Pat::Vector(ps, rest), vars))
}

/// The unguarded clauses: `(pattern, vars, full lengths it covers)`.
fn unguarded(g: &mut Gen, elem: &Ty) -> Vec<Planned> {
    let k = g.rng.range(0, 3) as usize;
    let catch_all = g.rng.chance(30);
    let mut out = Vec::new();
    let mut all_exact = true;
    for n in 0..k {
        if catch_all && g.rng.chance(40) {
            all_exact = false;
            continue;
        }
        push_with_twin(g, elem, Lens::Exact(n), &mut out);
    }
    if !catch_all || !all_exact {
        push_with_twin(g, elem, Lens::AtLeast(k), &mut out);
    }
    if catch_all {
        let p = if g.rng.chance(50) {
            full(g, elem, Lens::All)
        } else {
            (Pat::Wild, Vec::new())
        };
        out.push((p.0, p.1, Some(Lens::All)));
    }
    out
}

fn push_with_twin(g: &mut Gen, elem: &Ty, l: Lens, out: &mut Vec<Planned>) {
    if g.rng.chance(25) {
        if let Some((p, vs)) = twin(g, elem, l, false) {
            out.push((p, vs, None));
        }
    }
    let (p, vs) = full(g, elem, l);
    out.push((p, vs, Some(l)));
}

/// A random length set for a guarded clause.
fn some_lens(g: &mut Gen) -> Lens {
    let n = g.rng.range(0, 3) as usize;
    if g.rng.chance(50) {
        Lens::AtLeast(n)
    } else {
        Lens::Exact(n)
    }
}

/// The plan: unguarded clauses with guarded ones inserted where they
/// are not redundant; the flag marks the guarded ones.
fn plan(g: &mut Gen, elem: &Ty) -> Vec<(Planned, bool)> {
    let mut out: Vec<(Planned, bool)> =
        unguarded(g, elem).into_iter().map(|c| (c, false)).collect();
    let guards = if g.rng.chance(70) {
        g.rng.range(1, 2)
    } else {
        0
    };
    for _ in 0..guards {
        let at = g.rng.below(out.len() + 1);
        let above: Vec<Lens> = out[..at]
            .iter()
            .filter(|(_, guarded)| !guarded)
            .filter_map(|((_, _, l), _)| *l)
            .collect();
        let l = some_lens(g);
        if covered(&above, l) {
            continue;
        }
        let tw = if g.rng.chance(30) {
            twin(g, elem, l, true)
        } else {
            None
        };
        let (p, vs) = tw.unwrap_or_else(|| full(g, elem, l));
        out.insert(at, ((p, vs, None), true));
    }
    out
}

/// The clauses of a `match` over a vector of `elem`, each pattern
/// passed through `wrap` (which may put it inside a struct pattern).
pub fn vector_clauses(g: &mut Gen, cx: &Ctx, elem: &Ty, site: &Site, wrap: Wrapper) -> Vec<Clause> {
    let mut out = Vec::new();
    for ((p, mut vars, _), guarded) in plan(g, elem) {
        let (p, extra) = if matches!(p, Pat::Wild) {
            (p, Vec::new())
        } else {
            wrap(g, p)
        };
        vars.extend(extra);
        out.push(clause_for(g, cx, site, p, vars, guarded));
    }
    out
}

/// A `match` on a vector, or on a `Wrap` whose vector field is taken
/// apart by vector patterns inside the struct pattern.
pub fn vector_match(g: &mut Gen, cx: &Ctx, ty: &Ty, d: u32) -> Expr {
    let elems = [Ty::Int, Ty::Int, Ty::Str, Ty::Wrap, Ty::Pt];
    let (st, elem, in_wrap) = if g.rng.chance(25) {
        (Ty::Wrap, Ty::Int, true)
    } else {
        let e = elems[g.rng.below(elems.len())].clone();
        (Ty::vec(e.clone()), e, false)
    };
    let s = g.expr(cx, &st, d - 1);
    let site = Site {
        ty,
        counter: None,
        d,
    };
    let clauses = if in_wrap {
        vector_clauses(g, cx, &elem, &site, &wrap_in_wrap)
    } else {
        vector_clauses(g, cx, &elem, &site, &|_, p| (p, Vec::new()))
    };
    Expr::new(ty.clone(), Kind::GMatch(Box::new(s), clauses))
}

/// A `match` on a vector of `elem` whose value is a vector of `elem`,
/// so a clause's rest may leave as its value (case 129).
pub fn rest_match(g: &mut Gen, cx: &Ctx, elem: &Ty, d: u32) -> Expr {
    let vt = Ty::vec(elem.clone());
    let s = g.expr(cx, &vt, d - 1);
    let site = Site {
        ty: &vt,
        counter: None,
        d,
    };
    let clauses = vector_clauses(g, cx, elem, &site, &|_, p| (p, Vec::new()));
    Expr::new(vt, Kind::GMatch(Box::new(s), clauses))
}

/// `(Wrap s p)` around a vector pattern `p` for the field `v`.
pub fn wrap_in_wrap(g: &mut Gen, p: Pat) -> (Pat, Vec<Var>) {
    let (sp, vs) = irrefutable(g, &Ty::Str, 0);
    (Pat::Ctor("Wrap".into(), vec![sp, p]), vs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coverage_of_lengths_is_by_union() {
        let above = [Lens::Exact(0), Lens::AtLeast(1)];
        assert!(covered(&above, Lens::AtLeast(0)));
        assert!(covered(&above, Lens::Exact(3)));
        let gap = [Lens::Exact(1), Lens::AtLeast(2)];
        assert!(!covered(&gap, Lens::AtLeast(0)));
        assert!(covered(&gap, Lens::AtLeast(1)));
        assert!(!covered(&[Lens::Exact(2)], Lens::AtLeast(2)));
        assert!(covered(&[Lens::All], Lens::AtLeast(0)));
    }
}
