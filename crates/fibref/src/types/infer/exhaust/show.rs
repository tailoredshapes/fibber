//! Printing a missing case in the spelling of syntax §3.6.

use crate::types::decls::{Globals, Shape};
use crate::types::store::Store;
use crate::types::ty::{Con, Ty, TypeId};

use super::matrix::{Ctor, P};

/// The witness `p` of type `t` as a pattern.
pub(super) fn show(g: &Globals, st: &mut Store, p: &P, t: &Ty) -> String {
    let P::Ctor(c, args) = p else {
        return "_".into();
    };
    let (name, fields): (String, Vec<Ty>) = match (c, st.zonk(t)) {
        (Ctor::Bool(b), _) => return b.to_string(),
        (Ctor::Lit(l), _) => return l.clone(),
        (Ctor::Len(_) | Ctor::AtLeast(_), zt) => return vector(g, st, c, args, &zt),
        // A variant of the prelude's Vec is a set of lengths (§2.6).
        (Ctor::Variant(i), Ty::Con(Con::Nominal(id), _)) if g.vec == Some(id) => {
            return if *i == 0 { "[]" } else { "[_ & _]" }.into();
        }
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

/// `[a..]` for a length, `[a.. & _]` for a least length.
fn vector(g: &Globals, st: &mut Store, c: &Ctor, args: &[P], t: &Ty) -> String {
    let elem = match t {
        Ty::Con(_, targs) => targs.first().cloned().unwrap_or(Ty::unit()),
        _ => Ty::unit(),
    };
    let mut items: Vec<String> = args.iter().map(|a| show(g, st, a, &elem)).collect();
    if matches!(c, Ctor::AtLeast(_)) {
        items.push("&".into());
        items.push("_".into());
    }
    format!("[{}]", items.join(" "))
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
