//! Exhaustiveness and redundancy of `match` (spec/types.md §2.6), decided
//! after the unit's types are solved, on the resolved scrutinee type, by
//! the usefulness algorithm (Maranget). A scrutinee whose type is still
//! a variable admits only `_`/variable clauses as exhaustive.
//!
//! A missing case is printed as a pattern in the spelling of syntax
//! §3.6: `nil`, `(some _)`, `(Leaf)`, `(Circle _)`, `true`, `_`.

use crate::types::ast::{Expr, ExprKind, Lit, PatKind, Pattern};
use crate::types::decls::{Globals, Shape};
use crate::types::error::{ErrorKind, TResult, TypeError};
use crate::types::store::Store;
use crate::types::ty::{Con, Scalar, Ty, TypeId};

use super::cx::Cx;

/// A constructor of the matrix.
#[derive(Clone, Debug, PartialEq)]
enum Ctor {
    /// Variant `i` of a nominal type (a struct is its one variant).
    Variant(usize),
    /// `true` or `false`.
    Bool(bool),
    /// Any other literal: the type has infinitely many values.
    Lit(String),
}

/// A pattern of the matrix.
#[derive(Clone, Debug, PartialEq)]
enum P {
    Wild,
    Ctor(Ctor, Vec<P>),
}

fn simplify(p: &Pattern) -> P {
    match &p.kind {
        PatKind::Wild | PatKind::Bind(_) | PatKind::Lit(Lit::Unit) => P::Wild,
        PatKind::As(inner, _) => simplify(inner),
        PatKind::Lit(Lit::Bool(b)) => P::Ctor(Ctor::Bool(*b), Vec::new()),
        PatKind::Lit(l) => P::Ctor(Ctor::Lit(format!("{l:?}")), Vec::new()),
        PatKind::Ctor(_, v, subs) => P::Ctor(
            Ctor::Variant(v.unwrap_or(0)),
            subs.iter().map(simplify).collect(),
        ),
    }
}

/// The complete constructor set of a type, with each constructor's
/// field types; `None` when it is infinite or unknown.
fn signature(g: &Globals, st: &mut Store, t: &Ty) -> Option<Vec<(Ctor, Vec<Ty>)>> {
    match st.zonk(t) {
        Ty::Con(Con::Scalar(Scalar::Bool), _) => Some(vec![
            (Ctor::Bool(true), Vec::new()),
            (Ctor::Bool(false), Vec::new()),
        ]),
        Ty::Con(Con::Nominal(id), args) => Some(match &g.ty(id).shape {
            Shape::Struct(fs) => vec![(
                Ctor::Variant(0),
                fs.iter().map(|f| f.ty.subst_gen(&args, &[])).collect(),
            )],
            Shape::Enum(vs) => vs
                .iter()
                .enumerate()
                .map(|(i, v)| {
                    (
                        Ctor::Variant(i),
                        v.fields
                            .iter()
                            .map(|f| f.ty.subst_gen(&args, &[]))
                            .collect(),
                    )
                })
                .collect(),
        }),
        _ => None,
    }
}

struct Matrix<'a> {
    g: &'a Globals,
    st: &'a mut Store,
}

impl Matrix<'_> {
    /// A vector of patterns `q` not covered by `rows`, as a witness.
    fn useful(&mut self, rows: &[Vec<P>], q: &[P], tys: &[Ty]) -> Option<Vec<P>> {
        let Some(head) = q.first() else {
            return rows.is_empty().then(Vec::new);
        };
        match head {
            P::Ctor(c, args) => {
                let fields = self.fields_of(&tys[0], c, args.len());
                let rows = specialise(rows, c, args.len());
                let mut q2 = args.clone();
                q2.extend_from_slice(&q[1..]);
                let w = self.useful(&rows, &q2, &[fields, tys[1..].to_vec()].concat())?;
                Some(rebuild(c.clone(), args.len(), w))
            }
            P::Wild => self.useful_wild(rows, q, tys),
        }
    }

    fn useful_wild(&mut self, rows: &[Vec<P>], q: &[P], tys: &[Ty]) -> Option<Vec<P>> {
        let used: Vec<Ctor> = rows
            .iter()
            .filter_map(|r| match &r[0] {
                P::Ctor(c, _) => Some(c.clone()),
                P::Wild => None,
            })
            .collect();
        let sig = signature(self.g, self.st, &tys[0]);
        if let Some(sig) = &sig {
            if sig.iter().all(|(c, _)| used.contains(c)) {
                for (c, fields) in sig {
                    let rows2 = specialise(rows, c, fields.len());
                    let mut q2 = vec![P::Wild; fields.len()];
                    q2.extend_from_slice(&q[1..]);
                    let tys2 = [fields.clone(), tys[1..].to_vec()].concat();
                    if let Some(w) = self.useful(&rows2, &q2, &tys2) {
                        return Some(rebuild(c.clone(), fields.len(), w));
                    }
                }
                return None;
            }
        }
        let default: Vec<Vec<P>> = rows
            .iter()
            .filter(|r| r[0] == P::Wild)
            .map(|r| r[1..].to_vec())
            .collect();
        let mut w = self.useful(&default, &q[1..], &tys[1..])?;
        let missing = sig.and_then(|s| s.into_iter().find(|(c, _)| !used.contains(c)));
        let head = match missing {
            Some((c, fields)) => P::Ctor(c, vec![P::Wild; fields.len()]),
            None => P::Wild,
        };
        w.insert(0, head);
        Some(w)
    }

    fn fields_of(&mut self, t: &Ty, c: &Ctor, n: usize) -> Vec<Ty> {
        let sig = signature(self.g, self.st, t).unwrap_or_default();
        match sig.into_iter().find(|(d, _)| d == c) {
            Some((_, fields)) => fields,
            None => vec![Ty::unit(); n],
        }
    }
}

fn specialise(rows: &[Vec<P>], c: &Ctor, arity: usize) -> Vec<Vec<P>> {
    rows.iter()
        .filter_map(|r| match &r[0] {
            P::Ctor(d, args) if d == c => Some([args.clone(), r[1..].to_vec()].concat()),
            P::Ctor(..) => None,
            P::Wild => Some([vec![P::Wild; arity], r[1..].to_vec()].concat()),
        })
        .collect()
}

fn rebuild(c: Ctor, arity: usize, mut w: Vec<P>) -> Vec<P> {
    let rest = w.split_off(arity.min(w.len()));
    let mut out = vec![P::Ctor(c, w)];
    out.extend(rest);
    out
}

fn show(g: &Globals, st: &mut Store, p: &P, t: &Ty) -> String {
    let P::Ctor(c, args) = p else {
        return "_".into();
    };
    let (name, fields): (String, Vec<Ty>) = match (c, st.zonk(t)) {
        (Ctor::Bool(b), _) => return b.to_string(),
        (Ctor::Lit(l), _) => return l.clone(),
        (Ctor::Variant(i), Ty::Con(Con::Nominal(id), targs)) => variant(g, id, *i, &targs),
        _ => return "_".into(),
    };
    if name == "nil" && args.is_empty() {
        return name;
    }
    let mut out = format!("({name}");
    for (a, ft) in args.iter().zip(fields.iter()) {
        out.push(' ');
        out.push_str(&show(g, st, a, ft));
    }
    out.push(')');
    out
}

fn variant(g: &Globals, id: TypeId, i: usize, args: &[Ty]) -> (String, Vec<Ty>) {
    match &g.ty(id).shape {
        Shape::Struct(fs) => (
            g.ty(id).name.clone(),
            fs.iter().map(|f| f.ty.subst_gen(args, &[])).collect(),
        ),
        Shape::Enum(vs) => (
            vs[i].name.clone(),
            vs[i]
                .fields
                .iter()
                .map(|f| f.ty.subst_gen(args, &[]))
                .collect(),
        ),
    }
}

impl Cx<'_> {
    /// Checks every `match` in `e`.
    pub fn check_matches(&mut self, e: &Expr) -> TResult<()> {
        if let ExprKind::Match(s, clauses) = &e.kind {
            let st = self.t.expr_types.get(&s.id).cloned().unwrap_or(Ty::unit());
            self.check_match(e, &st, clauses)?;
        }
        let mut result = Ok(());
        e.children(&mut |c| {
            if result.is_ok() {
                result = self.check_matches(c);
            }
        });
        result
    }

    fn check_match(&mut self, e: &Expr, st: &Ty, clauses: &[(Pattern, Expr)]) -> TResult<()> {
        let g = self.g;
        let mut m = Matrix { g, st: self.st };
        let mut rows: Vec<Vec<P>> = Vec::new();
        for (p, _) in clauses {
            let row = vec![simplify(p)];
            if m.useful(&rows, &row, std::slice::from_ref(st)).is_none() {
                return Err(TypeError::new(
                    ErrorKind::Redundant,
                    &p.pos,
                    "redundant match clause",
                ));
            }
            rows.push(row);
        }
        if let Some(w) = m.useful(&rows, &[P::Wild], std::slice::from_ref(st)) {
            let text = show(g, m.st, &w[0], st);
            let msg = format!("non-exhaustive match: missing {text}");
            return Err(TypeError::new(ErrorKind::NonExhaustive, &e.pos, msg));
        }
        Ok(())
    }
}
