//! Top-level forms (§2): definitions are walked and registered in order,
//! a top-level `do` is spliced, and anything else is an error.

use crate::syntax::{Form, FormKind};

use super::build::{head_name, malformed};
use super::core::{body_start, skip_annotations, Role};
use super::error::{ExpandError, ExpandErrorKind as K};
use super::expr::{expand_head, Expander};
use super::heads::{is_core, is_definition, QUASI_FORMS};
use super::runner::{parse_params, MacroDef};
use super::walk::walk;

/// Expands one top-level form, appending what it stands for to `out`.
/// A spliced `do` is worked through with an explicit stack, so nesting
/// them costs no native stack.
pub(crate) fn top_form(
    ex: &mut Expander,
    form: Form,
    out: &mut Vec<Form>,
) -> Result<(), ExpandError> {
    let mut pending = vec![form];
    while let Some(form) = pending.pop() {
        let form = expand_head(ex, form)?;
        if head_name(&form) == Some("do") {
            if let FormKind::List(items) = form.kind {
                pending.extend(items.into_iter().skip(1).rev());
            }
            continue;
        }
        out.push(definition(ex, form, out.is_empty())?);
    }
    Ok(())
}

/// One top-level form that is not a `do`.
fn definition(ex: &mut Expander, form: Form, first: bool) -> Result<Form, ExpandError> {
    let name = head_name(&form).unwrap_or("").to_string();
    let role = match name.as_str() {
        "ns" if !first => return Err(ExpandError::new(K::NsNotFirst, &form.pos)),
        "defstruct" => {
            check_depth(ex, &form)?;
            ex.ctx.types.add_struct(&form)?;
            Role::Keep
        }
        "defenum" => {
            check_depth(ex, &form)?;
            ex.ctx.types.add_enum(&form)?;
            Role::Keep
        }
        "ns" | "defprotocol" | "extern" => {
            check_depth(ex, &form)?;
            Role::Keep
        }
        "defmacro" => return defmacro(ex, form),
        "defun" => defun_plan(&form)?,
        "def" => def_plan(&form)?,
        "impl" => impl_plan(&form)?,
        n if is_definition(n) => Role::Keep,
        _ => return Err(ExpandError::new(K::ExpressionAtTopLevel, &form.pos)),
    };
    walk(ex, form, role)
}

/// Fails if a form that is kept without walking nests deeper than the
/// limit (only a runner could build one), so later passes over it stay
/// bounded too.
fn check_depth(ex: &Expander, form: &Form) -> Result<(), ExpandError> {
    let limit = ex.ctx.limits.max_depth;
    let mut stack = vec![(form, 1usize)];
    while let Some((f, d)) = stack.pop() {
        if d > limit {
            return Err(ExpandError::new(K::TooDeep { limit }, &f.pos));
        }
        if let FormKind::List(items) | FormKind::Vec(items) | FormKind::Map(items) = &f.kind {
            stack.extend(items.iter().map(|i| (i, d + 1)));
        }
    }
    Ok(())
}

/// `(defun name (param*) where? ret? body)` (§3.1).
fn defun_plan(form: &Form) -> Result<Role, ExpandError> {
    let items = form.as_list().unwrap_or(&[]);
    if items.get(1).is_none_or(|n| n.as_sym().is_none()) {
        return Err(malformed("defun", "expected a name", &form.pos));
    }
    let start = body_start("defun", items, 2, &form.pos)?;
    Ok(Role::after(start, Role::Expr))
}

/// `(def name expr)` or `(def name: type expr)` (§3.19).
fn def_plan(form: &Form) -> Result<Role, ExpandError> {
    let items = form.as_list().unwrap_or(&[]);
    let annotated = items
        .get(1)
        .and_then(Form::as_sym)
        .is_some_and(|s| s.ends_with(':'));
    let len = if annotated { 4 } else { 3 };
    if items.len() != len {
        let reason = "expected (def name expr) or (def name: type expr)";
        return Err(malformed("def", reason, &form.pos));
    }
    Ok(Role::after(len - 1, Role::Expr))
}

/// `(impl P type where? method-impl+)`, `method-impl ::= (mname (self
/// sym*) ret? body)` (§3.10).
fn impl_plan(form: &Form) -> Result<Role, ExpandError> {
    let items = form.as_list().unwrap_or(&[]);
    let start = skip_annotations(items, 3);
    if items.len() < 4 || start >= items.len() {
        return Err(malformed(
            "impl",
            "expected a protocol, a type and methods",
            &form.pos,
        ));
    }
    let mut roles = vec![Role::Keep; start];
    for m in &items[start..] {
        let Some(parts) = m.as_list() else {
            return Err(malformed(
                "impl",
                "a method is (name (self param*) body)",
                &m.pos,
            ));
        };
        let body = body_start("impl", parts, 1, &m.pos)?;
        roles.push(Role::after(body, Role::Expr));
    }
    Ok(Role::items(roles, Role::Keep))
}

/// `(defmacro name (param*) body)` (§3.16): expands the body, records
/// the macro, and keeps the (expanded) definition in the program.
fn defmacro(ex: &mut Expander, form: Form) -> Result<Form, ExpandError> {
    let items = form.as_list().unwrap_or(&[]);
    if items.len() < 4 {
        let reason = "expected (defmacro name (param*) body)";
        return Err(malformed("defmacro", reason, &form.pos));
    }
    let Some(name) = items[1].as_sym().map(str::to_string) else {
        return Err(malformed(
            "defmacro",
            "the name must be a symbol",
            &items[1].pos,
        ));
    };
    if is_core(&name) || QUASI_FORMS.contains(&name.as_str()) || name == "nil" {
        return Err(ExpandError::new(
            K::MacroNamesCoreForm { name },
            &items[1].pos,
        ));
    }
    let (params, rest) = parse_params(&items[2])?;
    let form = walk(ex, form, Role::after(3, Role::Expr))?;
    let body = form.as_list().map(|i| i[3..].to_vec()).unwrap_or_default();
    let pos = form.pos.clone();
    let def = MacroDef {
        name: name.clone(),
        params,
        rest,
        body,
        pos,
    };
    ex.ctx.macros.insert(name, def);
    Ok(form)
}
