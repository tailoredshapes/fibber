//! Lowering one module's expanded forms to the resolved core AST
//! (types §3.5 steps 2–3): declare every name first (so definitions may
//! refer to each other in any order, syntax §3.1), resolve the field
//! types, then the protocol signatures and `impl` heads (instances with
//! their declared contexts, before any body), then every body.

mod bindings;
mod call;
mod clause;
mod decl;
mod defaults;
mod expr;
mod impl_head;
mod pattern;
mod private;
mod protos;
mod scope;
mod supers;
mod top;
mod typeform;

use crate::syntax::Form;

use crate::types::ast::{DefId, Expr, ExprKind, FunId, GlobalRef};
use crate::types::decls::{DefDef, ExternDef, FunDef, Globals, InstanceDef, ModuleId, Shape};
use crate::types::error::{ErrorKind, TResult, TypeError};
use crate::types::ty::{Con, ProtoId, Ty};

pub use decl::define_value;
pub use impl_head::{body_args, fixed_colours};
pub use private::{mark_private, marker_index, strip_private};
pub use protos::{declare_proto, pred_of, register_instance, resolve_proto};
pub use scope::Lowerer;
pub use supers::{entail_closure, super_closure};
pub use typeform::type_ann;

use decl::{declare_enum, declare_struct, resolve_shape, RawType};

/// What a module defines, in source order, for the per-module
/// algorithm.
#[derive(Clone, Debug, Default)]
pub struct ModuleItems {
    /// Its `defun`s.
    pub funs: Vec<FunId>,
    /// Its `def`s.
    pub defs: Vec<DefId>,
    /// Its `impl`s, as instance indices.
    pub impls: Vec<usize>,
    /// Its `defmacro`s (functions over `Form`, not values).
    pub macros: Vec<FunId>,
}

/// A module whose names are declared and whose bodies are not lowered.
pub struct Declared<'f> {
    m: ModuleId,
    protos: Vec<(ProtoId, &'f Form)>,
    funs: Vec<(FunId, &'f Form)>,
    defs: Vec<(DefId, &'f Form)>,
    macros: Vec<&'f Form>,
    impls: Vec<&'f Form>,
}

/// Steps 2–3 up to the field types: declares every name of `forms` in
/// module `m`.
pub fn declare<'f>(
    g: &mut Globals,
    m: ModuleId,
    forms: &'f [Form],
) -> Result<Declared<'f>, Vec<TypeError>> {
    let mut d = Declared {
        m,
        protos: Vec::new(),
        funs: Vec::new(),
        defs: Vec::new(),
        macros: Vec::new(),
        impls: Vec::new(),
    };
    let mut raw = Vec::new();
    let mut errors = Vec::new();
    for form in forms {
        if let Err(e) = declare_one(g, &mut d, &mut raw, form) {
            errors.push(e);
        }
    }
    for r in &raw {
        if let Err(e) = resolve_shape(g, m, r) {
            errors.push(e);
        }
    }
    for r in &raw {
        if let Err(e) = scalar_enum_instances(g, r.id) {
            errors.push(e);
        }
    }
    if errors.is_empty() {
        Ok(d)
    } else {
        Err(errors)
    }
}

fn declare_one<'f>(
    g: &mut Globals,
    d: &mut Declared<'f>,
    raw: &mut Vec<RawType<'f>>,
    form: &'f Form,
) -> TResult<()> {
    let m = d.m;
    let head = form
        .as_list()
        .and_then(|l| l.first())
        .and_then(Form::as_sym)
        .unwrap_or("");
    match head {
        "ns" => ns_form(form)?,
        "defstruct" => raw.push(declare_struct(g, m, form)?),
        "defenum" => raw.push(declare_enum(g, m, form)?),
        "defprotocol" => d.protos.push((declare_proto(g, m, form)?, form)),
        "defun" => d.funs.push((declare_fun(g, m, form)?, form)),
        "def" => d.defs.push((declare_def(g, m, form)?, form)),
        "extern" => declare_extern(g, m, form)?,
        "defmacro" => d.macros.push(form),
        "impl" => d.impls.push(form),
        _ => {
            let msg = format!("expected a definition, found {form}");
            return Err(TypeError::resolve(&form.pos, msg));
        }
    }
    Ok(())
}

/// Declares a `defun`'s name; its body is lowered later.
fn declare_fun(g: &mut Globals, m: ModuleId, form: &Form) -> TResult<FunId> {
    let (name, _) = top::defun_parts(form)?;
    let id = FunId(g.funs.len() as u32);
    let body = top::placeholder(g, &form.pos);
    g.funs.push(FunDef {
        name: name.to_string(),
        module: m,
        params: Vec::new(),
        ret: None,
        bounds: Vec::new(),
        body,
        is_macro: false,
        pos: form.pos.clone(),
    });
    define_value(g, m, name, GlobalRef::Fun(id), &form.pos)?;
    Ok(id)
}

/// Declares a `def`'s name; its initialiser is lowered later.
fn declare_def(g: &mut Globals, m: ModuleId, form: &Form) -> TResult<DefId> {
    let (name, _, _) = top::def_parts(form)?;
    let id = DefId(g.defs.len() as u32);
    let init = top::placeholder(g, &form.pos);
    g.defs.push(DefDef {
        name: name.to_string(),
        module: m,
        ann: None,
        init,
        pos: form.pos.clone(),
    });
    define_value(g, m, name, GlobalRef::Def(id), &form.pos)?;
    Ok(id)
}

/// Declares an `extern` with its type.
fn declare_extern(g: &mut Globals, m: ModuleId, form: &Form) -> TResult<()> {
    let (name, ty, varargs) = top::extern_type(g, m, form)?;
    let id = crate::types::ast::ExternId(g.externs.len() as u32);
    g.externs.push(ExternDef {
        name: name.clone(),
        ty,
        varargs,
        pos: form.pos.clone(),
    });
    define_value(g, m, &name, GlobalRef::Extern(id), &form.pos)
}

/// `(ns name)`: one module plus the prelude is all this checker
/// supports (syntax §5 kept minimal); a `:require` or `:use` is an error.
fn ns_form(_form: &Form) -> TResult<()> {
    // Its clauses were read by the loader (`crate::modules`), which
    // gave the module its chain and aliases.
    Ok(())
}

/// A field-less enum is a scalar with built-in `Eq`, `Ord`, `Hash` and
/// `Show` (§2.12).
fn scalar_enum_instances(g: &mut Globals, id: crate::types::ty::TypeId) -> TResult<()> {
    if !g.ty(id).is_fieldless_enum() {
        return Ok(());
    }
    let def = g.ty(id);
    let (names, pos) = (def.params.clone(), def.pos.clone());
    let head = Ty::Con(
        Con::Nominal(id),
        (0..names.len() as u32).map(Ty::Gen).collect(),
    );
    for p in ["Eq", "Ord", "Hash", "Show"] {
        let Some(proto) = g.proto_name(ModuleId::BUILTIN, p) else {
            continue;
        };
        let inst = InstanceDef {
            proto,
            con: Con::Nominal(id),
            module: ModuleId::BUILTIN,
            var_names: names.clone(),
            head: head.clone(),
            dets: Vec::new(),
            context: Vec::new(),
            methods: Vec::new(),
            pos: pos.clone(),
        };
        register_instance(g, inst)?;
    }
    Ok(())
}

/// Steps 3 (protocols, impls) and the bodies: lowers a declared module.
pub fn define(g: &mut Globals, d: Declared<'_>) -> Result<ModuleItems, Vec<TypeError>> {
    let m = d.m;
    let mut errors = Vec::new();
    let mut keep = |r: TResult<()>| {
        if let Err(e) = r {
            errors.push(e);
        }
    };
    for (id, form) in &d.protos {
        keep(resolve_proto(g, m, *id, form));
    }
    let mut items = ModuleItems::default();
    let mut impls = Vec::new();
    for form in &d.impls {
        match protos::declare_impl(g, m, form) {
            Ok(x) => impls.push(x),
            Err(e) => keep(Err(e)),
        }
    }
    for (id, form) in &d.funs {
        keep(top::lower_defun(g, m, form).map(|f| g.funs[id.0 as usize] = f));
        items.funs.push(*id);
    }
    for (id, form) in &d.defs {
        keep(lower_def(g, m, *id, form));
        items.defs.push(*id);
    }
    for (inst, _) in &impls {
        keep(supers::check_impl_supers(g, *inst));
    }
    for (inst, methods) in impls {
        keep(lower_impl(g, m, inst, methods));
        items.impls.push(inst);
    }
    for form in &d.macros {
        keep(top::lower_macro(g, m, form).map(|f| {
            items.macros.push(FunId(g.funs.len() as u32));
            g.funs.push(f);
        }));
    }
    if errors.is_empty() {
        Ok(items)
    } else {
        Err(errors)
    }
}

fn lower_impl(g: &mut Globals, m: ModuleId, inst: usize, methods: &[Form]) -> TResult<()> {
    let owner = impl_owner(g, &g.instances[inst]);
    let mut out: Vec<crate::types::decls::ImplMethod> = Vec::new();
    for mf in methods {
        let method = top::lower_impl_method(g, m, inst, mf, &owner)?;
        if out.iter().any(|o| o.index == method.index) {
            return Err(TypeError::other(
                &mf.pos,
                format!("{owner} defines a method twice"),
            ));
        }
        out.push(method);
    }
    let proto = g.proto(g.instances[inst].proto).clone();
    for (i, md) in proto.methods.iter().enumerate() {
        if out.iter().any(|o| o.index == i) {
            continue;
        }
        let Some(default) = &md.default else {
            let msg = format!("{owner} is missing the method {}", md.name);
            return Err(TypeError::other(&g.instances[inst].pos, msg));
        };
        // The default, specialised to this instance and resolved where
        // the protocol is defined (types §4.1).
        let form = defaults::method_form(md, default);
        out.push(top::lower_impl_method(
            g,
            proto.module,
            inst,
            &form,
            &owner,
        )?);
    }
    g.instances[inst].methods = out;
    Ok(())
}

/// `impl P for T`, for messages.
fn impl_owner(g: &Globals, inst: &InstanceDef) -> String {
    let names: Vec<String> = inst.var_names.clone();
    let head = inst.head.map_leaves(&mut |t| match t {
        Ty::Gen(i) => Some(Ty::Rigid(*i)),
        _ => None,
    });
    let text = crate::types::display::Printer::with_names(g, &[], &names).ty(&head);
    format!("impl {} for {text}", g.proto(inst.proto).name)
}

fn lower_def(g: &mut Globals, m: ModuleId, id: DefId, form: &Form) -> TResult<()> {
    let (name, ann, init) = top::def_parts(form)?;
    let ann = ann.map(|a| type_ann(g, m, a, false)).transpose()?;
    let mut lw = Lowerer::new(g, m, name);
    let init = lw.expr(init, false)?;
    check_const(g, id, &init)?;
    let def = &mut g.defs[id.0 as usize];
    def.ann = ann;
    def.init = init;
    Ok(())
}

/// The constant grammar of syntax §3.19.
fn check_const(g: &Globals, id: DefId, e: &Expr) -> TResult<()> {
    let ok = match &e.kind {
        ExprKind::Lit(_) | ExprKind::Quote(_) => true,
        ExprKind::Global(GlobalRef::Def(d)) => d.0 < id.0,
        ExprKind::Global(GlobalRef::Ctor(..)) | ExprKind::Global(GlobalRef::Fun(_)) => true,
        ExprKind::Call(head, args) => {
            const_head(g, head)
                && args.iter().all(|a| match a {
                    crate::types::ast::Arg::Expr(a) => check_const(g, id, a).is_ok(),
                    crate::types::ast::Arg::Amp(..) => false,
                })
        }
        _ => false,
    };
    if ok {
        return Ok(());
    }
    let name = &g.def(id).name;
    let msg = format!("def {name}: initialiser is not a constant expression");
    Err(TypeError::new(ErrorKind::DefNotConstant, &e.pos, msg))
}

/// A constructor, or one of the prelude calls of the literal rewrite.
fn const_head(g: &Globals, head: &Expr) -> bool {
    let ExprKind::Global(r) = &head.kind else {
        return false;
    };
    if let GlobalRef::Ctor(id, variant) = r {
        return match (&g.ty(*id).shape, variant) {
            (Shape::Struct(_), None) => true,
            (Shape::Enum(vs), Some(i)) => vs.get(*i).is_some_and(|v| !v.fields.is_empty()),
            _ => false,
        };
    }
    ["vec-empty", "vec-conj", "map-empty", "map-assoc"]
        .iter()
        .any(|n| g.value(ModuleId::PRELUDE, n) == Some(*r))
}
