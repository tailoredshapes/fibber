//! Type forms (spec/types.md §1) to [`TypeAnn`]: names resolved against
//! the module's types and protocols; a lowercase symbol that names no
//! type is a type variable.

use crate::syntax::{Form, FormKind};

use crate::types::ast::{ColourAnn, TypeAnn};
use crate::types::decls::{Globals, ModuleId, Space};
use crate::types::error::{TResult, TypeError};
use crate::types::ty::{Colour, Con, Scalar};

/// The built-in unary constructors.
fn builtin_con(name: &str) -> Option<Con> {
    match name {
        "Array" => Some(Con::Array),
        "Cell" => Some(Con::Cell),
        "Atom" => Some(Con::Atom),
        "Weak" => Some(Con::Weak),
        "Task" => Some(Con::Task),
        _ => None,
    }
}

/// Whether `name` can be a type variable: it starts with a lowercase
/// letter (or is a gensym-free name like `a1`).
fn is_var_name(name: &str) -> bool {
    name.chars().next().is_some_and(|c| c.is_ascii_lowercase())
}

/// Resolves the type form `form` in module `m`. `Self` is allowed when
/// `allow_self`.
pub fn type_ann(g: &Globals, m: ModuleId, form: &Form, allow_self: bool) -> TResult<TypeAnn> {
    match &form.kind {
        FormKind::Sym(name) => sym_type(g, m, name, form, allow_self),
        FormKind::List(items) if !items.is_empty() => list_type(g, m, items, form, allow_self),
        _ => Err(TypeError::resolve(
            &form.pos,
            format!("{form} is not a type"),
        )),
    }
}

fn sym_type(
    g: &Globals,
    m: ModuleId,
    name: &str,
    form: &Form,
    allow_self: bool,
) -> TResult<TypeAnn> {
    if let Some(s) = Scalar::from_name(name) {
        return Ok(TypeAnn::Scalar(s));
    }
    match name {
        "str" => return Ok(TypeAnn::Str),
        "Self" if allow_self => return Ok(TypeAnn::SelfTy),
        _ => {}
    }
    if let Some(id) = g.type_name(m, name) {
        let arity = g.ty(id).params.len();
        if arity != 0 {
            let msg = format!("type {name} takes {arity} argument(s)");
            return Err(TypeError::resolve(&form.pos, msg));
        }
        return Ok(TypeAnn::Nominal(id, Vec::new()));
    }
    if builtin_con(name).is_some() {
        let msg = format!("type {name} takes 1 argument");
        return Err(TypeError::resolve(&form.pos, msg));
    }
    if is_var_name(name) {
        return Ok(TypeAnn::Var(name.to_string()));
    }
    Err(TypeError::resolve(
        &form.pos,
        g.unknown(m, Space::Type, name, "unknown type"),
    ))
}

fn list_type(
    g: &Globals,
    m: ModuleId,
    items: &[Form],
    form: &Form,
    allow_self: bool,
) -> TResult<TypeAnn> {
    let Some(head) = items[0].as_sym() else {
        return Err(TypeError::resolve(
            &form.pos,
            format!("{form} is not a type"),
        ));
    };
    let args = &items[1..];
    match head {
        "fn" => return fn_type(g, m, args, form, allow_self),
        "dyn" => return dyn_type(g, m, args, form, allow_self),
        _ => {}
    }
    let anns = arg_anns(g, m, head, args, allow_self)?;
    if let Some(c) = builtin_con(head) {
        let mut anns = anns;
        return match (anns.pop(), anns.is_empty()) {
            (Some(a), true) => Ok(TypeAnn::Builtin(c, Box::new(a))),
            _ => Err(TypeError::resolve(
                &form.pos,
                format!("type {head} takes 1 argument"),
            )),
        };
    }
    let Some(id) = g.type_name(m, head) else {
        return Err(TypeError::resolve(
            &form.pos,
            g.unknown(m, Space::Type, head, "unknown type"),
        ));
    };
    let arity = g.ty(id).params.len();
    if arity != anns.len() {
        let msg = format!("type {head} takes {arity} argument(s), not {}", anns.len());
        return Err(TypeError::resolve(&form.pos, msg));
    }
    Ok(TypeAnn::Nominal(id, anns))
}

/// The arguments of the type constructor `head`: types, or colours at
/// its colour parameters (§1.3).
fn arg_anns(
    g: &Globals,
    m: ModuleId,
    head: &str,
    args: &[Form],
    allow_self: bool,
) -> TResult<Vec<TypeAnn>> {
    let colour_at = |i: usize| g.type_name(m, head).is_some_and(|id| g.ty(id).is_colour(i));
    args.iter()
        .enumerate()
        .map(|(i, a)| match colour_at(i) {
            true => colour_param_arg(a, head),
            false => type_ann(g, m, a, allow_self),
        })
        .collect()
}

/// `(fn κ? (A..) R)`.
fn fn_type(
    g: &Globals,
    m: ModuleId,
    args: &[Form],
    form: &Form,
    allow_self: bool,
) -> TResult<TypeAnn> {
    let (colour, rest) = match (args.first().map(|f| &f.kind), args.len()) {
        (Some(FormKind::Sym(k)), 3) if is_var_name(k) && Scalar::from_name(k).is_none() => {
            (Some(ColourAnn::Named(k.clone())), &args[1..])
        }
        (Some(_), _) => match colour_arg(&args[0]) {
            Some(c) => (Some(c), &args[1..]),
            None => (None, args),
        },
        _ => (None, args),
    };
    let bad = || TypeError::resolve(&form.pos, format!("malformed function type {form}"));
    let [params, ret] = rest else {
        return Err(bad());
    };
    let params = match &params.kind {
        FormKind::List(ps) => ps
            .iter()
            .map(|p| type_ann(g, m, p, allow_self))
            .collect::<TResult<Vec<_>>>()?,
        _ => return Err(bad()),
    };
    let ret = type_ann(g, m, ret, allow_self)?;
    Ok(TypeAnn::Fn(colour, params, Box::new(ret)))
}

/// `:send` or `:local`.
fn colour_arg(f: &Form) -> Option<ColourAnn> {
    match &f.kind {
        FormKind::Kw(k) if k == "send" => Some(ColourAnn::Fixed(Colour::Send)),
        FormKind::Kw(k) if k == "local" => Some(ColourAnn::Fixed(Colour::Local)),
        _ => None,
    }
}

/// An argument at a colour parameter (§1.3): `:send`, `:local` or a
/// colour variable.
fn colour_param_arg(f: &Form, head: &str) -> TResult<TypeAnn> {
    if let Some(c) = colour_arg(f) {
        return Ok(TypeAnn::ColourArg(c));
    }
    match f.as_sym() {
        Some(k) if is_var_name(k) && Scalar::from_name(k).is_none() && k != "str" => {
            Ok(TypeAnn::ColourArg(ColourAnn::Named(k.to_string())))
        }
        _ => Err(TypeError::resolve(
            &f.pos,
            format!("{f} is not a colour: {head} takes :send, :local or a colour variable there"),
        )),
    }
}

/// `(dyn P)` / `(dyn (P D..))`, each optionally followed by `:send`.
fn dyn_type(
    g: &Globals,
    m: ModuleId,
    args: &[Form],
    form: &Form,
    allow_self: bool,
) -> TResult<TypeAnn> {
    let (p, send) = match args {
        [p] => (p, false),
        [p, k] if matches!(&k.kind, FormKind::Kw(k) if k == "send") => (p, true),
        _ => return Err(TypeError::resolve(&form.pos, format!("malformed {form}"))),
    };
    let (proto, dets) = proto_ref(g, m, p, allow_self)?;
    Ok(TypeAnn::Dyn(proto, dets, send))
}

/// A protocol reference `P` or `(P D..)`, with the determined types.
pub fn proto_ref(
    g: &Globals,
    m: ModuleId,
    form: &Form,
    allow_self: bool,
) -> TResult<(crate::types::ty::ProtoId, Vec<TypeAnn>)> {
    let (name, dets) = match &form.kind {
        FormKind::Sym(n) => (n.as_str(), &[][..]),
        FormKind::List(items) if !items.is_empty() => match items[0].as_sym() {
            Some(n) => (n, &items[1..]),
            None => return Err(TypeError::resolve(&form.pos, "expected a protocol")),
        },
        _ => return Err(TypeError::resolve(&form.pos, "expected a protocol")),
    };
    let Some(p) = g.proto_name(m, name) else {
        return Err(TypeError::resolve(
            &form.pos,
            g.unknown(m, Space::Proto, name, "unknown protocol"),
        ));
    };
    let want = g.proto(p).params.len() - 1;
    if want != dets.len() {
        let msg = format!("protocol {name} has {want} determined parameter(s)");
        return Err(TypeError::resolve(&form.pos, msg));
    }
    let dets = dets
        .iter()
        .map(|d| type_ann(g, m, d, allow_self))
        .collect::<TResult<Vec<_>>>()?;
    Ok((p, dets))
}
