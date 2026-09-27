//! Which types are objects (types §1): everything but the scalars of
//! §1.1 and field-less enums. A quantified variable is treated as an
//! object for every rule (§6), which is safe.

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
}
