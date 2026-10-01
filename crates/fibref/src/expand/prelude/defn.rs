//! `defn` and `defn-` (stdlib design §4.2, tranche 1 R6b), Clojure's
//! spelling of `defun`:
//!
//! ```text
//! (defn  name doc? [x: T ..] -> R body ..) ⟹ (defun name (x: T ..) -> R body ..)
//! (defn- name doc? [x: T ..] -> R body ..) ⟹ (defun name :private (x: T ..) -> R body ..)
//! ```
//!
//! The docstring is dropped; everything after the parameter vector (`->
//! R`, `:where`, the body) is passed on as written. Two Clojure shapes
//! `defun` has no counterpart for are errors that name the stdlib item
//! that adds them: a `&` in the vector (rest parameters, L2) and a list of
//! clauses `([x] ..) ([x y] ..)` (several arities, L1). The built `defun`
//! takes the call's position; the parameter list takes the vector's.

use crate::syntax::{Form, FormKind, Pos};

use crate::expand::build::{check_arity, keyword, list, malformed, sym};
use crate::expand::error::ExpandError;

/// The parameter vector of a `defn` call and the index of the first form
/// after it. `name` is the macro's own name, for the error.
fn parameters<'a>(
    name: &str,
    items: &'a [Form],
    pos: &Pos,
) -> Result<(&'a Form, &'a [Form], usize), ExpandError> {
    let mut at = 2;
    if matches!(items.get(at).map(|f| &f.kind), Some(FormKind::Str(_))) {
        at += 1;
    }
    let Some(form) = items.get(at) else {
        return Err(malformed(name, "expected a parameter vector", pos));
    };
    match &form.kind {
        FormKind::Vec(params) => Ok((form, params, at + 1)),
        FormKind::List(clauses) if clauses.first().is_some_and(is_vector) => {
            Err(malformed(name, "several arities need L1", &form.pos))
        }
        _ => Err(malformed(name, "expected a parameter vector", &form.pos)),
    }
}

fn is_vector(form: &Form) -> bool {
    matches!(form.kind, FormKind::Vec(_))
}

/// `(defn name doc? [params] rest..)` and, with `private`, `(defn- ..)`.
pub(super) fn defn(items: Vec<Form>, pos: &Pos, private: bool) -> Result<Form, ExpandError> {
    let name = if private { "defn-" } else { "defn" };
    check_arity(name, &items, 2, None, pos)?;
    if items[1].as_sym().is_none() {
        return Err(malformed(name, "the name must be a symbol", &items[1].pos));
    }
    let (vector, params, rest_at) = parameters(name, &items, pos)?;
    if params.iter().any(|p| p.as_sym() == Some("&")) {
        return Err(malformed(name, "rest parameters need L2", &vector.pos));
    }
    let mut out = vec![sym("defun", pos), items[1].clone()];
    if private {
        out.push(keyword("private", pos));
    }
    out.push(Form::new(
        FormKind::List(params.to_vec()),
        vector.pos.clone(),
    ));
    out.extend(items[rest_at..].iter().cloned());
    Ok(list(out, pos))
}
