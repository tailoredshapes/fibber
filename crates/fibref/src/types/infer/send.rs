//! The `Send` predicate (spec/types.md §5.1) evaluated structurally, with
//! the witness path of §5.3 when it fails.
//!
//! Path steps, outermost first, are spelled: `closure capture x`, `async
//! capture x`, `field f of S` (a struct), `payload of V` / `payload i of
//! V` (an enum variant with one field / several), `element of (Array
//! T)`, `target of (Weak T)`, `result of (Task T)`. For recursive
//! nominal types `Send` is the greatest fixpoint: a type already being
//! examined is assumed sendable.

use crate::syntax::Pos;

use crate::types::decls::{Globals, Shape};
use crate::types::display::Printer;
use crate::types::error::{ErrorKind, TypeError};
use crate::types::store::Store;
use crate::types::ty::{Colour, Con, Scalar, Ty, TypeId};

use super::cx::Witness;

/// What `Send T` still depends on when it did not fail: type variables
/// that must be `Send`, and colour variables that must be `send`, each
/// with its path.
#[derive(Clone, Debug, Default)]
pub struct Needs {
    /// Variables (unification, rigid or quantified).
    pub vars: Vec<(Ty, Vec<String>)>,
    /// Colour variables.
    pub colours: Vec<(Colour, Vec<String>)>,
}

/// Evaluates `Send ty` (zonking as it goes), `path` being the steps
/// that led here.
pub fn send_eval(g: &Globals, st: &mut Store, ty: &Ty, path: &[String]) -> Result<Needs, Witness> {
    let mut needs = Needs::default();
    let mut seen = Vec::new();
    let mut path = path.to_vec();
    let z = st.zonk(ty);
    walk(g, st, &z, &mut path, &mut seen, &mut needs)?;
    Ok(needs)
}

fn fail(path: &[String], t: &Ty) -> Result<(), Witness> {
    Err(Witness {
        path: path.to_vec(),
        offending: t.clone(),
    })
}

fn walk(
    g: &Globals,
    st: &mut Store,
    t: &Ty,
    path: &mut Vec<String>,
    seen: &mut Vec<(TypeId, Vec<Ty>)>,
    needs: &mut Needs,
) -> Result<(), Witness> {
    let step = |label: String, path: &mut Vec<String>| path.push(label);
    match t {
        Ty::Var(_) | Ty::Rigid(_) | Ty::Gen(_) => {
            needs.vars.push((t.clone(), path.clone()));
            Ok(())
        }
        Ty::Con(Con::Scalar(Scalar::Ptr), _) | Ty::Con(Con::Dyn(_), _) | Ty::Con(Con::Cell, _) => {
            fail(path, t)
        }
        Ty::Con(Con::Scalar(_), _) | Ty::Con(Con::Str, _) | Ty::Con(Con::Atom, _) => Ok(()),
        Ty::Con(c @ (Con::Array | Con::Weak | Con::Task), args) => {
            let what = match c {
                Con::Array => "element",
                Con::Weak => "target",
                _ => "result",
            };
            let label = format!("{what} of {}", Printer::new(g).ty(t));
            step(label, path);
            let r = args
                .iter()
                .try_for_each(|a| walk(g, st, a, path, seen, needs));
            path.pop();
            r
        }
        Ty::Con(Con::Nominal(id), args) => nominal(g, st, *id, args, path, seen, needs),
        Ty::Fn(k, _, _) => match k {
            Colour::Send => Ok(()),
            Colour::Local => fail(path, t),
            _ => {
                needs.colours.push((*k, path.clone()));
                Ok(())
            }
        },
    }
}

fn nominal(
    g: &Globals,
    st: &mut Store,
    id: TypeId,
    args: &[Ty],
    path: &mut Vec<String>,
    seen: &mut Vec<(TypeId, Vec<Ty>)>,
    needs: &mut Needs,
) -> Result<(), Witness> {
    let key = (id, args.to_vec());
    if seen.contains(&key) {
        return Ok(());
    }
    seen.push(key);
    let def = g.ty(id);
    let fields: Vec<(String, Ty)> = match &def.shape {
        Shape::Struct(fs) => fs
            .iter()
            .map(|f| (format!("field {} of {}", f.name, def.name), f.ty.clone()))
            .collect(),
        Shape::Enum(vs) => vs
            .iter()
            .flat_map(|v| {
                let one = v.fields.len() == 1;
                v.fields.iter().enumerate().map(move |(i, f)| {
                    let label = if one {
                        format!("payload of {}", v.name)
                    } else {
                        format!("payload {i} of {}", v.name)
                    };
                    (label, f.ty.clone())
                })
            })
            .collect(),
    };
    for (label, fty) in fields {
        path.push(label);
        let ft = st.zonk(&fty.subst_gen(args, &[]));
        let r = walk(g, st, &ft, path, seen, needs);
        path.pop();
        r?;
    }
    Ok(())
}

/// The error of §5.3 for a failed `Send`.
pub fn send_error(g: &Globals, w: &Witness, pos: &Pos, rigid: &[String]) -> TypeError {
    let p = Printer::with_names(g, &[], rigid);
    let path = if w.path.is_empty() {
        "the value".to_string()
    } else {
        w.path.join(", ")
    };
    let t = p.ty(&w.offending);
    match &w.offending {
        Ty::Con(Con::Cell, _) => TypeError::new(
            ErrorKind::CellNotSend,
            pos,
            format!("cell cannot be shared between threads: {path} has type {t}"),
        ),
        _ => TypeError::new(
            ErrorKind::ValueNotSend,
            pos,
            format!("value of type {t} cannot be shared between threads: {path}"),
        ),
    }
}
