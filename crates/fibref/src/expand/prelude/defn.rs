//! `defn` and `defn-` (stdlib design §4.2, tranche 1 R6b), Clojure's
//! spelling of `defun`:
//!
//! ```text
//! (defn  name doc? [x: T ..] -> R body ..) ⟹ (defun name (x: T ..) -> R body ..)
//! (defn- name doc? [x: T ..] -> R body ..) ⟹ (defun name :private (x: T ..) -> R body ..)
//! ```
//!
//! The docstring is dropped; everything after the parameter vector (`->
//! R`, `:where`, the body) is passed on as written. A `&` in the vector
//! (rest parameters, L2) is an error that names the stdlib item that adds
//! it. Several arities, `([x] ..) ([x y] ..)` (L1), are one `defun` per
//! clause named `NAME$N` for the clause's parameter count `N`, in a `do`
//! (`overload` resolves the calls); each clause has its own `:private`
//! (right after its vector), `:where` and `-> R`, and `defn-` makes every
//! clause private. A single clause `(defn f ([x] ..))` is `(defn f [x]
//! ..)`. The built `defun` takes the call's position; the parameter list
//! takes the vector's.

use crate::syntax::{Form, FormKind, Pos};

use crate::expand::build::{check_arity, keyword, list, malformed, sym};
use crate::expand::error::ExpandError;
use crate::expand::overload::{clause_arity, clause_name, clauses_of};

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
        _ => Err(malformed(name, "expected a parameter vector", &form.pos)),
    }
}

/// `(defn name doc? [params] rest..)` and, with `private`, `(defn- ..)`.
pub(super) fn defn(items: Vec<Form>, pos: &Pos, private: bool) -> Result<Form, ExpandError> {
    let name = if private { "defn-" } else { "defn" };
    check_arity(name, &items, 2, None, pos)?;
    if items[1].as_sym().is_none() {
        return Err(malformed(name, "the name must be a symbol", &items[1].pos));
    }
    if let Some(clauses) = clauses_of(&items) {
        return clauses_defn(&items, clauses, pos, private);
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

/// `(defn name doc? ([x] ..) ..)`: one clause is the plain `defn`, several
/// are a `do` of the `defun`s `name$N`.
fn clauses_defn(
    items: &[Form],
    clauses: &[Form],
    pos: &Pos,
    private: bool,
) -> Result<Form, ExpandError> {
    let name = if private { "defn-" } else { "defn" };
    let mut parts = Vec::new();
    for clause in clauses {
        match clause.as_list() {
            Some(c)
                if c.first()
                    .is_some_and(|v| matches!(v.kind, FormKind::Vec(_))) =>
            {
                parts.push((clause, c))
            }
            _ => {
                return Err(malformed(
                    name,
                    "every clause is ([params] ..)",
                    &clause.pos,
                ))
            }
        }
    }
    if let [(clause, c)] = parts[..] {
        let mut single = vec![items[0].clone(), items[1].clone()];
        single.extend(c.iter().cloned());
        let _ = clause;
        return defn(single, pos, private);
    }
    let mut out = vec![sym("do", pos)];
    for (clause, c) in parts {
        let FormKind::Vec(params) = &c[0].kind else {
            continue;
        };
        if params.iter().any(|p| p.as_sym() == Some("&")) {
            return Err(malformed(name, "rest parameters need L2", &c[0].pos));
        }
        let arity = clause_arity(clause).unwrap_or(0);
        let mut defun = vec![sym("defun", pos), sym(&clause_name_of(items, arity), pos)];
        let marked = matches!(c.get(1).map(|f| &f.kind), Some(FormKind::Kw(k)) if k == "private");
        if private || marked {
            defun.push(keyword("private", pos));
        }
        defun.push(Form::new(FormKind::List(params.clone()), c[0].pos.clone()));
        defun.extend(c[1 + usize::from(marked)..].iter().cloned());
        out.push(list(defun, &clause.pos));
    }
    Ok(list(out, pos))
}

fn clause_name_of(items: &[Form], arity: usize) -> String {
    clause_name(items[1].as_sym().unwrap_or(""), arity)
}
