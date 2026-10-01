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
    Dyn,
    Unit,
}

/// The class of a concrete type.
pub fn class_of(g: &Globals, t: &Ty) -> Result<Class, Unsupported> {
    Ok(match lir_ty(g, t)? {
        None => Class::Unit,
        Some(LirTy::Dyn) => Class::Dyn,
        Some(LirTy::Ptr) => match option_payload(g, t) {
            Some(p) if option_rep(g, p)? == OptRep::Null => Class::Opt,
            _ => Class::Ptr,
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

/// The bodies still to emit, and the name of every one requested.
#[derive(Debug, Default)]
pub struct Queue {
    pending: VecDeque<Inst>,
    names: HashMap<Inst, String>,
}

impl Queue {
    /// The name of the specialisation, queued for emission if new.
    pub fn request(&mut self, g: &Globals, inst: Inst) -> String {
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
        assert_eq!(class_of(g, &opt2).unwrap(), Class::Ptr);
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
