//! The builtin module: the built-in enums `Option` and `Form` (§1.5,
//! syntax §3.16), the built-in protocols with their instances (§2.9,
//! §2.12), and the builtin functions of the table in `builtins`.

use std::collections::HashMap;
use std::sync::Arc;

use crate::expand::ExpandCtx;
use crate::syntax::{read_all, Form, Pos};

use super::annot::{ann_to_ty, GenEnv};
use super::ast::{BuiltinId, GlobalRef};
use super::builtins::{BUILTINS, BUILTIN_PROTOCOLS};
use super::decls::{
    FieldDef, Globals, InstanceDef, ModuleId, ModuleInfo, Shape, TypeDef, VariantDef,
};
use super::error::{TResult, TypeError};
use super::lower::{
    declare_proto, define_value, pred_of, register_instance, resolve_proto, type_ann,
};
use super::scheme::Scheme;
use super::ty::{Colour, Con, Scalar, Ty, TypeId};

/// The position of built-in definitions.
pub fn builtin_pos() -> Pos {
    Pos {
        file: Arc::from("<builtin>"),
        line: 0,
        col: 0,
        start: 0,
        end: 0,
    }
}

/// Tables holding the builtin module, before the prelude.
pub fn new_globals() -> TResult<Globals> {
    let mut g = Globals {
        types: Vec::new(),
        protos: Vec::new(),
        instances: Vec::new(),
        funs: Vec::new(),
        defs: Vec::new(),
        externs: Vec::new(),
        builtin_schemes: Vec::new(),
        bindings: Vec::new(),
        modules: vec![
            ModuleInfo {
                ns: "the builtins".into(),
                chain: vec![ModuleId::BUILTIN],
                ..ModuleInfo::default()
            },
            ModuleInfo {
                ns: crate::expand::PRELUDE_NS.into(),
                chain: vec![ModuleId::PRELUDE, ModuleId::BUILTIN],
                ..ModuleInfo::default()
            },
        ],
        main: ModuleId::MAIN,
        instance_index: HashMap::new(),
        option: TypeId(0),
        form: None,
        vec: None,
        deref_proto: None,
        expr_count: 0,
    };
    declare_builtin_enum(&mut g, "Option")?;
    let form = declare_builtin_enum(&mut g, "Form")?;
    g.form = Some(form);
    g.types[0].shape = Shape::Enum(vec![
        VariantDef {
            name: "nil".into(),
            fields: Vec::new(),
        },
        VariantDef {
            name: "some".into(),
            fields: vec![FieldDef {
                name: "v".into(),
                ty: Ty::Gen(0),
            }],
        },
    ]);
    builtin_protocols(&mut g)?;
    builtin_instances(&mut g)?;
    Ok(g)
}

/// Declares `Option` or `Form` with its variant names (fields later).
fn declare_builtin_enum(g: &mut Globals, name: &str) -> TResult<TypeId> {
    let ctx = ExpandCtx::new();
    let Some(info) = ctx.enum_info(name) else {
        return Err(TypeError::other(
            &builtin_pos(),
            format!("the expander has no {name}"),
        ));
    };
    let id = TypeId(g.types.len() as u32);
    let params = info
        .params
        .iter()
        .filter_map(Form::as_sym)
        .map(str::to_string)
        .collect();
    g.types.push(TypeDef {
        name: name.into(),
        module: ModuleId::BUILTIN,
        params,
        colours: Vec::new(),
        shape: Shape::Enum(Vec::new()),
        pos: builtin_pos(),
    });
    g.names_mut(ModuleId::BUILTIN).types.insert(name.into(), id);
    for (i, v) in info.variants.iter().enumerate() {
        if v.name != "nil" {
            define_value(
                g,
                ModuleId::BUILTIN,
                &v.name,
                GlobalRef::Ctor(id, Some(i)),
                &builtin_pos(),
            )?;
        }
    }
    Ok(id)
}

fn builtin_protocols(g: &mut Globals) -> TResult<()> {
    let forms = read_all(BUILTIN_PROTOCOLS, "<builtin>")
        .map_err(|e| TypeError::other(&builtin_pos(), format!("builtin protocols: {e}")))?;
    for form in &forms {
        let id = declare_proto(g, ModuleId::BUILTIN, form)?;
        resolve_proto(g, ModuleId::BUILTIN, id, form)?;
    }
    g.deref_proto = g.proto_name(ModuleId::BUILTIN, "Deref");
    Ok(())
}

/// `Num` for the integers and floats, `Float` for the floats, `Bits` for
/// the integers, `Eq`, `Ord`, `Hash`, `Show` for every scalar and `str`
/// (§2.12); `Deref` for `Cell`, `Atom`, `Weak` (§2.9).
fn builtin_instances(g: &mut Globals) -> TResult<()> {
    let mut plain: Vec<(&str, Con)> = Vec::new();
    for s in Scalar::ALL {
        for p in ["Eq", "Ord", "Hash", "Show"] {
            plain.push((p, Con::Scalar(s)));
        }
        if s.is_int() || s.is_float() {
            plain.push(("Num", Con::Scalar(s)));
        }
        if s.is_float() {
            plain.push(("Float", Con::Scalar(s)));
        }
        if s.is_int() {
            plain.push(("Bits", Con::Scalar(s)));
        }
    }
    for p in ["Eq", "Ord", "Hash", "Show"] {
        plain.push((p, Con::Str));
    }
    for (p, con) in plain {
        add_instance(g, p, con, 0, Vec::new())?;
    }
    add_instance(g, "Deref", Con::Cell, 1, vec![Ty::Gen(0)])?;
    add_instance(g, "Deref", Con::Atom, 1, vec![Ty::Gen(0)])?;
    let opt = Ty::nominal(g.option, vec![Ty::Gen(0)]);
    add_instance(g, "Deref", Con::Weak, 1, vec![opt])
}

fn add_instance(g: &mut Globals, proto: &str, con: Con, vars: u32, dets: Vec<Ty>) -> TResult<()> {
    let Some(p) = g.proto_name(ModuleId::BUILTIN, proto) else {
        return Err(TypeError::other(
            &builtin_pos(),
            format!("no builtin protocol {proto}"),
        ));
    };
    let head = Ty::Con(con, (0..vars).map(Ty::Gen).collect());
    let var_names = (0..vars)
        .map(|i| super::display::letter_name(i as usize))
        .collect();
    let inst = InstanceDef {
        proto: p,
        con,
        module: ModuleId::BUILTIN,
        var_names,
        head,
        dets,
        context: Vec::new(),
        methods: Vec::new(),
        pos: builtin_pos(),
    };
    register_instance(g, inst).map(|_| ())
}

/// After the prelude's types are declared: `Form`'s fields (which
/// mention the prelude's `Vec`) and the builtin functions.
pub fn finish_builtins(g: &mut Globals) -> TResult<()> {
    g.vec = g.type_name(ModuleId::PRELUDE, "Vec");
    form_fields(g)?;
    for (i, sig) in BUILTINS.iter().enumerate() {
        let scheme = builtin_scheme(g, sig.sig, sig.bounds).map_err(|e| {
            TypeError::other(
                &builtin_pos(),
                format!("builtin {}: {}", sig.name, e.message),
            )
        })?;
        g.builtin_schemes.push(scheme);
        define_value(
            g,
            ModuleId::BUILTIN,
            sig.name,
            GlobalRef::Builtin(BuiltinId(i as u32)),
            &builtin_pos(),
        )?;
    }
    Ok(())
}

fn form_fields(g: &mut Globals) -> TResult<()> {
    let ctx = ExpandCtx::new();
    let (Some(info), Some(form)) = (ctx.enum_info("Form"), g.form) else {
        return Ok(());
    };
    let mut variants = Vec::new();
    for v in &info.variants {
        let mut fields = Vec::new();
        for (i, (name, t)) in v.fields.iter().enumerate() {
            let ann = type_ann(g, ModuleId::PRELUDE, t, false)?;
            let ty = ann_to_ty(&ann, &mut GenEnv::default(), &t.pos)?;
            let name = name.clone().unwrap_or_else(|| i.to_string());
            fields.push(FieldDef { name, ty });
        }
        variants.push(VariantDef {
            name: v.name.clone(),
            fields,
        });
    }
    g.types[form.0 as usize].shape = Shape::Enum(variants);
    Ok(())
}

/// Parses `(fn (P..) R)` with `(& T)` positions, and `((C T..) ..)`.
fn builtin_scheme(g: &Globals, sig: &str, bounds: &str) -> TResult<Scheme> {
    let pos = builtin_pos();
    let read =
        |s: &str| read_all(s, "<builtin>").map_err(|e| TypeError::other(&pos, e.to_string()));
    let forms = read(sig)?;
    let bad = || TypeError::other(&pos, format!("malformed builtin signature {sig}"));
    let items = forms.first().and_then(Form::as_list).ok_or_else(bad)?;
    let [_, params, ret] = items else {
        return Err(bad());
    };
    let mut env = GenEnv::default();
    let mut tys = Vec::new();
    let mut amps = Vec::new();
    for p in params.as_list().ok_or_else(bad)? {
        let (t, amp) = match p.as_list() {
            Some([h, t]) if h.as_sym() == Some("&") => (t, true),
            _ => (p, false),
        };
        tys.push(ann_to_ty(
            &type_ann(g, ModuleId::PRELUDE, t, false)?,
            &mut env,
            &pos,
        )?);
        amps.push(amp);
    }
    let ret = ann_to_ty(&type_ann(g, ModuleId::PRELUDE, ret, false)?, &mut env, &pos)?;
    let mut preds = Vec::new();
    for c in read(bounds)?.first().and_then(Form::as_list).unwrap_or(&[]) {
        let items = c.as_list().ok_or_else(bad)?;
        let name = items.first().and_then(Form::as_sym).ok_or_else(bad)?;
        let args = items[1..]
            .iter()
            .map(|t| ann_to_ty(&type_ann(g, ModuleId::PRELUDE, t, false)?, &mut env, &pos))
            .collect::<TResult<Vec<_>>>()?;
        preds.push(pred_of(g, ModuleId::BUILTIN, name, args, &pos)?);
    }
    let params = (1..=tys.len()).map(|i| i.to_string()).collect();
    Ok(Scheme {
        n_vars: env.names.len() as u32,
        n_colours: env.colours,
        var_names: env.names,
        preds,
        colour_bounds: Vec::new(),
        ty: Ty::Fn(Colour::Send, tys, Box::new(ret)),
        amps,
        params,
    })
}
