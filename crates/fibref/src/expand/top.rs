//! Top-level forms (§2): definitions are walked and registered in order,
//! a top-level `do` is spliced, and anything else is an error.

use crate::syntax::{Form, FormKind};

use super::build::{head_name, malformed};
use super::core::{body_start, skip_annotations, Role};
use super::error::{ExpandError, ExpandErrorKind as K};
use super::expr::{expand_head, Expander};
use super::fuse::names_of;
use super::heads::{is_core, is_definition, QUASI_FORMS};
use super::params;
use super::private::{marker_index, put_marker, take_marker, type_name as private_type_name};
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
        let done = definition(ex, form, out.is_empty())?;
        ex.ctx.add_own(names_of(&done));
        out.push(done);
    }
    Ok(())
}

/// One top-level form that is not a `do`. A `:private` marker (syntax
/// §5) is taken off while the definition is parsed and registered, and
/// put back for name resolution.
fn definition(ex: &mut Expander, form: Form, first: bool) -> Result<Form, ExpandError> {
    let Some(at) = marker_index(&form) else {
        return definition_of(ex, form, first);
    };
    let (form, marker) = take_marker(form, at);
    let kind = head_name(&form).unwrap_or("").to_string();
    if kind == "defstruct" || kind == "defenum" {
        if let Some(name) = private_type_name(&form) {
            ex.ctx.types.private.insert(name.to_string());
        }
    }
    let out = definition_of(ex, form, first)?;
    if kind == "defmacro" && marker.is_some() {
        if let Some(name) = out.as_list().and_then(|i| i.get(1)).and_then(Form::as_sym) {
            ex.ctx.make_macro_private(name);
        }
    }
    Ok(match marker {
        Some(m) => put_marker(out, at, m),
        None => out,
    })
}

fn definition_of(ex: &mut Expander, mut form: Form, first: bool) -> Result<Form, ExpandError> {
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
        "ns" | "extern" => {
            check_depth(ex, &form)?;
            Role::Keep
        }
        "defprotocol" => protocol_plan(&form)?,
        "defmacro" => return defmacro(ex, form),
        "defun" => {
            form = params::desugar(ex.ctx, form, true)?;
            defun_plan(&form)?
        }
        "def" => def_plan(&form)?,
        "impl" => impl_plan(&form)?,
        n if is_definition(n) => Role::Keep,
        _ => return Err(ExpandError::new(K::ExpressionAtTopLevel, &form.pos)),
    };
    walk(ex, form, role)
}

/// The roles of the items of a definition whose bodies hold expressions
/// (`defun`, `def`, `impl`, `defprotocol`), for a later walk of the
/// expanded program: the `:private` marker taken off. `None` for any
/// other form.
pub(crate) fn definition_role(form: &Form) -> Option<Role> {
    let role = match head_name(form)? {
        "defun" => defun_plan(form),
        "def" => def_plan(form),
        "impl" => impl_plan(form),
        "defprotocol" => protocol_plan(form),
        _ => return None,
    };
    role.ok()
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

/// `(defprotocol head (:requires (..))? method+)`, `method ::= (mname
/// (self ..) -> type body*)` (§3.10): the body of a method with a
/// default is expanded (types §4.1), the rest kept.
fn protocol_plan(form: &Form) -> Result<Role, ExpandError> {
    let items = form.as_list().unwrap_or(&[]);
    let start = match items.get(2).map(|f| &f.kind) {
        Some(FormKind::Kw(k)) if k == "requires" => 4,
        _ => 2,
    };
    if items.len() <= start {
        let reason = "expected a name and methods";
        return Err(malformed("defprotocol", reason, &form.pos));
    }
    let mut roles = vec![Role::Keep; start];
    for m in &items[start..] {
        let role = match m.as_list() {
            Some(parts) if parts.len() > 4 => Role::after(4, Role::Expr),
            _ => Role::Keep,
        };
        roles.push(role);
    }
    Ok(Role::items(roles, Role::Keep))
}

/// `(impl P type where? method-impl*)`, `method-impl ::= (mname (self
/// sym*) ret? body)` (§3.10); a method with a default may be omitted,
/// so an `impl` may have none (types §4.1).
fn impl_plan(form: &Form) -> Result<Role, ExpandError> {
    let items = form.as_list().unwrap_or(&[]);
    let start = skip_annotations(items, 3);
    if items.len() < 3 || start > items.len() {
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
        name,
        ns: String::new(),
        key: String::new(),
        private: false,
        params,
        rest,
        body,
        pos,
    };
    ex.ctx.define_macro(def);
    Ok(form)
}
