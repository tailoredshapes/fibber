//! Small constructors for the forms the expander builds. Every built
//! form takes the position it is given: the macro call's, per §1.3.

use crate::syntax::{Form, FormKind, IntWidth, Pos};

use super::error::{ExpandError, ExpandErrorKind};

/// `(Sym name)`.
pub(crate) fn sym(name: &str, pos: &Pos) -> Form {
    Form::new(FormKind::Sym(name.to_string()), pos.clone())
}

/// `(List items)`.
pub(crate) fn list(items: Vec<Form>, pos: &Pos) -> Form {
    Form::new(FormKind::List(items), pos.clone())
}

/// `(Vec items)`.
pub(crate) fn vector(items: Vec<Form>, pos: &Pos) -> Form {
    Form::new(FormKind::Vec(items), pos.clone())
}

/// A call `(head args...)` with a symbol head.
pub(crate) fn call(head: &str, args: Vec<Form>, pos: &Pos) -> Form {
    let mut items = Vec::with_capacity(args.len() + 1);
    items.push(sym(head, pos));
    items.extend(args);
    list(items, pos)
}

/// `()`, the unit value.
pub(crate) fn unit(pos: &Pos) -> Form {
    list(Vec::new(), pos)
}

/// An `i64` literal.
pub(crate) fn int(v: i64, pos: &Pos) -> Form {
    let width = IntWidth::I64;
    Form::new(FormKind::Int { v, width }, pos.clone())
}

/// A string literal.
pub(crate) fn string(s: &str, pos: &Pos) -> Form {
    Form::new(FormKind::Str(s.to_string()), pos.clone())
}

/// A boolean literal.
pub(crate) fn boolean(b: bool, pos: &Pos) -> Form {
    Form::new(FormKind::Bool(b), pos.clone())
}

/// A keyword, name without the colon.
pub(crate) fn keyword(name: &str, pos: &Pos) -> Form {
    Form::new(FormKind::Kw(name.to_string()), pos.clone())
}

/// The name of the head symbol of a list form, if it has one.
pub(crate) fn head_name(form: &Form) -> Option<&str> {
    form.as_list()?.first()?.as_sym()
}

/// A [`ExpandErrorKind::Malformed`] error.
pub(crate) fn malformed(head: &str, reason: &'static str, pos: &Pos) -> ExpandError {
    let kind = ExpandErrorKind::Malformed {
        head: head.to_string(),
        reason,
    };
    ExpandError::new(kind, pos)
}

/// A [`ExpandErrorKind::MacroArity`] error unless `min <= n <= max`
/// (`max` of `None` means unbounded), where `n` counts the arguments
/// after the head of `items`.
pub(crate) fn check_arity(
    name: &str,
    items: &[Form],
    min: usize,
    max: Option<usize>,
    pos: &Pos,
) -> Result<(), ExpandError> {
    let found = items.len().saturating_sub(1);
    if found >= min && max.is_none_or(|m| found <= m) {
        return Ok(());
    }
    let expected = match max {
        Some(m) if m == min => format!("{min}"),
        Some(m) => format!("{min} to {m}"),
        None => format!("at least {min}"),
    };
    let name = name.to_string();
    let kind = ExpandErrorKind::MacroArity {
        name,
        expected,
        found,
    };
    Err(ExpandError::new(kind, pos))
}
