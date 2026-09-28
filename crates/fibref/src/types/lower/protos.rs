//! `defprotocol` and the heads of `impl`s (syntax §3.10; types §2.7,
//! §4.1): method signatures as schemes, instances keyed by `(P, head)`
//! with their declared contexts, coherence and the Paterson condition.

use crate::syntax::{Form, FormKind, Pos};

use crate::types::annot::{ann_to_ty, GenEnv, ParamEnv};
use crate::types::ast::{GlobalRef, TypeAnn};
use crate::types::decls::{Globals, InstanceDef, MethodDef, MethodParam, ModuleId, ProtoDef};
use crate::types::error::{TResult, TypeError};
use crate::types::scheme::Scheme;
use crate::types::ty::{Colour, Con, Leaf, Pred, ProtoId, Ty};

use super::decl::{annotation_name, define_value};
use super::supers::{methods_start, resolve_supers};
use super::typeform::{proto_ref, type_ann};

/// `Name` (dispatch parameter `Self`) or `(Name self det*)`.
fn proto_head(head: &Form) -> TResult<(String, Vec<String>)> {
    if let FormKind::Sym(n) = &head.kind {
        return Ok((n.clone(), vec!["Self".to_string()]));
    }
    let names: Option<Vec<&str>> = head
        .as_list()
        .map(|h| h.iter().filter_map(Form::as_sym).collect());
    match (names, head.as_list()) {
        (Some(ns), Some(h)) if !ns.is_empty() && ns.len() == h.len() => {
            let mut params: Vec<String> = ns[1..].iter().map(|s| s.to_string()).collect();
            if params.is_empty() {
                params.push("Self".to_string());
            }
            Ok((ns[0].to_string(), params))
        }
        _ => Err(TypeError::resolve(
            &head.pos,
            "expected Name or (Name self det*)",
        )),
    }
}

/// Declares a protocol's name, parameters and method names.
pub fn declare_proto(g: &mut Globals, m: ModuleId, form: &Form) -> TResult<ProtoId> {
    let items = form.as_list().unwrap_or(&[]);
    if items.len() <= methods_start(items) {
        return Err(TypeError::resolve(
            &form.pos,
            "expected (defprotocol Name :requires (P..)? method+)",
        ));
    }
    let (name, params) = proto_head(&items[1])?;
    if g.names(m).protos.contains_key(&name) {
        let msg = format!("protocol {name} is already defined");
        return Err(TypeError::resolve(&form.pos, msg));
    }
    let id = ProtoId(g.protos.len() as u32);
    let mut methods = Vec::new();
    for (i, mf) in items[methods_start(items)..].iter().enumerate() {
        let mname = mf.as_list().and_then(|l| l.first()).and_then(Form::as_sym);
        let Some(mname) = mname else {
            return Err(TypeError::resolve(
                &mf.pos,
                "a method is (name (self ..) -> type)",
            ));
        };
        define_value(g, m, mname, GlobalRef::Method(id, i), &mf.pos)?;
        methods.push(MethodDef {
            name: mname.to_string(),
            params: Vec::new(),
            scheme: Scheme::mono(Ty::unit()),
            self_elsewhere: false,
            default: None,
            pos: mf.pos.clone(),
        });
    }
    let pos = form.pos.clone();
    g.protos.push(ProtoDef {
        name: name.clone(),
        module: m,
        params,
        methods,
        supers: Vec::new(),
        pos,
    });
    g.names_mut(m).protos.insert(name, id);
    Ok(id)
}

/// Resolves the method signatures of a declared protocol.
pub fn resolve_proto(g: &mut Globals, m: ModuleId, id: ProtoId, form: &Form) -> TResult<()> {
    let items = form.as_list().unwrap_or(&[]);
    resolve_supers(g, m, id, form)?;
    for (i, mf) in items[methods_start(items)..].iter().enumerate() {
        let method = method_sig(g, m, id, mf)?;
        g.protos[id.0 as usize].methods[i] = method;
    }
    Ok(())
}

/// `(mname (self qual* mparam*) -> type)`, `mparam ::= sym: type qual*`.
fn method_sig(g: &Globals, m: ModuleId, id: ProtoId, mf: &Form) -> TResult<MethodDef> {
    let items = mf.as_list().unwrap_or(&[]);
    let bad = || {
        TypeError::resolve(
            &mf.pos,
            "a method is (name (self qual* x: T qual*) -> type)",
        )
    };
    let [name, params, arrow, ret, body @ ..] = items else {
        return Err(bad());
    };
    let ps = params.as_list().ok_or_else(bad)?;
    if arrow.as_sym() != Some("->") || ps.first().and_then(Form::as_sym) != Some("self") {
        return Err(bad());
    }
    let proto = g.proto(id);
    let mut env = GenEnv::with_names(proto.params.clone());
    env.self_gen = true;
    let (mparams, tys) = method_params(g, m, ps, &proto.params[0], &mut env).ok_or_else(bad)??;
    let ret_ann = rename_dispatch(&type_ann(g, m, ret, true)?, &proto.params[0]);
    let ret = ann_to_ty(&ret_ann, &mut env, &ret.pos)?;
    let self_elsewhere = tys[1..]
        .iter()
        .chain(std::iter::once(&ret))
        .any(mentions_gen0);
    let dispatch = (0..proto.params.len() as u32).map(Ty::Gen).collect();
    let scheme = Scheme {
        n_vars: env.names.len() as u32,
        n_colours: env.colours,
        var_names: env.names.clone(),
        preds: vec![Pred::Proto(id, dispatch)],
        colour_bounds: Vec::new(),
        amps: vec![false; tys.len()],
        params: mparams.iter().map(|p| p.name.clone()).collect(),
        ty: Ty::Fn(Colour::Send, tys, Box::new(ret)),
    };
    let name = name.as_sym().unwrap_or("").to_string();
    Ok(MethodDef {
        name,
        params: mparams,
        scheme,
        self_elsewhere,
        default: (!body.is_empty()).then(|| mf.clone()),
        pos: mf.pos.clone(),
    })
}

/// The parameters of a method signature, `self` first: their declared
/// kinds and their types. `None` when the list is malformed.
fn method_params(
    g: &Globals,
    m: ModuleId,
    ps: &[Form],
    dispatch: &str,
    env: &mut GenEnv,
) -> Option<TResult<(Vec<MethodParam>, Vec<Ty>)>> {
    let plain = |name: &str| MethodParam {
        name: name.to_string(),
        borrow: false,
        owned: false,
    };
    let mut mparams = vec![plain("self")];
    let mut tys = vec![Ty::Gen(0)];
    let mut i = quals(ps, 1, &mut mparams[0]);
    while i < ps.len() {
        let pname = annotation_name(&ps[i])?;
        let t = ps.get(i + 1)?;
        let ty = type_ann(g, m, t, true)
            .and_then(|ann| ann_to_ty(&rename_dispatch(&ann, dispatch), env, &t.pos));
        match ty {
            Ok(ty) => tys.push(ty),
            Err(e) => return Some(Err(e)),
        }
        let mut p = plain(pname);
        i = quals(ps, i + 2, &mut p);
        mparams.push(p);
    }
    Some(Ok((mparams, tys)))
}

/// The dispatch parameter's own name (`s` in `(Seq s e)`) means `Self`.
fn rename_dispatch(ann: &TypeAnn, dispatch: &str) -> TypeAnn {
    match ann {
        TypeAnn::Var(n) if n == dispatch => TypeAnn::SelfTy,
        TypeAnn::Builtin(c, a) => TypeAnn::Builtin(*c, Box::new(rename_dispatch(a, dispatch))),
        TypeAnn::Nominal(id, args) => TypeAnn::Nominal(
            *id,
            args.iter().map(|a| rename_dispatch(a, dispatch)).collect(),
        ),
        TypeAnn::Dyn(p, args, send) => TypeAnn::Dyn(
            *p,
            args.iter().map(|a| rename_dispatch(a, dispatch)).collect(),
            *send,
        ),
        TypeAnn::Fn(k, ps, r) => TypeAnn::Fn(
            *k,
            ps.iter().map(|a| rename_dispatch(a, dispatch)).collect(),
            Box::new(rename_dispatch(r, dispatch)),
        ),
        other => other.clone(),
    }
}

fn mentions_gen0(t: &Ty) -> bool {
    let mut found = false;
    t.visit(&mut |l| found |= l == Leaf::Gen(0));
    found
}

/// Reads `:borrow`/`:owned` qualifiers from `i`; returns the index after.
fn quals(ps: &[Form], mut i: usize, p: &mut MethodParam) -> usize {
    while let Some(FormKind::Kw(k)) = ps.get(i).map(|f| &f.kind) {
        match k.as_str() {
            "borrow" => p.borrow = true,
            "owned" => p.owned = true,
            _ => break,
        }
        i += 1;
    }
    i
}

/// The instance head `T` of an `impl`: its constructor, its variables.
fn impl_head(g: &Globals, m: ModuleId, form: &Form) -> TResult<(Con, Vec<String>)> {
    let bad = |msg: &str| Err(TypeError::other(&form.pos, msg.to_string()));
    let ann = type_ann(g, m, form, false)?;
    let (con, args) = match ann {
        TypeAnn::Var(_) => return bad("an instance head must not be a type variable"),
        TypeAnn::Scalar(s) => (Con::Scalar(s), Vec::new()),
        TypeAnn::Str => (Con::Str, Vec::new()),
        TypeAnn::Builtin(c, a) => (c, vec![*a]),
        TypeAnn::Nominal(id, args) => (Con::Nominal(id), args),
        _ => return bad("an instance head is a type constructor applied to distinct variables"),
    };
    let mut vars: Vec<String> = Vec::new();
    for a in args {
        match a {
            TypeAnn::Var(v) if !vars.contains(&v) => vars.push(v),
            _ => {
                return bad("an instance head is a type constructor applied to distinct variables")
            }
        }
    }
    Ok((con, vars))
}

/// Declares the instance of an `impl` form; returns its index and the
/// method forms.
pub fn declare_impl<'f>(
    g: &mut Globals,
    m: ModuleId,
    form: &'f Form,
) -> TResult<(usize, &'f [Form])> {
    let items = form.as_list().unwrap_or(&[]);
    if items.len() < 3 {
        return Err(TypeError::resolve(&form.pos, "expected (impl P T method*)"));
    }
    let (proto, det_anns) = proto_ref(g, m, &items[1], false)?;
    let (con, vars) = impl_head(g, m, &items[2])?;
    let mut rest = &items[3..];
    let mut context = Vec::new();
    if let Some(FormKind::Kw(k)) = rest.first().map(|f| &f.kind) {
        if k == "where" {
            let Some(cs) = rest.get(1) else {
                return Err(TypeError::resolve(&form.pos, ":where without constraints"));
            };
            context = where_preds(g, m, cs, &vars)?;
            rest = &rest[2..];
        }
    }
    let mut env = ParamEnv {
        params: &vars,
        owner: "the impl head",
    };
    let dets = det_anns
        .iter()
        .map(|d| ann_to_ty(d, &mut env, &items[1].pos))
        .collect::<TResult<Vec<_>>>()?;
    let head = Ty::Con(con, (0..vars.len() as u32).map(Ty::Gen).collect());
    paterson(g, &context, &head, &form.pos)?;
    let inst = InstanceDef {
        proto,
        con,
        module: m,
        var_names: vars,
        head,
        dets,
        context,
        methods: Vec::new(),
        pos: form.pos.clone(),
    };
    let index = register_instance(g, inst)?;
    Ok((index, rest))
}

/// Registers an instance, checking coherence (§4.1).
pub fn register_instance(g: &mut Globals, inst: InstanceDef) -> TResult<usize> {
    let key = (inst.proto, inst.con);
    if let Some(i) = g.instance_index.get(&key) {
        let other = &g.instances[*i];
        let msg = format!(
            "overlapping instances: {} for {} is already implemented at {}",
            g.proto(inst.proto).name,
            crate::types::display::Printer::new(g).ty(&strip(&inst.head, &inst.var_names)),
            other.pos
        );
        return Err(TypeError::other(&inst.pos, msg));
    }
    g.instances.push(inst);
    g.instance_index.insert(key, g.instances.len() - 1);
    Ok(g.instances.len() - 1)
}

fn strip(head: &Ty, names: &[String]) -> Ty {
    head.map_leaves(&mut |t| match t {
        Ty::Gen(i) => Some(Ty::Rigid(*i)).filter(|_| (*i as usize) < names.len()),
        _ => None,
    })
}

/// The constraints of a `:where` clause: `(P T..)` or `(Send T)`, over
/// the variables `vars` only.
pub fn where_preds(g: &Globals, m: ModuleId, cs: &Form, vars: &[String]) -> TResult<Vec<Pred>> {
    let Some(list) = cs.as_list() else {
        return Err(TypeError::resolve(
            &cs.pos,
            ":where takes a list of constraints",
        ));
    };
    let mut out = Vec::new();
    for c in list {
        let items = c.as_list().unwrap_or(&[]);
        let Some(pname) = items.first().and_then(Form::as_sym) else {
            return Err(TypeError::resolve(
                &c.pos,
                "a constraint is (P T..) or (Send T)",
            ));
        };
        let mut env = ParamEnv {
            params: vars,
            owner: "the impl head",
        };
        let tys = items[1..]
            .iter()
            .map(|t| ann_to_ty(&type_ann(g, m, t, false)?, &mut env, &t.pos))
            .collect::<TResult<Vec<_>>>()?;
        out.push(pred_of(g, m, pname, tys, &c.pos)?);
    }
    Ok(out)
}

/// `(P T..)`, `(Send T)`, `(Object T)` or `(Weakable T)` from its parts.
pub fn pred_of(g: &Globals, m: ModuleId, name: &str, mut tys: Vec<Ty>, pos: &Pos) -> TResult<Pred> {
    match (name, tys.len()) {
        ("Send", 1) => return Ok(Pred::Send(tys.remove(0))),
        ("Object", 1) => return Ok(Pred::Object(tys.remove(0))),
        ("Weakable", 1) => return Ok(Pred::Weakable(tys.remove(0))),
        _ => {}
    }
    let Some(p) = g.proto_name(m, name) else {
        let msg = g.unknown(
            m,
            crate::types::decls::Space::Proto,
            name,
            "unknown protocol",
        );
        return Err(TypeError::resolve(pos, msg));
    };
    if g.proto(p).params.len() != tys.len() {
        return Err(TypeError::resolve(
            pos,
            format!("protocol {name} takes {} type(s)", g.proto(p).params.len()),
        ));
    }
    Ok(Pred::Proto(p, tys))
}

/// The Paterson condition (§3.3): each context constraint has no
/// variable more often than the head and is smaller than the head.
fn paterson(g: &Globals, context: &[Pred], head: &Ty, pos: &Pos) -> TResult<()> {
    let (head_size, head_vars) = size(&[head]);
    for c in context {
        let (s, vars) = size(&c.tys());
        let too_many = vars.iter().any(|(v, n)| {
            head_vars
                .iter()
                .find(|(w, _)| w == v)
                .is_none_or(|(_, m)| n > m)
        });
        if too_many || s >= head_size {
            let msg = format!(
                "the context constraint {} is not smaller than the instance head",
                crate::types::display::Printer::new(g).pred(c)
            );
            return Err(TypeError::other(pos, msg));
        }
    }
    Ok(())
}

/// Constructors plus variables, and the count of each variable.
fn size(tys: &[&Ty]) -> (usize, Vec<(u32, usize)>) {
    let mut n = 0;
    let mut vars: Vec<(u32, usize)> = Vec::new();
    fn walk(t: &Ty, n: &mut usize, vars: &mut Vec<(u32, usize)>) {
        *n += 1;
        match t {
            Ty::Gen(i) => match vars.iter_mut().find(|(v, _)| v == i) {
                Some((_, c)) => *c += 1,
                None => vars.push((*i, 1)),
            },
            Ty::Con(_, args) => args.iter().for_each(|a| walk(a, n, vars)),
            Ty::Fn(_, ps, r) => {
                ps.iter().for_each(|a| walk(a, n, vars));
                walk(r, n, vars);
            }
            _ => {}
        }
    }
    tys.iter().for_each(|t| walk(t, &mut n, &mut vars));
    (n, vars)
}
