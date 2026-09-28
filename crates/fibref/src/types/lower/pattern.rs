//! Lowering patterns (syntax §3.6): `_`, a variable, a literal, `nil`,
//! `(some p)`, `(Variant p..)`, `(Struct p..)`, `(p :as x)`. A bare
//! symbol is always a binding; a field-less variant is matched as
//! `(Variant)`. A variable may occur once per pattern. A `let` pattern
//! must be irrefutable (§3.3).

use crate::syntax::{Form, FormKind};

use crate::types::ast::{BindingKind, GlobalRef, PatKind, Pattern};
use crate::types::decls::Shape;
use crate::types::error::{TResult, TypeError};

use super::expr::literal;
use super::scope::Lowerer;

/// Lowers a pattern, binding its variables with `kind`. `names` holds
/// the names bound so far by the enclosing form (one pattern, or one
/// `let`), which may not repeat.
pub fn pattern(
    lw: &mut Lowerer<'_>,
    form: &Form,
    kind: BindingKind,
    names: &mut Vec<String>,
) -> TResult<Pattern> {
    let pos = form.pos.clone();
    if let Some(lit) = literal(form) {
        return Ok(Pattern {
            pos,
            kind: PatKind::Lit(lit),
        });
    }
    let kind = match &form.kind {
        FormKind::Sym(s) if s == "_" => PatKind::Wild,
        FormKind::Sym(s) => PatKind::Bind(bind_once(lw, s, kind, form, names)?),
        FormKind::Nil => PatKind::Ctor(lw.g.option, Some(0), Vec::new()),
        FormKind::List(items) => list_pattern(lw, items, form, kind, names)?,
        _ => return Err(TypeError::resolve(&pos, format!("{form} is not a pattern"))),
    };
    Ok(Pattern { pos, kind })
}

fn bind_once(
    lw: &mut Lowerer<'_>,
    name: &str,
    kind: BindingKind,
    form: &Form,
    names: &mut Vec<String>,
) -> TResult<crate::types::ast::BindingId> {
    if names.iter().any(|n| n == name) {
        let msg = format!("{name} is bound twice in one pattern");
        return Err(TypeError::resolve(&form.pos, msg));
    }
    names.push(name.to_string());
    Ok(lw.bind(name, kind, &form.pos))
}

fn list_pattern(
    lw: &mut Lowerer<'_>,
    items: &[Form],
    form: &Form,
    kind: BindingKind,
    names: &mut Vec<String>,
) -> TResult<PatKind> {
    if let [p, kw, x] = items {
        if matches!(&kw.kind, FormKind::Kw(k) if k == "as") {
            let Some(xn) = x.as_sym() else {
                return Err(TypeError::resolve(&x.pos, ":as binds a symbol"));
            };
            let inner = pattern(lw, p, kind, names)?;
            let b = bind_once(lw, xn, kind, x, names)?;
            return Ok(PatKind::As(Box::new(inner), b));
        }
    }
    if matches!(items.first().map(|f| &f.kind), Some(FormKind::Nil)) && items.len() == 1 {
        return Ok(PatKind::Ctor(lw.g.option, Some(0), Vec::new()));
    }
    let Some(head) = items.first().and_then(Form::as_sym) else {
        return Err(TypeError::resolve(
            &form.pos,
            format!("{form} is not a pattern"),
        ));
    };
    let Some(GlobalRef::Ctor(id, variant)) = lw.g.value(lw.m, head) else {
        let space = crate::types::decls::Space::Value;
        let msg = match lw.g.private_owner(lw.m, space, head) {
            Some(_) => lw.g.unknown(lw.m, space, head, ""),
            None => format!("{head} is not a variant or struct"),
        };
        return Err(TypeError::resolve(&items[0].pos, msg));
    };
    let arity = match (&lw.g.ty(id).shape, variant) {
        (Shape::Struct(fs), None) => fs.len(),
        (Shape::Enum(vs), Some(i)) => vs.get(i).map_or(0, |v| v.fields.len()),
        _ => 0,
    };
    if arity != items.len() - 1 {
        let msg = format!(
            "{head} has {arity} field(s), the pattern has {}",
            items.len() - 1
        );
        return Err(TypeError::resolve(&form.pos, msg));
    }
    let subs = items[1..]
        .iter()
        .map(|p| pattern(lw, p, kind, names))
        .collect::<TResult<Vec<_>>>()?;
    Ok(PatKind::Ctor(id, variant, subs))
}

/// Lowers the pattern of a `let` binding, which must be irrefutable.
pub fn let_pattern(lw: &mut Lowerer<'_>, form: &Form, names: &mut Vec<String>) -> TResult<Pattern> {
    let p = pattern(lw, form, BindingKind::Let, names)?;
    if !irrefutable(lw, &p) {
        let msg = "a let pattern must be irrefutable";
        return Err(TypeError::resolve(&form.pos, msg));
    }
    Ok(p)
}

fn irrefutable(lw: &Lowerer<'_>, p: &Pattern) -> bool {
    match &p.kind {
        PatKind::Wild | PatKind::Bind(_) => true,
        PatKind::Lit(crate::types::ast::Lit::Unit) => true,
        PatKind::Lit(_) => false,
        PatKind::As(inner, _) => irrefutable(lw, inner),
        PatKind::Ctor(id, variant, subs) => {
            let single = match (&lw.g.ty(*id).shape, variant) {
                (Shape::Struct(_), None) => true,
                (Shape::Enum(vs), Some(_)) => vs.len() == 1,
                _ => false,
            };
            single && subs.iter().all(|s| irrefutable(lw, s))
        }
    }
}
