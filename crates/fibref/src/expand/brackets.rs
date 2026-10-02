//! Bracket binding forms (syntax §3.2, §3.3, §3.18): the bracket spelling
//! of the binding list of `let` and `loop`, and of the parameter list of
//! `fn`, is the parenthesised form they have always had.
//!
//! ```text
//! (let [a 1 b 2] body)            (let ((a 1) (b 2)) body)
//! (let [a: i64 1 b 2] body)       (let ((a: i64 1) (b 2)) body)
//! (loop [i 0 acc 0] body)         (loop ((i 0) (acc 0)) body)
//! (fn [x y] body)                 (fn (x y) body)
//! (fn name [x y] body)            (fn name (x y) body)
//! ```
//!
//! A binding is a pattern and an expression; a name that ends in `:` takes
//! the next form as its type and the next as its value, three items. The
//! rewrite is syntactic, one level, and needs no step of the budget: it
//! replaces the vector by a list of the same items (`fn`) or regroups them
//! (`let`, `loop`), so the planner and the checker only ever see the
//! parenthesised forms. Every parenthesised form keeps working.

use crate::syntax::{Form, FormKind};

use super::build::{list, malformed};
use super::core::annotated_name;
use super::error::ExpandError;

/// The items of a list or of a vector, the two spellings of a pair or a
/// parameter list that the binding macros accept.
pub(crate) fn seq_items(form: &Form) -> Option<&[Form]> {
    match &form.kind {
        FormKind::List(items) | FormKind::Vec(items) => Some(items),
        _ => None,
    }
}

/// The index of the bracket operand of `(let [..] ..)`, `(loop [..] ..)`,
/// `(fn [..] ..)` or `(fn name [..] ..)`, when the form has one.
fn bracket_at(items: &[Form]) -> Option<usize> {
    let at = match items.first()?.as_sym()? {
        "let" | "loop" => 1,
        "fn" if items.get(1).is_some_and(|f| f.as_sym().is_some()) => 2,
        "fn" => 1,
        _ => return None,
    };
    matches!(items.get(at)?.kind, FormKind::Vec(_)).then_some(at)
}

/// Whether `form` is a `let`, `loop` or `fn` written with a bracket
/// operand.
pub(crate) fn has_brackets(form: &Form) -> bool {
    form.as_list().and_then(bracket_at).is_some()
}

/// `form` with its bracket operand in the parenthesised spelling; a form
/// without one is returned as it is.
pub(crate) fn rewrite(form: Form) -> Result<Form, ExpandError> {
    let Form { kind, pos } = form;
    let FormKind::List(mut items) = kind else {
        return Ok(Form::new(kind, pos));
    };
    if let Some(at) = bracket_at(&items) {
        let head = items[0].as_sym().unwrap_or("let").to_string();
        let operand = std::mem::replace(&mut items[at], Form::new(FormKind::Nil, pos.clone()));
        let FormKind::Vec(inner) = operand.kind else {
            return Ok(Form::new(FormKind::List(items), pos));
        };
        let parts = if head == "fn" {
            inner
        } else {
            pairs(&head, inner)?
        };
        items[at] = list(parts, &operand.pos);
    }
    Ok(Form::new(FormKind::List(items), pos))
}

/// The bindings of a flat binding vector as the parenthesised pairs:
/// `pattern expr` and `name: type expr`.
fn pairs(head: &str, flat: Vec<Form>) -> Result<Vec<Form>, ExpandError> {
    let mut out = Vec::new();
    let mut rest = flat.into_iter();
    while let Some(first) = rest.next() {
        let width = if annotated_name(&first) { 3 } else { 2 };
        let at = first.pos.clone();
        let mut binding = vec![first];
        binding.extend(rest.by_ref().take(width - 1));
        if binding.len() < width {
            let reason =
                "a binding is a pattern and an expression, or name: a type and an expression";
            return Err(malformed(head, reason, &at));
        }
        out.push(list(binding, &at));
    }
    Ok(out)
}
