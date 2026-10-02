//! The usefulness matrix (Maranget) over the constructors of §2.6:
//! variants, `bool`s, other literals (infinitely many), and the lengths
//! of a `(Vec T)`. In a vector column with bound `L` (one more than the
//! longest fixed-length pattern, or the longest prefix of a rest
//! pattern if that is more), the constructors are `Len(n)` for `n < L`
//! and `AtLeast(L)`, which no pattern of the column can split; the set
//! is complete, so every one is tried.

use crate::types::decls::{Globals, Shape};
use crate::types::store::Store;
use crate::types::ty::{colour_args, Con, Scalar, Ty};

/// A constructor of the matrix.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Ctor {
    /// Variant `i` of a nominal type (a struct is its one variant).
    Variant(usize),
    /// `true` or `false`.
    Bool(bool),
    /// Any other literal: the type has infinitely many values.
    Lit(String),
    /// A vector of exactly this length.
    Len(usize),
    /// A vector of at least this length (arity: this length).
    AtLeast(usize),
}

/// A pattern of the matrix.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum P {
    Wild,
    Ctor(Ctor, Vec<P>),
    /// `[p.. & r]`: its element patterns.
    Slice(Vec<P>),
}

pub(super) struct Matrix<'a> {
    g: &'a Globals,
    pub st: &'a mut Store,
    /// Set when a vector column also holds a pattern of a variant of
    /// `Vec` (§2.6: an error).
    pub mixed: bool,
}

impl<'a> Matrix<'a> {
    pub fn new(g: &'a Globals, st: &'a mut Store) -> Self {
        Matrix {
            g,
            st,
            mixed: false,
        }
    }

    /// A vector of patterns `q` not covered by `rows`, as a witness.
    pub fn useful(&mut self, rows: &[Vec<P>], q: &[P], tys: &[Ty]) -> Option<Vec<P>> {
        let Some(head) = q.first() else {
            return rows.is_empty().then(Vec::new);
        };
        if let Some((rows, q)) = self.tuple_column(rows, q, tys) {
            return self.useful(&rows, &q, tys);
        }
        if let Some(l) = self.vec_bound(rows, head) {
            return self.useful_vec(rows, q, tys, l);
        }
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
            // A slice makes its column a vector column (above).
            P::Slice(_) => None,
        }
    }

    /// When column 0 is a `Pair` or `Triple` (§7 L3b) and holds vector
    /// patterns, the matrix with each of them as the struct's one
    /// constructor over its fields; `None` when there is nothing to
    /// change.
    fn tuple_column(
        &mut self,
        rows: &[Vec<P>],
        q: &[P],
        tys: &[Ty],
    ) -> Option<(Vec<Vec<P>>, Vec<P>)> {
        let Ty::Con(Con::Nominal(id), _) = self.st.zonk(&tys[0]) else {
            return None;
        };
        if !self.g.is_tuple(id) {
            return None;
        }
        let Shape::Struct(fields) = &self.g.ty(id).shape else {
            return None;
        };
        let n = fields.len();
        let is_vec = |p: &P| matches!(p, P::Slice(_) | P::Ctor(Ctor::Len(_), _));
        if !rows.iter().map(|r| &r[0]).chain(q.first()).any(is_vec) {
            return None;
        }
        let fix = |row: &[P]| {
            let head = match &row[0] {
                P::Slice(subs) | P::Ctor(Ctor::Len(_), subs) => {
                    let mut subs = subs.clone();
                    subs.resize(n, P::Wild);
                    P::Ctor(Ctor::Variant(0), subs)
                }
                other => other.clone(),
            };
            [vec![head], row[1..].to_vec()].concat()
        };
        Some((rows.iter().map(|r| fix(r)).collect(), fix(q)))
    }

    /// `L` when column 0 holds a vector pattern; notes a mix with
    /// variant patterns.
    fn vec_bound(&mut self, rows: &[Vec<P>], head: &P) -> Option<usize> {
        let mut bound = None;
        let mut variant = false;
        for p in rows.iter().map(|r| &r[0]).chain(std::iter::once(head)) {
            let l = match p {
                P::Ctor(Ctor::Len(n), _) => n + 1,
                P::Slice(pre) => pre.len(),
                P::Ctor(Ctor::Variant(_), _) => {
                    variant = true;
                    continue;
                }
                _ => continue,
            };
            bound = Some(bound.map_or(l, |b: usize| b.max(l)));
        }
        self.mixed |= bound.is_some() && variant;
        bound
    }

    fn useful_vec(&mut self, rows: &[Vec<P>], q: &[P], tys: &[Ty], l: usize) -> Option<Vec<P>> {
        let elem = match self.st.zonk(&tys[0]) {
            Ty::Con(Con::Nominal(_), args) => args.first().cloned().unwrap_or(Ty::unit()),
            _ => Ty::unit(),
        };
        let ctors = (0..l)
            .map(Ctor::Len)
            .chain(std::iter::once(Ctor::AtLeast(l)));
        for c in ctors {
            let n = arity(&c);
            let Some(mut q2) = expand(&q[0], &c) else {
                continue;
            };
            q2.extend_from_slice(&q[1..]);
            let rows2: Vec<Vec<P>> = rows
                .iter()
                .filter_map(|r| expand(&r[0], &c).map(|h| [h, r[1..].to_vec()].concat()))
                .collect();
            let tys2 = [vec![elem.clone(); n], tys[1..].to_vec()].concat();
            if let Some(w) = self.useful(&rows2, &q2, &tys2) {
                return Some(rebuild(c, n, w));
            }
        }
        None
    }

    fn useful_wild(&mut self, rows: &[Vec<P>], q: &[P], tys: &[Ty]) -> Option<Vec<P>> {
        let used: Vec<Ctor> = rows
            .iter()
            .filter_map(|r| match &r[0] {
                P::Ctor(c, _) => Some(c.clone()),
                P::Wild | P::Slice(_) => None,
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

fn arity(c: &Ctor) -> usize {
    match c {
        Ctor::Len(n) | Ctor::AtLeast(n) => *n,
        _ => 0,
    }
}

/// The sub-patterns `p` has under the vector constructor `c`, or `None`
/// when `p` cannot match it.
fn expand(p: &P, c: &Ctor) -> Option<Vec<P>> {
    let n = arity(c);
    match p {
        P::Wild => Some(vec![P::Wild; n]),
        P::Ctor(d, args) if d == c => Some(args.clone()),
        P::Slice(pre) if pre.len() <= n => {
            let mut out = pre.clone();
            out.resize(n, P::Wild);
            Some(out)
        }
        _ => None,
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
                fs.iter()
                    .map(|f| f.ty.subst_gen(&args, &colour_args(&args)))
                    .collect(),
            )],
            Shape::Enum(vs) => vs
                .iter()
                .enumerate()
                .map(|(i, v)| {
                    let fields = v
                        .fields
                        .iter()
                        .map(|f| f.ty.subst_gen(&args, &colour_args(&args)));
                    (Ctor::Variant(i), fields.collect())
                })
                .collect(),
        }),
        _ => None,
    }
}

fn specialise(rows: &[Vec<P>], c: &Ctor, arity: usize) -> Vec<Vec<P>> {
    rows.iter()
        .filter_map(|r| match &r[0] {
            P::Ctor(d, args) if d == c => Some([args.clone(), r[1..].to_vec()].concat()),
            P::Ctor(..) | P::Slice(_) => None,
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
