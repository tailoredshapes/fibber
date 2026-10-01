//! Monomorphisation (types §4.3, compiler.md §7): the key of a
//! specialisation, its representative types, and the queue of bodies
//! to emit.

use std::collections::{HashMap, VecDeque};

use fibref::own::program::BodyKey;
use fibref::types::decls::Globals;
use fibref::types::scheme::Scheme;
use fibref::types::ty::{Con, Pred, ProtoId, Scalar, Ty};

use crate::compile::Unsupported;
use crate::ir::LirTy;
use crate::layout::{lir_ty, option_payload, option_rep, OptRep};
use crate::names::body_name;

/// One specialisation of a body: the concrete (representative) type of
/// each quantified variable the body's expressions may mention.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Inst {
    pub key: BodyKey,
    pub tys: Vec<Ty>,
}

impl Inst {
    /// A type of the body at this specialisation.
    pub fn subst(&self, t: &Ty) -> Ty {
        t.subst_gen(&self.tys, &[])
    }
}

/// The layout class of a concrete type (§4.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Class {
    Scalar(LirTy),
    Ptr,
    Opt,
    /// An `(Option T)` held as a heap enum (types §8.1): a payload that
    /// is a scalar, a unit or itself an `Option`. It is a pointer like
    /// `Ptr`, but `(Option it)` is a heap enum too, where `(Option str)`
    /// is a nullable pointer.
    Boxed,
    Dyn,
    Unit,
}

/// The class of a concrete type.
pub fn class_of(g: &Globals, t: &Ty) -> Result<Class, Unsupported> {
    Ok(match lir_ty(g, t)? {
        None => Class::Unit,
        Some(LirTy::Dyn) => Class::Dyn,
        Some(LirTy::Ptr) => match option_payload(g, t) {
            Some(p) => match option_rep(g, p)? {
                OptRep::Null => Class::Opt,
                OptRep::Boxed => Class::Boxed,
            },
            None => Class::Ptr,
        },
        Some(s) => Class::Scalar(s),
    })
}

/// The representative type of a class: what an unconstrained variable
/// becomes inside its specialisation (compiler.md §7).
pub fn representative(g: &Globals, c: Class) -> Ty {
    match c {
        Class::Scalar(LirTy::I1) => Ty::bool(),
        Class::Scalar(LirTy::I8) => Ty::scalar(Scalar::I8),
        Class::Scalar(LirTy::I16) => Ty::scalar(Scalar::I16),
        Class::Scalar(LirTy::I32) => Ty::scalar(Scalar::I32),
        Class::Scalar(LirTy::Float) => Ty::scalar(Scalar::F32),
        Class::Scalar(LirTy::Double) => Ty::scalar(Scalar::F64),
        Class::Scalar(LirTy::Raw) => Ty::scalar(Scalar::Ptr),
        Class::Scalar(_) => Ty::i64(),
        Class::Unit => Ty::unit(),
        Class::Ptr => Ty::str(),
        Class::Opt => Ty::nominal(g.option, vec![Ty::str()]),
        Class::Boxed => Ty::nominal(g.option, vec![Ty::i64()]),
        Class::Dyn => Ty::Con(Con::Dyn(ProtoId(0), false), Vec::new()),
    }
}

/// Whether quantified variable `i` of a scheme carries a protocol
/// bound (directly or inside a constraint), which keys it by its full
/// type.
pub fn bounded(s: &Scheme, i: u32) -> bool {
    s.preds.iter().any(|p| match p {
        Pred::Proto(_, tys) => tys.iter().any(|t| mentions(t, i)),
        _ => false,
    })
}

fn mentions(t: &Ty, i: u32) -> bool {
    let mut found = false;
    t.visit(&mut |l| {
        if l == fibref::types::ty::Leaf::Gen(i) {
            found = true;
        }
    });
    found
}

/// The key of a `defun`'s specialisation at the concrete `tys`: the
/// full type where bounded, the class's representative elsewhere.
pub fn fun_key(g: &Globals, s: &Scheme, tys: &[Ty]) -> Result<Vec<Ty>, Unsupported> {
    tys.iter()
        .enumerate()
        .map(|(i, t)| {
            if bounded(s, i as u32) {
                Ok(t.clone())
            } else {
                Ok(representative(g, class_of(g, t)?))
            }
        })
        .collect()
}

/// How deep a type may nest and how many nodes it may have to key a
/// specialisation. A body that is wanted at ever larger types (a
/// polymorphic recursion that never ends, through a method of an `impl`
/// or a cycle of functions and methods, which the checker's rule on
/// functions (types §3.6) does not see) would otherwise be compiled for
/// ever, its names growing with it, until memory ran out. No program
/// that was run needed more than a few dozen nodes.
const MAX_TYPE_DEPTH: usize = 64;
const MAX_TYPE_SIZE: usize = 1000;

/// The depth and the node count of a type.
fn extent(t: &Ty) -> (usize, usize) {
    let kids: Vec<&Ty> = match t {
        Ty::Con(_, args) => args.iter().collect(),
        Ty::Fn(_, params, ret) => params.iter().chain(std::iter::once(&**ret)).collect(),
        _ => Vec::new(),
    };
    let (mut depth, mut size) = (0, 1);
    for k in kids {
        let (d, n) = extent(k);
        depth = depth.max(d);
        size += n;
    }
    (depth + 1, size)
}

/// What a body is called in a message: a function's name, or `the
/// method m of P`.
fn describe(g: &Globals, key: BodyKey) -> String {
    match key {
        BodyKey::Fun(f) | BodyKey::AllOwned(f) => g.fun(f).name.clone(),
        BodyKey::Def(d) => g.def(d).name.clone(),
        BodyKey::Method(i, m) | BodyKey::MethodOwned(i, m) => {
            let inst = &g.instances[i];
            let proto = g.proto(inst.proto);
            let method = &proto.methods[inst.methods[m].index].name;
            format!("the method {method} of {}", proto.name)
        }
    }
}

/// The bodies still to emit, and the name of every one requested.
#[derive(Debug, Default)]
pub struct Queue {
    pending: VecDeque<Inst>,
    names: HashMap<Inst, String>,
    overflow: Option<String>,
}

impl Queue {
    /// The name of the specialisation, queued for emission if new. A
    /// request at a type past the limits above is not queued, and what
    /// it was is kept for [`Queue::overflow`].
    pub fn request(&mut self, g: &Globals, inst: Inst) -> String {
        let big = |t: &Ty| {
            let (depth, size) = extent(t);
            depth > MAX_TYPE_DEPTH || size > MAX_TYPE_SIZE
        };
        if self.overflow.is_some() || inst.tys.iter().any(big) {
            let _ = self.overflow.get_or_insert_with(|| describe(g, inst.key));
            return "@overflow".to_string();
        }
        if let Some(n) = self.names.get(&inst) {
            return n.clone();
        }
        let name = body_name(g, inst.key, &inst.tys);
        self.names.insert(inst.clone(), name.clone());
        self.pending.push_back(inst);
        name
    }

    /// The next body to emit.
    pub fn pop(&mut self) -> Option<Inst> {
        self.pending.pop_front()
    }

    /// The first body that was requested at a type past the limits, if
    /// any: the compilation cannot finish.
    pub fn overflow(&self) -> Option<&str> {
        self.overflow.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classes_and_representatives() {
        let checked = fibref::own::check_source("(defun main () -> i64 1)", "t").expect("checks");
        let g = &checked.typed.globals;
        assert_eq!(class_of(g, &Ty::str()).unwrap(), Class::Ptr);
        let opt = Ty::nominal(g.option, vec![Ty::str()]);
        assert_eq!(class_of(g, &opt).unwrap(), Class::Opt);
        let opt2 = Ty::nominal(g.option, vec![opt.clone()]);
        assert_eq!(class_of(g, &opt2).unwrap(), Class::Boxed);
        let opt_i64 = Ty::nominal(g.option, vec![Ty::i64()]);
        assert_eq!(class_of(g, &opt_i64).unwrap(), Class::Boxed);
        assert_eq!(representative(g, Class::Boxed), opt_i64);
        // The representative of a class is in that class, and an
        // `Option` of it has the representation of an `Option` of the
        // member: a pointer for `Ptr` and `Opt` that is plain only for `Ptr`.
        for t in [&Ty::str(), &opt, &opt2, &opt_i64] {
            let c = class_of(g, t).unwrap();
            assert_eq!(class_of(g, &representative(g, c)).unwrap(), c);
            let (a, b) = (
                Ty::nominal(g.option, vec![t.clone()]),
                Ty::nominal(g.option, vec![representative(g, c)]),
            );
            assert_eq!(class_of(g, &a).unwrap(), class_of(g, &b).unwrap());
        }
        assert_eq!(class_of(g, &Ty::i64()).unwrap(), Class::Scalar(LirTy::I64));
        assert_eq!(representative(g, Class::Opt), opt);
        let vec = g.vec.unwrap();
        let s = checked.typed.scheme("map").unwrap().clone();
        // map : (fn ((fn (a) b) (Vec a)) (Vec b)) has no bounds.
        let key = fun_key(g, &s, &[Ty::nominal(vec, vec![Ty::i64()]), Ty::bool()]).unwrap();
        assert_eq!(key, vec![Ty::str(), Ty::bool()]);
    }

    /// A body at `ptr` is not the body at an object: the object's counts
    /// would be taken on an address (types §4.3, §8.1).
    #[test]
    fn a_raw_ptr_is_a_scalar_class_of_its_own() {
        let checked = fibref::own::check_source("(defun main () -> i64 1)", "t").expect("checks");
        let g = &checked.typed.globals;
        let ptr = Ty::scalar(Scalar::Ptr);
        assert_eq!(class_of(g, &ptr).unwrap(), Class::Scalar(LirTy::Raw));
        assert_eq!(representative(g, Class::Scalar(LirTy::Raw)), ptr);
        let s = checked.typed.scheme("map").unwrap().clone();
        let key = fun_key(g, &s, &[ptr.clone(), Ty::bool()]).unwrap();
        assert_eq!(key, vec![ptr, Ty::bool()]);
        assert_ne!(key[0], fun_key(g, &s, &[Ty::str(), Ty::bool()]).unwrap()[0]);
    }
}
