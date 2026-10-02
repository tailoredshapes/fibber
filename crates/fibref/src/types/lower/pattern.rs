//! Lowering patterns (syntax §3.6): `_`, a variable, a literal, `nil`,
//! `(some p)`, `(Variant p..)`, `(Struct p..)`, `[p.. & r]`, `(p :as
//! x)`. A bare symbol is always a binding; a field-less variant is
//! matched as `(Variant)`. A variable may occur once per pattern. A
//! `let` pattern may be refutable: it traps when it does not match (§3.3).

use crate::syntax::{Form, FormKind};

use crate::types::ast::{BindingKind, GlobalRef, PatKind, Pattern, Rest};
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
        FormKind::Vec(items) => vec_pattern(lw, items, kind, names)?,
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
        return Err(TypeError::resolve(&items[0].pos, not_a_ctor(lw, head)));
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

/// `[p.. & r]` (§3.6): the elements, then `&` and one symbol or `_`.
fn vec_pattern(
    lw: &mut Lowerer<'_>,
    items: &[Form],
    kind: BindingKind,
    names: &mut Vec<String>,
) -> TResult<PatKind> {
    let amp = items.iter().position(|f| f.as_sym() == Some("&"));
    let (elems, tail) = match amp {
        Some(i) => (&items[..i], &items[i..]),
        None => (items, &items[items.len()..]),
    };
    let mut subs = Vec::with_capacity(elems.len());
    for p in elems {
        if let Some([h, _]) = p.as_list() {
            if h.as_sym() == Some("&") {
                let msg = "&r reads as (& r); write & r, with a space, in a vector pattern";
                return Err(TypeError::resolve(&p.pos, msg));
            }
        }
        subs.push(pattern(lw, p, kind, names)?);
    }
    let rest = match tail {
        [] => Rest::Exact,
        [_, r] => match r.as_sym() {
            Some("_") => Rest::Ignore,
            Some(n) if n != "&" => Rest::Bind(bind_once(lw, n, kind, r, names)?),
            _ => return Err(bad_rest(r)),
        },
        [amp, ..] => return Err(bad_rest(amp)),
    };
    Ok(PatKind::Vec(subs, rest))
}

fn bad_rest(at: &Form) -> TypeError {
    let msg = "& in a vector pattern is followed by one symbol or _";
    TypeError::resolve(&at.pos, msg)
}

/// Lowers the pattern of a `let` or `loop` binding (§3.3, stdlib §7 L8):
/// a vector pattern in binding position takes a prefix, so `[a b]` is
/// `[a b & _]` at every depth. Whether the result is refutable is the
/// second part: the binding is then a `match` whose last clause traps.
pub fn let_pattern(
    lw: &mut Lowerer<'_>,
    form: &Form,
    names: &mut Vec<String>,
) -> TResult<(Pattern, bool)> {
    let p = prefix(pattern(lw, form, BindingKind::Let, names)?);
    let refutable = !irrefutable(lw, &p);
    Ok((p, refutable))
}

/// `p` with every exact vector pattern in it open at the end.
fn prefix(p: Pattern) -> Pattern {
    let kind = match p.kind {
        PatKind::Vec(subs, rest) => {
            let rest = if rest == Rest::Exact {
                Rest::Ignore
            } else {
                rest
            };
            PatKind::Vec(subs.into_iter().map(prefix).collect(), rest)
        }
        PatKind::Ctor(id, v, subs) => PatKind::Ctor(id, v, subs.into_iter().map(prefix).collect()),
        PatKind::As(inner, b) => PatKind::As(Box::new(prefix(*inner)), b),
        other => other,
    };
    Pattern { pos: p.pos, kind }
}

fn irrefutable(lw: &Lowerer<'_>, p: &Pattern) -> bool {
    match &p.kind {
        PatKind::Wild | PatKind::Bind(_) => true,
        PatKind::Lit(crate::types::ast::Lit::Unit) => true,
        PatKind::Lit(_) => false,
        PatKind::As(inner, _) => irrefutable(lw, inner),
        PatKind::Vec(subs, rest) => subs.is_empty() && *rest != Rest::Exact,
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

/// `V is not a variant or struct`, `V is private to m` when a private
/// definition of another module has the name, or `V is exported by both
/// a and b` when two `:use`d modules export it (syntax §5).
fn not_a_ctor(lw: &Lowerer<'_>, head: &str) -> String {
    let space = crate::types::decls::Space::Value;
    match (
        lw.g.private_owner(lw.m, space, head),
        lw.g.ambiguity(lw.m, space, head),
    ) {
        (None, None) => format!("{head} is not a variant or struct"),
        _ => lw.g.unknown(lw.m, space, head, ""),
    }
}
