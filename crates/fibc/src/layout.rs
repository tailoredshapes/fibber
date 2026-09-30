//! The representation of every type (types §8.1) and the layout of
//! every object (§8.2–§8.7): the lIR type of a fibber type, the
//! `Option` rule, and sizes.

use fibref::types::decls::{Globals, Shape};
use fibref::types::ty::{Con, Scalar, Ty};

use crate::compile::Unsupported;
use crate::ir::LirTy;

/// The lIR type of a concrete fibber type; `None` for `unit`.
pub fn lir_ty(g: &Globals, t: &Ty) -> Result<Option<LirTy>, Unsupported> {
    Ok(Some(match t {
        Ty::Con(Con::Scalar(s), _) => match s {
            Scalar::Bool => LirTy::I1,
            Scalar::I8 => LirTy::I8,
            Scalar::I16 => LirTy::I16,
            Scalar::I32 | Scalar::Char => LirTy::I32,
            Scalar::I64 | Scalar::Keyword => LirTy::I64,
            Scalar::F32 => LirTy::Float,
            Scalar::F64 => LirTy::Double,
            Scalar::Ptr => LirTy::Ptr,
            Scalar::Unit => return Ok(None),
        },
        Ty::Con(Con::Weak, args) => match args.first() {
            Some(Ty::Con(Con::Dyn(..), _)) => LirTy::Dyn,
            _ => LirTy::Ptr,
        },
        Ty::Con(Con::Dyn(..), _) => LirTy::Dyn,
        Ty::Con(Con::Nominal(id), _) if g.ty(*id).is_fieldless_enum() => LirTy::I32,
        Ty::Con(_, _) | Ty::Fn(..) => LirTy::Ptr,
        Ty::Gen(_) | Ty::Var(_) | Ty::Rigid(_) => {
            return Err(Unsupported(format!(
                "a type variable reached the lowering: {t:?}"
            )))
        }
    }))
}

/// Whether a type is an object type: its values are counted pointers
/// (or a `dyn`, whose object word is).
pub fn is_object(g: &Globals, t: &Ty) -> bool {
    match t {
        Ty::Con(Con::Scalar(_), _) => false,
        Ty::Con(Con::Nominal(id), _) => !g.ty(*id).is_fieldless_enum(),
        _ => true,
    }
}

/// How an `(Option T)` is represented (§8.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OptRep {
    /// A nullable pointer: `nil` is null, `some` allocates nothing.
    Null,
    /// A heap enum with a tag and the payload.
    Boxed,
}

/// The payload type of an `Option`, if `t` is one.
pub fn option_payload<'t>(g: &Globals, t: &'t Ty) -> Option<&'t Ty> {
    match t {
        Ty::Con(Con::Nominal(id), args) if *id == g.option => args.first(),
        _ => None,
    }
}

/// The representation of `(Option payload)`.
pub fn option_rep(g: &Globals, payload: &Ty) -> Result<OptRep, Unsupported> {
    let plain_object = lir_ty(g, payload)? == Some(LirTy::Ptr)
        && option_payload(g, payload).is_none()
        && !matches!(payload, Ty::Con(Con::Scalar(Scalar::Ptr), _));
    Ok(if plain_object {
        OptRep::Null
    } else {
        OptRep::Boxed
    })
}

/// The lIR types of the fields of a struct or of one variant, in
/// declaration order, at the concrete arguments.
pub fn field_tys(
    fields: &[fibref::types::decls::FieldDef],
    args: &[Ty],
) -> Result<Vec<Ty>, Unsupported> {
    Ok(fields.iter().map(|f| f.ty.subst_gen(args, &[])).collect())
}

/// A variant: its name (`None` for a struct) and its field types.
pub type Variant = (Option<String>, Vec<Ty>);

/// The names and field types of a nominal type's variants (one entry,
/// `None`-named, for a struct).
pub fn variants(
    g: &Globals,
    id: fibref::types::ty::TypeId,
    args: &[Ty],
) -> Result<Vec<Variant>, Unsupported> {
    let def = g.ty(id);
    match &def.shape {
        Shape::Struct(fs) => Ok(vec![(None, field_tys(fs, args)?)]),
        Shape::Enum(vs) => vs
            .iter()
            .map(|v| Ok((Some(v.name.clone()), field_tys(&v.fields, args)?)))
            .collect(),
    }
}

/// The size and alignment of an lIR type on the targets lIR supports.
pub fn size_align(t: LirTy) -> (u64, u64) {
    match t {
        LirTy::I1 | LirTy::I8 => (1, 1),
        LirTy::I16 => (2, 2),
        LirTy::I32 | LirTy::Float => (4, 4),
        LirTy::I64 | LirTy::Double | LirTy::Ptr => (8, 8),
        LirTy::Dyn => (16, 8),
    }
}

/// The size of a struct of these fields, laid out as LLVM lays out a
/// non-packed struct: each field at its alignment, the whole rounded
/// up to the largest alignment.
pub fn struct_size(fields: &[LirTy]) -> u64 {
    struct_size_of(&fields.iter().map(|f| size_align(*f)).collect::<Vec<_>>())
}

/// [`struct_size`] over (size, alignment) pairs.
pub fn struct_size_of(fields: &[(u64, u64)]) -> u64 {
    let mut off = 0u64;
    let mut max_align = 1u64;
    for (s, a) in fields {
        off = off.div_ceil(*a) * a + s;
        max_align = max_align.max(*a);
    }
    off.div_ceil(max_align) * max_align
}

/// The header of every object (§8.2).
pub const HEADER: [LirTy; 3] = [LirTy::I64, LirTy::I32, LirTy::I32];

/// Header flag bits (§8.2).
pub const SHARED: i64 = 1;
pub const HAS_WEAK: i64 = 2;
pub const STACK: i64 = 4;
pub const IMMORTAL: i64 = 8;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_follow_the_c_layout() {
        assert_eq!(struct_size(&HEADER), 16);
        assert_eq!(
            struct_size(&[LirTy::I64, LirTy::I32, LirTy::I32, LirTy::I64, LirTy::Ptr]),
            32
        );
        assert_eq!(
            struct_size(&[LirTy::I64, LirTy::I32, LirTy::I32, LirTy::I32, LirTy::Ptr]),
            32
        );
        assert_eq!(
            struct_size(&[LirTy::I64, LirTy::I32, LirTy::I32, LirTy::I1]),
            24
        );
        assert_eq!(
            struct_size(&[LirTy::I64, LirTy::I32, LirTy::I32, LirTy::Dyn]),
            32
        );
    }

    #[test]
    fn option_of_an_object_is_a_null_pointer_and_of_a_scalar_a_box() {
        let checked = fibref::own::check_source("(defun main () -> i64 1)", "t").expect("checks");
        let g = &checked.typed.globals;
        assert_eq!(option_rep(g, &Ty::str()).unwrap(), OptRep::Null);
        assert_eq!(option_rep(g, &Ty::i64()).unwrap(), OptRep::Boxed);
        let opt_str = Ty::nominal(g.option, vec![Ty::str()]);
        assert_eq!(option_rep(g, &opt_str).unwrap(), OptRep::Boxed);
        assert_eq!(lir_ty(g, &opt_str).unwrap(), Some(LirTy::Ptr));
        assert_eq!(lir_ty(g, &Ty::unit()).unwrap(), None);
        assert!(!is_object(g, &Ty::bool()) && is_object(g, &Ty::str()));
    }
}
