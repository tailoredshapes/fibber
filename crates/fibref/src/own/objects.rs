//! Which types are objects (types §1): everything but the scalars of
//! §1.1 and field-less enums. A quantified variable is treated as an
//! object for every rule (§6), which is safe. Also how an `Option` is
//! represented (§8.1), which decides whether its constructors allocate.

use crate::types::decls::Globals;
use crate::types::ty::{Con, Ty};

/// Whether values of `t` are objects (counted, §1.2).
pub fn is_object(g: &Globals, t: &Ty) -> bool {
    match t {
        Ty::Con(Con::Scalar(_), _) => false,
        Ty::Con(Con::Nominal(id), _) => !g.ty(*id).is_fieldless_enum(),
        _ => true,
    }
}

/// How values of an `(Option T)` type are represented (types §8.1,
/// §8.3), which decides whether its `some` and `nil` allocate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OptionRep {
    /// `T` is an object that is not itself an `Option`: a nullable
    /// pointer; `some` and `nil` allocate nothing.
    Pointer,
    /// `T` is a scalar, a `dyn` or an `Option`: a heap enum object (tag
    /// and payload); `some` and `nil` each allocate one, on the heap,
    /// never on the stack (§6.11).
    HeapEnum,
    /// `T` is a quantified variable of a generic body, which the
    /// interpreter runs once for every instantiation (it does not
    /// monomorphise, §4.3): decided at run time by the payload.
    Generic,
}

/// The representation of `t` if it is an `(Option T)` type.
pub fn option_rep(g: &Globals, t: &Ty) -> Option<OptionRep> {
    let Ty::Con(Con::Nominal(id), args) = t else {
        return None;
    };
    if *id != g.option {
        return None;
    }
    Some(match args.first()? {
        Ty::Gen(_) | Ty::Var(_) | Ty::Rigid(_) => OptionRep::Generic,
        Ty::Con(Con::Dyn(..), _) => OptionRep::HeapEnum,
        Ty::Con(Con::Nominal(p), _) if *p == g.option => OptionRep::HeapEnum,
        payload if !is_object(g, payload) => OptionRep::HeapEnum,
        _ => OptionRep::Pointer,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ty::Scalar;

    #[test]
    fn scalars_are_not_objects_and_everything_else_is() {
        let g = crate::types::init::new_globals().expect("globals");
        assert!(!is_object(&g, &Ty::i64()));
        assert!(!is_object(&g, &Ty::scalar(Scalar::Unit)));
        assert!(is_object(&g, &Ty::str()));
        assert!(is_object(&g, &Ty::cell(Ty::i64())));
        assert!(is_object(&g, &Ty::Gen(0)));
        assert!(is_object(&g, &Ty::nominal(g.option, vec![Ty::i64()])));
    }

    #[test]
    fn option_representation_follows_section_8_1() {
        let g = crate::types::init::new_globals().expect("globals");
        let opt = |t: Ty| Ty::nominal(g.option, vec![t]);
        assert_eq!(option_rep(&g, &opt(Ty::str())), Some(OptionRep::Pointer));
        let cell = opt(Ty::cell(Ty::i64()));
        assert_eq!(option_rep(&g, &cell), Some(OptionRep::Pointer));
        assert_eq!(option_rep(&g, &opt(Ty::i64())), Some(OptionRep::HeapEnum));
        let unit = opt(Ty::scalar(Scalar::Unit));
        assert_eq!(option_rep(&g, &unit), Some(OptionRep::HeapEnum));
        let nested = opt(opt(Ty::str()));
        assert_eq!(option_rep(&g, &nested), Some(OptionRep::HeapEnum));
        assert_eq!(option_rep(&g, &opt(Ty::Gen(0))), Some(OptionRep::Generic));
        assert_eq!(option_rep(&g, &Ty::str()), None);
    }
}
