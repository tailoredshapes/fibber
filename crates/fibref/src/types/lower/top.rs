//! `defun`, `def`, `extern`, `defmacro` and `impl` method bodies
//! (syntax §3.1, §3.10, §3.15, §3.16, §3.19).

use crate::syntax::{Form, FormKind, Pos};

use crate::types::annot::{ann_to_ty, AnnEnv};
use crate::types::ast::{BindingKind, Expr, ExprKind, Lit, TypeAnn};
use crate::types::decls::{FunDef, Globals, ImplMethod, ModuleId, ParamDecl, PredAnn};
use crate::types::error::{TResult, TypeError};
use crate::types::ty::{Colour, Ty};

use super::decl::annotation_name;
use super::scope::Lowerer;
use super::typeform::type_ann;

/// A placeholder body, replaced when bodies are lowered.
pub fn placeholder(g: &mut Globals, pos: &Pos) -> Expr {
    Expr {
        id: g.next_expr(),
        pos: pos.clone(),
        kind: ExprKind::Lit(Lit::Unit),
    }
}

/// A parameter of a `defun` before its binding exists.
struct RawParam<'f> {
    name: String,
    amp: bool,
    ann: Option<&'f Form>,
    borrow: bool,
    pos: Pos,
}

/// `param ::= sym | sym: type | sym :borrow | sym: type :borrow | &sym | &sym: type`.
fn raw_params(list: &[Form]) -> TResult<Vec<RawParam<'_>>> {
    let mut out: Vec<RawParam<'_>> = Vec::new();
    let mut i = 0;
    while i < list.len() {
        let f = &list[i];
        let (sym, amp) = match super::call::amp_operand(f) {
            Some(x) => (x, true),
            None => match f.as_sym() {
                Some(s) => (s, false),
                None => {
                    return Err(TypeError::resolve(
                        &f.pos,
                        format!("{f} is not a parameter"),
                    ))
                }
            },
        };
        let (name, ann) = match sym.strip_suffix(':') {
            Some(n) if !n.is_empty() => (n, list.get(i + 1)),
            _ => (sym, None),
        };
        i += if ann.is_some() { 2 } else { 1 };
        let borrow = matches!(list.get(i).map(|f| &f.kind), Some(FormKind::Kw(k)) if k == "borrow");
        if borrow {
            i += 1;
        }
        if out.iter().any(|p| p.name == name) {
            return Err(TypeError::resolve(
                &f.pos,
                format!("parameter {name} is repeated"),
            ));
        }
        out.push(RawParam {
            name: name.to_string(),
            amp,
            ann,
            borrow,
            pos: f.pos.clone(),
        });
    }
    Ok(out)
}

/// The parts of `(defun name (params) :where (..)? -> R? body+)`.
pub struct DefunParts<'f> {
    params: Vec<RawParam<'f>>,
    bounds: Option<&'f Form>,
    ret: Option<&'f Form>,
    body: &'f [Form],
}

/// Splits a `defun` form.
pub fn defun_parts(form: &Form) -> TResult<(&str, DefunParts<'_>)> {
    let items = form.as_list().unwrap_or(&[]);
    let bad = || TypeError::resolve(&form.pos, "expected (defun name (param*) body+)");
    let name = items.get(1).and_then(Form::as_sym).ok_or_else(bad)?;
    let params = items.get(2).and_then(Form::as_list).ok_or_else(bad)?;
    let mut i = 3;
    let (mut bounds, mut ret) = (None, None);
    loop {
        match items.get(i).map(|f| &f.kind) {
            Some(FormKind::Kw(k)) if k == "where" => bounds = items.get(i + 1),
            Some(FormKind::Sym(s)) if s == "->" => ret = items.get(i + 1),
            _ => break,
        }
        i += 2;
    }
    if i >= items.len() {
        return Err(TypeError::resolve(
            &form.pos,
            format!("defun {name} has no body"),
        ));
    }
    let parts = DefunParts {
        params: raw_params(params)?,
        bounds,
        ret,
        body: &items[i..],
    };
    Ok((name, parts))
}

/// Lowers a `defun` whose name is already declared.
pub fn lower_defun(g: &mut Globals, m: ModuleId, form: &Form) -> TResult<FunDef> {
    let (name, parts) = defun_parts(form)?;
    let ret = parts.ret.map(|r| type_ann(g, m, r, false)).transpose()?;
    let bounds = match parts.bounds {
        Some(b) => where_anns(g, m, b)?,
        None => Vec::new(),
    };
    let mut lw = Lowerer::new(g, m, name);
    let mut params = Vec::new();
    for p in &parts.params {
        let ann = p.ann.map(|a| type_ann(lw.g, m, a, false)).transpose()?;
        let kind = if p.amp {
            BindingKind::AmpParam
        } else {
            BindingKind::Param
        };
        let binding = lw.bind(&p.name, kind, &p.pos);
        params.push(ParamDecl {
            binding,
            name: p.name.clone(),
            amp: p.amp,
            ann,
            borrow: p.borrow,
        });
    }
    let body = lw.body(parts.body, &form.pos, false)?;
    Ok(FunDef {
        name: name.to_string(),
        module: m,
        params,
        ret,
        bounds,
        body,
        is_macro: false,
        pos: form.pos.clone(),
    })
}

/// `:where ((P T..) (Send T) ..)` as annotations.
fn where_anns(g: &Globals, m: ModuleId, cs: &Form) -> TResult<Vec<PredAnn>> {
    let list = cs.as_list().unwrap_or(&[]);
    list.iter().map(|c| where_ann(g, m, c)).collect()
}

/// `(Send T)` or `(P T₁ .. Tₙ)`, the dispatch type first.
fn where_ann(g: &Globals, m: ModuleId, c: &Form) -> TResult<PredAnn> {
    let items = c.as_list().unwrap_or(&[]);
    let Some(name) = items.first().and_then(Form::as_sym) else {
        return Err(TypeError::resolve(
            &c.pos,
            "a constraint is (P T..) or (Send T)",
        ));
    };
    let tys = items[1..]
        .iter()
        .map(|t| type_ann(g, m, t, false))
        .collect::<TResult<Vec<_>>>()?;
    if name == "Send" && tys.len() == 1 {
        return Ok(PredAnn::Send(tys[0].clone()));
    }
    let Some(p) = g.proto_name(m, name) else {
        let space = crate::types::decls::Space::Proto;
        return Err(TypeError::resolve(
            &c.pos,
            g.unknown(m, space, name, "unknown protocol"),
        ));
    };
    let want = g.proto(p).params.len();
    if want != tys.len() {
        let msg = format!("protocol {name} takes {want} type(s)");
        return Err(TypeError::resolve(&c.pos, msg));
    }
    Ok(PredAnn::Proto(p, tys))
}

/// Lowers `(defmacro name (p.. ... rest?) body+)` as a function over
/// `Form` (§2.8): each parameter `Form`, the rest `(Vec Form)`, the
/// result `Form`.
pub fn lower_macro(g: &mut Globals, m: ModuleId, form: &Form) -> TResult<FunDef> {
    let items = form.as_list().unwrap_or(&[]);
    let bad = || TypeError::resolve(&form.pos, "expected (defmacro name (param*) body+)");
    let name = items.get(1).and_then(Form::as_sym).ok_or_else(bad)?;
    let ps = items.get(2).and_then(Form::as_list).ok_or_else(bad)?;
    if items.len() < 4 {
        return Err(bad());
    }
    let form_ann = form_type(g, &form.pos)?;
    let vec_form = match g.vec {
        Some(v) => TypeAnn::Nominal(v, vec![form_ann.clone()]),
        None => return Err(TypeError::resolve(&form.pos, "the prelude defines no Vec")),
    };
    let mut lw = Lowerer::new(g, m, name);
    let mut params = Vec::new();
    let mut rest = false;
    for p in ps {
        let Some(n) = p.as_sym() else {
            return Err(bad());
        };
        if n == "..." {
            rest = true;
            continue;
        }
        let ann = if rest {
            vec_form.clone()
        } else {
            form_ann.clone()
        };
        let binding = lw.bind(n, BindingKind::Param, &p.pos);
        params.push(ParamDecl {
            binding,
            name: n.to_string(),
            amp: false,
            ann: Some(ann),
            borrow: false,
        });
    }
    let body = lw.body(&items[3..], &form.pos, false)?;
    Ok(FunDef {
        name: name.to_string(),
        module: m,
        params,
        ret: Some(form_ann),
        bounds: Vec::new(),
        body,
        is_macro: true,
        pos: form.pos.clone(),
    })
}

fn form_type(g: &Globals, pos: &Pos) -> TResult<TypeAnn> {
    match g.form {
        Some(f) => Ok(TypeAnn::Nominal(f, Vec::new())),
        None => Err(TypeError::resolve(pos, "Form is not declared")),
    }
}

/// `(def name expr)` / `(def name: type expr)`: the name, annotation and
/// initialiser form.
pub fn def_parts(form: &Form) -> TResult<(&str, Option<&Form>, &Form)> {
    let items = form.as_list().unwrap_or(&[]);
    match items {
        [_, n, e] => match n.as_sym() {
            Some(name) if annotation_name(n).is_none() => Ok((name, None, e)),
            _ => Err(TypeError::resolve(&form.pos, "expected (def name expr)")),
        },
        [_, n, t, e] => match annotation_name(n) {
            Some(name) => Ok((name, Some(t), e)),
            None => Err(TypeError::resolve(
                &form.pos,
                "expected (def name: type expr)",
            )),
        },
        _ => Err(TypeError::resolve(&form.pos, "expected (def name expr)")),
    }
}

/// Extern types are closed: a variable is an error, colours are `send`.
struct ClosedEnv;

impl AnnEnv for ClosedEnv {
    fn var(&mut self, name: &str, pos: &Pos) -> TResult<Ty> {
        Err(TypeError::resolve(pos, format!("unknown type {name}")))
    }
    fn colour(&mut self) -> Colour {
        Colour::Send
    }
}

/// `(extern c (T̄) -> R opts)`: its type, checking that every position
/// is a scalar, `ptr` or `unit` (§3.15).
pub fn extern_type(g: &Globals, m: ModuleId, form: &Form) -> TResult<(String, Ty, bool)> {
    let items = form.as_list().unwrap_or(&[]);
    let bad = || TypeError::resolve(&form.pos, "expected (extern name (type*) -> type opt*)");
    let name = items.get(1).and_then(Form::as_sym).ok_or_else(bad)?;
    let ps = items.get(2).and_then(Form::as_list).ok_or_else(bad)?;
    if items.get(3).and_then(Form::as_sym) != Some("->") {
        return Err(bad());
    }
    let ret_form = items.get(4).ok_or_else(bad)?;
    let varargs = items[5..]
        .iter()
        .any(|o| matches!(&o.kind, FormKind::Kw(k) if k == "varargs"));
    let scalar = |f: &Form| -> TResult<Ty> {
        match type_ann(g, m, f, false)? {
            TypeAnn::Scalar(s) => ann_to_ty(&TypeAnn::Scalar(s), &mut ClosedEnv, &f.pos),
            _ => Err(TypeError::other(
                &f.pos,
                "extern positions are scalars, ptr or unit",
            )),
        }
    };
    let params = ps.iter().map(scalar).collect::<TResult<Vec<_>>>()?;
    let ret = scalar(ret_form)?;
    Ok((
        name.to_string(),
        Ty::Fn(Colour::Send, params, Box::new(ret)),
        varargs,
    ))
}

/// Lowers one `impl` method body `(mname (self x..) ret? body+)`.
pub fn lower_impl_method(
    g: &mut Globals,
    m: ModuleId,
    inst: usize,
    mf: &Form,
    owner: &str,
) -> TResult<ImplMethod> {
    let items = mf.as_list().unwrap_or(&[]);
    let bad = || TypeError::resolve(&mf.pos, "a method is (name (self x*) body+)");
    let name = items.first().and_then(Form::as_sym).ok_or_else(bad)?;
    let ps = items.get(1).and_then(Form::as_list).ok_or_else(bad)?;
    let proto = g.proto(g.instances[inst].proto);
    let Some(index) = proto.methods.iter().position(|md| md.name == name) else {
        let msg = format!("{name} is not a method of {}", proto.name);
        return Err(TypeError::other(&mf.pos, msg));
    };
    let want = proto.methods[index].params.len();
    if ps.len() != want || ps.first().and_then(Form::as_sym) != Some("self") {
        let msg = format!("method {name} takes (self ..) with {want} parameter(s)");
        return Err(TypeError::other(&mf.pos, msg));
    }
    let mut lw = Lowerer::new(g, m, owner);
    let (ret, start) = lw.ret_annotation(items, 2)?;
    let mut params = Vec::new();
    for p in ps {
        let Some(n) = p.as_sym() else {
            return Err(bad());
        };
        params.push(lw.bind(n, BindingKind::Param, &p.pos));
    }
    if start >= items.len() {
        return Err(bad());
    }
    let body = lw.body(&items[start..], &mf.pos, false)?;
    Ok(ImplMethod {
        index,
        params,
        ret,
        body,
        pos: mf.pos.clone(),
    })
}
