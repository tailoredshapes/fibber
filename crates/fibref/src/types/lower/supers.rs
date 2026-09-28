//! Supertraits (types §4.1, owner's decision of 2026-09-28): the
//! `:requires` list of a protocol, the constraints a given entails
//! through it, and the check that an `impl` has its supertraits' impls
//! with contexts its own context entails.

use crate::syntax::{Form, FormKind, Pos};

use crate::types::annot::{ann_to_ty, AnnEnv};
use crate::types::decls::{Globals, InstanceDef, ModuleId, Space};
use crate::types::display::Printer;
use crate::types::error::{TResult, TypeError};
use crate::types::ty::{Colour, Pred, ProtoId, Ty};

use super::typeform::type_ann;

/// Where a `defprotocol`'s methods start: after `Name`, and after
/// `:requires (..)` when present.
pub fn methods_start(items: &[Form]) -> usize {
    match items.get(2).map(|f| &f.kind) {
        Some(FormKind::Kw(k)) if k == "requires" => 4,
        _ => 2,
    }
}

/// The protocol's own parameters as `Gen(i)`, `Self` as `Gen(0)`;
/// nothing else may occur in a supertrait.
struct SuperEnv<'a> {
    params: &'a [String],
}

impl AnnEnv for SuperEnv<'_> {
    fn var(&mut self, name: &str, pos: &Pos) -> TResult<Ty> {
        match self.params.iter().position(|p| p == name) {
            Some(i) => Ok(Ty::Gen(i as u32)),
            None => Err(TypeError::resolve(
                pos,
                format!("a supertrait may mention only the protocol's parameters, not {name}"),
            )),
        }
    }
    fn self_ty(&mut self, _: &Pos) -> TResult<Ty> {
        Ok(Ty::Gen(0))
    }
    fn colour(&mut self) -> Colour {
        Colour::Local
    }
}

/// One entry of `:requires`: `Q` for `(Q Self)`, or `(Q s t..)`.
fn super_pred(g: &Globals, m: ModuleId, params: &[String], f: &Form) -> TResult<Pred> {
    let items = f.as_list().unwrap_or(&[]);
    let name = f
        .as_sym()
        .or_else(|| items.first().and_then(Form::as_sym))
        .unwrap_or("");
    let Some(q) = g.proto_name(m, name) else {
        let msg = g.unknown(m, Space::Proto, name, "unknown protocol");
        return Err(TypeError::resolve(&f.pos, msg));
    };
    let want = g.proto(q).params.len();
    let args = if f.as_sym().is_some() {
        vec![Ty::Gen(0)]
    } else {
        let mut env = SuperEnv { params };
        items[1..]
            .iter()
            .map(|t| ann_to_ty(&type_ann(g, m, t, true)?, &mut env, &t.pos))
            .collect::<TResult<Vec<_>>>()?
    };
    if args.len() != want || args.first() != Some(&Ty::Gen(0)) {
        let msg = format!(
            "a supertrait is {name} or ({name} s ..) with {want} type(s), the first the protocol's own dispatch parameter"
        );
        return Err(TypeError::resolve(&f.pos, msg));
    }
    Ok(Pred::Proto(q, args))
}

/// Reads `:requires` of protocol `id` and records its supertraits;
/// one that reaches the protocol again is an error.
pub fn resolve_supers(g: &mut Globals, m: ModuleId, id: ProtoId, form: &Form) -> TResult<()> {
    let items = form.as_list().unwrap_or(&[]);
    if methods_start(items) == 2 {
        return Ok(());
    }
    let Some(list) = items.get(3).and_then(Form::as_list) else {
        return Err(TypeError::resolve(
            &form.pos,
            ":requires takes a list of protocols",
        ));
    };
    let params = g.proto(id).params.clone();
    let supers = list
        .iter()
        .map(|f| super_pred(g, m, &params, f))
        .collect::<TResult<Vec<_>>>()?;
    g.protos[id.0 as usize].supers = supers;
    let own = Pred::Proto(id, (0..params.len() as u32).map(Ty::Gen).collect());
    if super_closure(g, &own)
        .iter()
        .any(|p| matches!(p, Pred::Proto(q, _) if *q == id))
    {
        let msg = format!("protocol {} requires itself", g.proto(id).name);
        return Err(TypeError::resolve(&form.pos, msg));
    }
    Ok(())
}

/// Every supertrait constraint `p` entails, transitively (§4.1 rule 2);
/// `p` itself only through a cycle, which [`resolve_supers`] reports.
/// Stops at a repeat, so a cycle cannot loop.
pub fn super_closure(g: &Globals, p: &Pred) -> Vec<Pred> {
    let mut out: Vec<Pred> = Vec::new();
    let mut work = vec![p.clone()];
    while let Some(Pred::Proto(q, args)) = work.pop() {
        for s in &g.proto(q).supers {
            let inst = s.map_tys(&mut |t| t.subst_gen(&args, &[]));
            if !out.contains(&inst) {
                out.push(inst.clone());
                work.push(inst);
            }
        }
    }
    out
}

/// `preds` and everything they entail through supertraits.
pub fn entail_closure(g: &Globals, preds: &[Pred]) -> Vec<Pred> {
    let mut out: Vec<Pred> = preds.to_vec();
    for p in preds {
        if let Pred::Proto(..) = p {
            for q in super_closure(g, p) {
                if !out.contains(&q) {
                    out.push(q);
                }
            }
        }
    }
    out
}

/// Whether the constraint `c` (over the instance's `Gen`s) follows from
/// `given` (closed under supertraits): it is one of them, or an
/// instance for its head resolves it with a context that follows.
fn entailed(g: &Globals, given: &[Pred], c: &Pred, depth: u32) -> bool {
    if given.contains(c) {
        return true;
    }
    let Pred::Proto(r, args) = c else {
        return false;
    };
    let Some(Ty::Con(con, targs)) = args.first() else {
        return false;
    };
    match g.instance(*r, *con) {
        Some(inst) if depth < 8 => inst.context.iter().all(|q| {
            let q = q.map_tys(&mut |t| t.subst_gen(targs, &[]));
            entailed(g, given, &q, depth + 1)
        }),
        _ => false,
    }
}

/// `impl P for T`, for messages.
fn owner(g: &Globals, inst: &InstanceDef, proto: ProtoId) -> String {
    let head = inst.head.map_leaves(&mut |t| match t {
        Ty::Gen(i) => Some(Ty::Rigid(*i)),
        _ => None,
    });
    let text = Printer::with_names(g, &[], &inst.var_names).ty(&head);
    format!("impl {} for {text}", g.proto(proto).name)
}

/// Rule 1 of §4.1 for instance `index`: every direct supertrait has an
/// instance for the same head, with the determined arguments the
/// supertrait names, and a context this instance's context entails.
pub fn check_impl_supers(g: &Globals, index: usize) -> TResult<()> {
    let inst = &g.instances[index];
    let mut subst = vec![inst.head.clone()];
    subst.extend(inst.dets.iter().cloned());
    let given = entail_closure(g, &inst.context);
    let me = owner(g, inst, inst.proto);
    let printer = Printer::with_names(g, &inst.var_names, &[]);
    for sup in &g.proto(inst.proto).supers {
        let Pred::Proto(q, args) = sup else { continue };
        let want: Vec<Ty> = args.iter().map(|t| t.subst_gen(&subst, &[])).collect();
        let Some(qi) = g.instance(*q, inst.con) else {
            let head = owner(g, inst, *q).replacen("impl ", "impl of ", 1);
            let msg = format!("{me} requires an {head}");
            return Err(TypeError::other(&inst.pos, msg));
        };
        if !covers_colours(g, qi, inst) {
            let head = owner(g, inst, *q).replacen("impl ", "impl of ", 1);
            let theirs = owner(g, qi, *q);
            let only = theirs.split(" for ").nth(1).unwrap_or_default();
            let msg = format!("{me} requires an {head}; {theirs} covers only {only}");
            return Err(TypeError::other(&inst.pos, msg));
        }
        if qi.dets != want[1..] {
            let mut theirs = vec![want[0].clone()];
            theirs.extend(qi.dets.iter().cloned());
            let msg = format!(
                "{me} requires {}, but {} determines {}",
                printer.pred(&Pred::Proto(*q, want.clone())),
                owner(g, qi, *q),
                printer.pred(&Pred::Proto(*q, theirs))
            );
            return Err(TypeError::other(&inst.pos, msg));
        }
        if let Some(c) = qi.context.iter().find(|c| !entailed(g, &given, c, 0)) {
            let msg = format!(
                "{me} does not entail the context of {}: {}",
                owner(g, qi, *q),
                printer.pred(c)
            );
            return Err(TypeError::other(&inst.pos, msg));
        }
    }
    Ok(())
}

/// Whether instance `qi` covers every colour that `inst`'s head covers
/// (§1.3, §4.1 rule 1): wherever `qi`'s head gives a colour, `inst`'s
/// gives the same one.
fn covers_colours(g: &Globals, qi: &InstanceDef, inst: &InstanceDef) -> bool {
    let theirs = super::impl_head::fixed_colours(g, &qi.head);
    let mine = super::impl_head::fixed_colours(g, &inst.head);
    theirs.iter().zip(&mine).all(|(t, m)| t.is_none() || t == m)
}
