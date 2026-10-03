//! The quasiquote rewrite of §3.16:
//!
//! ```text
//! `atom                ⟹ (quote atom)
//! `(a ~b ~@cs d)       ⟹ (List (concat ['a] [b] cs ['d]))
//! `[ ... ]  `{ ... }   ⟹ the same with Vec / Map
//! ```
//!
//! The heads `List`, `Vec`, `Map` and `concat` are written
//! `fib.prelude/List` and so on (`collections::prelude_name`, as §1.4's
//! literals are): a program's or a library's own `concat`, or a variant of
//! another enum called `List`, would otherwise be what a macro's template
//! built with. `concat` of no parts (`` `() ``) is the empty vector.
//!
//! Nesting follows the usual levels: an inner `quasiquote` raises the
//! level, an `unquote` or `unquote-splicing` lowers it, and only one at
//! level 1 is evaluated; deeper ones are rebuilt as data with their
//! operand rewritten one level down. A `~@` at level 1 that is not an
//! item of a list, vector or map is [`ExpandErrorKind::SpliceOutsideList`].
//!
//! The rewrite produces vector literals (`['a]`), which the expander
//! then rewrites by §1.4 like any other. Every built form takes the
//! position of the template form it was built for. The template is
//! walked with an explicit stack, so its depth costs no native stack.

use crate::syntax::{Form, FormKind, Pos};

use super::build::{call, head_name, malformed, sym, vector};
use super::collections::prelude_name;
use super::error::{ExpandError, ExpandErrorKind as K};

/// Work still to do; results go on a value stack.
enum Task {
    /// Rewrite this template form at this level; push its expression.
    Visit(Form, usize),
    /// Push this expression as it is (a spliced operand).
    Push(Form),
    /// Pop an expression `x`, push `[x]`.
    Wrap(Pos),
    /// Pop `n` parts, push `(ctor (concat parts...))`.
    Seq(&'static str, usize, Pos),
    /// Pop the operand's expression `x`, push the data `(head x)`.
    Wrapped(&'static str, Pos),
}

/// Rewrites the form `(quasiquote template)`.
pub(crate) fn rewrite(form: Form) -> Result<Form, ExpandError> {
    let qpos = form.pos.clone();
    let template = operand(form, "quasiquote")?;
    let mut tasks = vec![Task::Visit(template, 1)];
    let mut values: Vec<Form> = Vec::new();
    while let Some(task) = tasks.pop() {
        match task {
            Task::Visit(t, level) => visit(t, level, &mut tasks, &mut values)?,
            Task::Push(f) => values.push(f),
            Task::Wrap(pos) => {
                let x = pop(&mut values, &pos)?;
                values.push(vector(vec![x], &pos));
            }
            Task::Seq(ctor, n, pos) => {
                let parts = values.split_off(values.len().saturating_sub(n));
                let parts = call(&prelude_name("concat"), parts, &pos);
                values.push(call(&prelude_name(ctor), vec![parts], &pos));
            }
            Task::Wrapped(head, pos) => {
                let x = pop(&mut values, &pos)?;
                let quoted = vector(vec![call("quote", vec![sym(head, &pos)], &pos)], &pos);
                let parts = vec![quoted, vector(vec![x], &pos)];
                let parts = call(&prelude_name("concat"), parts, &pos);
                values.push(call(&prelude_name("List"), vec![parts], &pos));
            }
        }
    }
    match (values.pop(), values.is_empty()) {
        (Some(v), true) => Ok(v),
        _ => Err(malformed(
            "quasiquote",
            "internal: unbalanced rewrite",
            &qpos,
        )),
    }
}

fn pop(values: &mut Vec<Form>, pos: &Pos) -> Result<Form, ExpandError> {
    values
        .pop()
        .ok_or_else(|| malformed("quasiquote", "internal: empty value stack", pos))
}

/// Schedules the rewrite of the template form `t` at `level`.
fn visit(
    t: Form,
    level: usize,
    tasks: &mut Vec<Task>,
    values: &mut Vec<Form>,
) -> Result<(), ExpandError> {
    let special = match head_name(&t) {
        Some("unquote") => Some("unquote"),
        Some("unquote-splicing") => Some("unquote-splicing"),
        Some("quasiquote") => Some("quasiquote"),
        _ => None,
    };
    match (special, level) {
        (Some("unquote"), 1) => values.push(operand(t, "unquote")?),
        (Some("unquote-splicing"), 1) => {
            return Err(ExpandError::new(K::SpliceOutsideList, &t.pos))
        }
        (Some(head), _) => {
            let inner = if head == "quasiquote" {
                level + 1
            } else {
                level - 1
            };
            let pos = t.pos.clone();
            tasks.push(Task::Wrapped(head, pos));
            tasks.push(Task::Visit(operand(t, head)?, inner));
        }
        (None, _) => sequence(t, level, tasks, values)?,
    }
    Ok(())
}

/// A list, vector or map with no special head, or an atom.
fn sequence(
    t: Form,
    level: usize,
    tasks: &mut Vec<Task>,
    values: &mut Vec<Form>,
) -> Result<(), ExpandError> {
    let pos = t.pos;
    let (ctor, items) = match t.kind {
        FormKind::List(items) => ("List", items),
        FormKind::Vec(items) => ("Vec", items),
        FormKind::Map(items) => ("Map", items),
        kind => {
            values.push(call("quote", vec![Form::new(kind, pos.clone())], &pos));
            return Ok(());
        }
    };
    tasks.push(Task::Seq(ctor, items.len(), pos));
    for item in items.into_iter().rev() {
        if level == 1 && head_name(&item) == Some("unquote-splicing") {
            tasks.push(Task::Push(operand(item, "unquote-splicing")?));
        } else {
            tasks.push(Task::Wrap(item.pos.clone()));
            tasks.push(Task::Visit(item, level));
        }
    }
    Ok(())
}

/// The one operand of `(head x)`.
fn operand(t: Form, head: &str) -> Result<Form, ExpandError> {
    let pos = t.pos;
    match t.kind {
        FormKind::List(items) if items.len() == 2 => items
            .into_iter()
            .nth(1)
            .ok_or_else(|| malformed(head, "expected one operand", &pos)),
        _ => Err(malformed(head, "expected one operand", &pos)),
    }
}
