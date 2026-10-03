//! Pattern parameters (stdlib spec §7 L7): a parameter that destructures
//! is sugar for a plain parameter and a `let` around the body.
//!
//! ```text
//! (fn (acc [k v]) body)            (fn (acc #param.1) (let (([k v] #param.1)) body))
//! (fn (a ([k v] :as p)) body)      (fn (a p) (let (([k v] p)) body))
//! (defun f (a ([k v] :as p: T)) body)
//!                                  (defun f (a p: T) (let (([k v] p)) body))
//! ```
//!
//! A `fn` literal takes the bare vector `[k v]` or `(PAT :as p)`, whose
//! type is inferred; a `defun` takes only `(PAT :as p: T)`, because a
//! top-level function states its types. The `let` is the checker's: the
//! pattern in it is refutable or not as it would be written there (§7 L8).

use crate::syntax::{Form, FormKind, Pos};

use super::build::{list, malformed, sym};
use super::core::body_start;
use super::ctx::ExpandCtx;
use super::error::ExpandError;

/// One item of a parameter list as the desugar sees it.
enum Param {
    /// Items that stay as they are: `x`, or `x:` and its type.
    Plain(Vec<Form>),
    /// A pattern parameter: the pattern, the symbol that stands for it
    /// in the `let`, and the items it leaves in the parameter list.
    Pattern {
        pat: Form,
        name: Option<Form>,
        kept: Vec<Form>,
        typed: bool,
    },
}

/// The index of the parameter list in `(fn name? (params) ..)` or
/// `(defun name (params) ..)`.
fn params_at(items: &[Form], defun: bool) -> usize {
    let named = items.get(1).is_some_and(|f| f.as_sym().is_some());
    if defun || named {
        2
    } else {
        1
    }
}

/// Whether `form` is a `fn` literal with a pattern among its parameters.
pub(crate) fn fn_has_patterns(form: &Form) -> bool {
    let Some(items) = form.as_list() else {
        return false;
    };
    if items.first().and_then(Form::as_sym) != Some("fn") {
        return false;
    }
    let params = items.get(params_at(items, false)).and_then(Form::as_list);
    params.is_some_and(|ps| split(ps).iter().any(|p| matches!(p, Param::Pattern { .. })))
}

/// Splits a parameter list into plain items and pattern parameters.
fn split(params: &[Form]) -> Vec<Param> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < params.len() {
        let p = &params[i];
        if p.as_sym().is_some_and(|s| s.ends_with(':')) && i + 1 < params.len() {
            out.push(Param::Plain(params[i..i + 2].to_vec()));
            i += 2;
            continue;
        }
        out.push(match pattern_param(p) {
            Some(pattern) => pattern,
            None => Param::Plain(vec![p.clone()]),
        });
        i += 1;
    }
    out
}

/// The number of parameters in a parameter list or vector (an annotated
/// parameter `x: T` and a pattern parameter count as one).
pub(crate) fn arity(params: &[Form]) -> usize {
    split(params).len()
}

/// `[pat]` or `(pat :as p)` or `(pat :as p: T)`.
fn pattern_param(p: &Form) -> Option<Param> {
    match &p.kind {
        FormKind::Vec(_) => Some(Param::Pattern {
            pat: p.clone(),
            name: None,
            kept: Vec::new(),
            typed: false,
        }),
        FormKind::List(items) => {
            let is_as = matches!(items.get(1).map(|f| &f.kind), Some(FormKind::Kw(k)) if k == "as");
            let var = items.get(2).and_then(Form::as_sym)?;
            if !is_as || var == "_" || var.ends_with(':') && items.len() != 4 {
                return None;
            }
            match (items.len(), var.strip_suffix(':')) {
                (3, None) => Some(Param::Pattern {
                    pat: items[0].clone(),
                    name: Some(items[2].clone()),
                    kept: vec![items[2].clone()],
                    typed: false,
                }),
                (4, Some(bare)) if !bare.is_empty() => Some(Param::Pattern {
                    pat: items[0].clone(),
                    name: Some(sym(bare, &items[2].pos)),
                    kept: vec![items[2].clone(), items[3].clone()],
                    typed: true,
                }),
                _ => None,
            }
        }
        _ => None,
    }
}

/// `form` with its pattern parameters replaced (see the module): `defun`
/// says whether it is a `defun`, which wants every pattern typed. A form
/// with no pattern parameter is returned as it is.
pub(crate) fn desugar(ctx: &ExpandCtx, form: Form, defun: bool) -> Result<Form, ExpandError> {
    let head = if defun { "defun" } else { "fn" };
    let Some(items) = form.as_list() else {
        return Ok(form);
    };
    let at = params_at(items, defun);
    let Some(params) = items.get(at).and_then(Form::as_list) else {
        return Ok(form);
    };
    let parsed = split(params);
    if !parsed.iter().any(|p| matches!(p, Param::Pattern { .. })) {
        return Ok(form);
    }
    let start = body_start(head, items, at, &form.pos)?;
    let (new_params, bindings) = rebuild(ctx, parsed, defun, &form.pos)?;
    let mut body = vec![sym("let", &form.pos), list(bindings, &form.pos)];
    body.extend_from_slice(&items[start..]);
    let mut out = items[..at].to_vec();
    out.push(list(new_params, &items[at].pos));
    out.extend_from_slice(&items[at + 1..start]);
    out.push(list(body, &form.pos));
    Ok(list(out, &form.pos))
}

/// The plain parameter list and the `(pat name)` bindings of the `let`.
fn rebuild(
    ctx: &ExpandCtx,
    parsed: Vec<Param>,
    defun: bool,
    pos: &Pos,
) -> Result<(Vec<Form>, Vec<Form>), ExpandError> {
    let mut new_params = Vec::new();
    let mut bindings = Vec::new();
    for p in parsed {
        match p {
            Param::Plain(items) => new_params.extend(items),
            Param::Pattern {
                pat,
                name,
                kept,
                typed,
            } => {
                if defun && !typed {
                    let reason = "a pattern parameter is (pattern :as name: type)";
                    return Err(malformed("defun", reason, &pat.pos));
                }
                let name = name.unwrap_or_else(|| ctx.gensym("param", &pat.pos));
                new_params.extend(if kept.is_empty() {
                    vec![name.clone()]
                } else {
                    kept
                });
                bindings.push(list(vec![pat, name], pos));
            }
        }
    }
    Ok((new_params, bindings))
}
