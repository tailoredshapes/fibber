//! The type grammar of spec/types.md §1 as Rust data.
//!
//! A [`Ty`] is a unification variable ([`Ty::Var`], a node of the
//! union-find store), a quantified variable of a scheme ([`Ty::Gen`]),
//! a rigid variable ([`Ty::Rigid`]: an annotation's named variable
//! while its definition is checked, §3.1), a constructor applied to
//! arguments ([`Ty::Con`]), or a function type with a colour
//! ([`Ty::Fn`], §1.4). `Option` and `Form` are nominal enums like any
//! `defenum` (their names are special only to the reader and the
//! expander), so they are [`Con::Nominal`].

/// A nominal type: an index into the table of structs and enums.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TypeId(pub u32);

/// A protocol: an index into the table of protocols.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProtoId(pub u32);

/// A unification variable: an index into the store.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TvId(pub u32);

/// A colour variable ς (§5.4): a separate sort, never bound by
/// unification.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CvId(pub u32);

/// The scalar types of §1.1 (a field-less enum is a scalar too, but it
/// is nominal, so it is a [`Con::Nominal`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Scalar {
    /// `bool`
    Bool,
    /// `i8`
    I8,
    /// `i16`
    I16,
    /// `i32`
    I32,
    /// `i64`
    I64,
    /// `f32`
    F32,
    /// `f64`
    F64,
    /// `char`
    Char,
    /// `keyword`
    Keyword,
    /// `unit`
    Unit,
    /// `ptr`, only inside `unsafe`
    Ptr,
}

impl Scalar {
    /// Every scalar type.
    pub const ALL: [Scalar; 11] = [
        Scalar::Bool,
        Scalar::I8,
        Scalar::I16,
        Scalar::I32,
        Scalar::I64,
        Scalar::F32,
        Scalar::F64,
        Scalar::Char,
        Scalar::Keyword,
        Scalar::Unit,
        Scalar::Ptr,
    ];

    /// The type's name as written.
    pub fn name(self) -> &'static str {
        match self {
            Scalar::Bool => "bool",
            Scalar::I8 => "i8",
            Scalar::I16 => "i16",
            Scalar::I32 => "i32",
            Scalar::I64 => "i64",
            Scalar::F32 => "f32",
            Scalar::F64 => "f64",
            Scalar::Char => "char",
            Scalar::Keyword => "keyword",
            Scalar::Unit => "unit",
            Scalar::Ptr => "ptr",
        }
    }

    /// The scalar named `name`.
    pub fn from_name(name: &str) -> Option<Scalar> {
        Scalar::ALL.into_iter().find(|s| s.name() == name)
    }

    /// Whether this is an integer type.
    pub fn is_int(self) -> bool {
        matches!(self, Scalar::I8 | Scalar::I16 | Scalar::I32 | Scalar::I64)
    }

    /// Whether this is a float type.
    pub fn is_float(self) -> bool {
        matches!(self, Scalar::F32 | Scalar::F64)
    }
}

/// A type constructor: the head of a [`Ty::Con`]. Instances are keyed
/// by `(protocol, Con)` (§4.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Con {
    /// A scalar; no arguments.
    Scalar(Scalar),
    /// `str`; no arguments.
    Str,
    /// `(Array T)`
    Array,
    /// `(Cell T)`
    Cell,
    /// `(Atom T)`
    Atom,
    /// `(Weak T)`
    Weak,
    /// `(Task T)`
    Task,
    /// A struct or enum, applied to its parameters.
    Nominal(TypeId),
    /// `(dyn P)` or `(dyn (P D..))`: the arguments are the determined
    /// parameters `D..`.
    Dyn(ProtoId),
}

/// A closure colour (§1.4, §5.4): `send ⊑ local`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Colour {
    /// May cross a thread.
    Send,
    /// May not cross a thread.
    Local,
    /// A colour variable of the unit being solved.
    Var(CvId),
    /// The `i`th quantified colour variable of a scheme.
    Gen(u32),
}

/// A type (§1).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Ty {
    /// A unification variable; look it up in the store.
    Var(TvId),
    /// The `i`th quantified type variable of the enclosing scheme.
    Gen(u32),
    /// A rigid variable: the `i`th named variable of the definition
    /// being checked (an annotation's variable, an `impl`'s parameter).
    Rigid(u32),
    /// A constructor applied to arguments.
    Con(Con, Vec<Ty>),
    /// `(fn κ (A..) R)`.
    Fn(Colour, Vec<Ty>, Box<Ty>),
}

impl Ty {
    /// A scalar type.
    pub fn scalar(s: Scalar) -> Ty {
        Ty::Con(Con::Scalar(s), Vec::new())
    }

    /// `i64`, the type of an unsuffixed integer literal.
    pub fn i64() -> Ty {
        Ty::scalar(Scalar::I64)
    }

    /// `bool`
    pub fn bool() -> Ty {
        Ty::scalar(Scalar::Bool)
    }

    /// `unit`
    pub fn unit() -> Ty {
        Ty::scalar(Scalar::Unit)
    }

    /// `str`
    pub fn str() -> Ty {
        Ty::Con(Con::Str, Vec::new())
    }

    /// `(Cell t)`
    pub fn cell(t: Ty) -> Ty {
        Ty::Con(Con::Cell, vec![t])
    }

    /// `(Task t)`
    pub fn task(t: Ty) -> Ty {
        Ty::Con(Con::Task, vec![t])
    }

    /// A nominal type applied to arguments.
    pub fn nominal(id: TypeId, args: Vec<Ty>) -> Ty {
        Ty::Con(Con::Nominal(id), args)
    }

    /// Replaces every `Gen(i)` by `tys[i]` and every `Colour::Gen(i)` by
    /// `colours[i]`. Indices out of range are left alone.
    pub fn subst_gen(&self, tys: &[Ty], colours: &[Colour]) -> Ty {
        match self {
            Ty::Gen(i) => tys.get(*i as usize).cloned().unwrap_or(Ty::Gen(*i)),
            Ty::Var(_) | Ty::Rigid(_) => self.clone(),
            Ty::Con(c, args) => {
                Ty::Con(*c, args.iter().map(|a| a.subst_gen(tys, colours)).collect())
            }
            Ty::Fn(k, ps, r) => Ty::Fn(
                subst_colour(*k, colours),
                ps.iter().map(|p| p.subst_gen(tys, colours)).collect(),
                Box::new(r.subst_gen(tys, colours)),
            ),
        }
    }

    /// Replaces every `Rigid(i)` by `tys[i]` (out of range: kept).
    pub fn subst_rigid(&self, tys: &[Ty]) -> Ty {
        self.map_leaves(&mut |t| match t {
            Ty::Rigid(i) => tys.get(*i as usize).cloned(),
            _ => None,
        })
    }

    /// Rebuilds the type, replacing each leaf (`Var`, `Gen`, `Rigid`)
    /// for which `f` returns a type.
    pub fn map_leaves(&self, f: &mut dyn FnMut(&Ty) -> Option<Ty>) -> Ty {
        match self {
            Ty::Var(_) | Ty::Gen(_) | Ty::Rigid(_) => f(self).unwrap_or_else(|| self.clone()),
            Ty::Con(c, args) => Ty::Con(*c, args.iter().map(|a| a.map_leaves(f)).collect()),
            Ty::Fn(k, ps, r) => Ty::Fn(
                *k,
                ps.iter().map(|p| p.map_leaves(f)).collect(),
                Box::new(r.map_leaves(f)),
            ),
        }
    }

    /// Rebuilds the type with every colour passed through `f`.
    pub fn map_colours(&self, f: &mut dyn FnMut(Colour) -> Colour) -> Ty {
        match self {
            Ty::Var(_) | Ty::Gen(_) | Ty::Rigid(_) => self.clone(),
            Ty::Con(c, args) => Ty::Con(*c, args.iter().map(|a| a.map_colours(f)).collect()),
            Ty::Fn(k, ps, r) => Ty::Fn(
                f(*k),
                ps.iter().map(|p| p.map_colours(f)).collect(),
                Box::new(r.map_colours(f)),
            ),
        }
    }

    /// Calls `f` on every leaf and every colour, in order.
    pub fn visit(&self, f: &mut dyn FnMut(Leaf)) {
        match self {
            Ty::Var(v) => f(Leaf::Var(*v)),
            Ty::Gen(i) => f(Leaf::Gen(*i)),
            Ty::Rigid(i) => f(Leaf::Rigid(*i)),
            Ty::Con(_, args) => args.iter().for_each(|a| a.visit(f)),
            Ty::Fn(k, ps, r) => {
                f(Leaf::Colour(*k));
                ps.iter().for_each(|p| p.visit(f));
                r.visit(f);
            }
        }
    }

    /// The unification variables in the type, in order, without repeats.
    pub fn vars(&self) -> Vec<TvId> {
        let mut out = Vec::new();
        self.visit(&mut |l| {
            if let Leaf::Var(v) = l {
                if !out.contains(&v) {
                    out.push(v);
                }
            }
        });
        out
    }
}

/// What [`Ty::visit`] reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Leaf {
    /// A unification variable.
    Var(TvId),
    /// A quantified variable.
    Gen(u32),
    /// A rigid variable.
    Rigid(u32),
    /// The colour of a function type.
    Colour(Colour),
}

/// `colours[i]` for `Colour::Gen(i)`, else `k`.
pub fn subst_colour(k: Colour, colours: &[Colour]) -> Colour {
    match k {
        Colour::Gen(i) => colours.get(i as usize).copied().unwrap_or(k),
        _ => k,
    }
}

/// A predicate of a scheme's context or of the worklist (§1.8).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Pred {
    /// `(P T₁ .. Tₙ)`: `T₁` is the dispatch position.
    Proto(ProtoId, Vec<Ty>),
    /// `(Send T)` (§5.1).
    Send(Ty),
    /// `(Object T)`: `T` is not a scalar (§2.11, §2.15).
    Object(Ty),
    /// `(Weakable T)`: `T` is an object type that is not an `Option`
    /// (§2.11): what `weak` can observe.
    Weakable(Ty),
}

impl Pred {
    /// The predicate with `f` applied to every type in it.
    pub fn map_tys(&self, f: &mut dyn FnMut(&Ty) -> Ty) -> Pred {
        match self {
            Pred::Proto(p, args) => Pred::Proto(*p, args.iter().map(&mut *f).collect()),
            Pred::Send(t) => Pred::Send(f(t)),
            Pred::Object(t) => Pred::Object(f(t)),
            Pred::Weakable(t) => Pred::Weakable(f(t)),
        }
    }

    /// Every type in the predicate.
    pub fn tys(&self) -> Vec<&Ty> {
        match self {
            Pred::Proto(_, args) => args.iter().collect(),
            Pred::Send(t) | Pred::Object(t) | Pred::Weakable(t) => vec![t],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scalar_names_round_trip() {
        for s in Scalar::ALL {
            assert_eq!(Scalar::from_name(s.name()), Some(s));
        }
        assert_eq!(Scalar::from_name("int"), None);
        assert!(Scalar::I32.is_int() && !Scalar::F32.is_int() && Scalar::F64.is_float());
    }

    #[test]
    fn subst_gen_replaces_types_and_colours() {
        let t = Ty::Fn(Colour::Gen(0), vec![Ty::Gen(0)], Box::new(Ty::Gen(1)));
        let out = t.subst_gen(&[Ty::i64(), Ty::str()], &[Colour::Local]);
        assert_eq!(
            out,
            Ty::Fn(Colour::Local, vec![Ty::i64()], Box::new(Ty::str()))
        );
    }

    #[test]
    fn vars_are_listed_once_in_order() {
        let t = Ty::Con(
            Con::Array,
            vec![Ty::Fn(
                Colour::Send,
                vec![Ty::Var(TvId(2)), Ty::Var(TvId(1))],
                Box::new(Ty::Var(TvId(2))),
            )],
        );
        assert_eq!(t.vars(), vec![TvId(2), TvId(1)]);
    }
}
