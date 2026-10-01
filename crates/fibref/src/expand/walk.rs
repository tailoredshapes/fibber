//! The walk: applies [`Role`]s to a whole tree of forms with an explicit
//! stack of frames, so the nesting depth of the program costs heap, not
//! native stack. A user macro's runner is called from here at any
//! depth with the stack of the caller of [`walk`] only.
//!
//! Each form in expression position is entered (counted against
//! `Limits::max_depth`), expanded at its head, planned (`expr`), its
//! items walked by the plan, and finished (the §1.4 rewrite, `nil`).
//! Patterns are normalised the same way (§3.9, §1.4).
//!
//! The fusion rewrite (`fuse`) walks the forms of a module a second time
//! with this machine: `Role::Fuse` marks a collection position, and
//! `plan_expr` plans a form of that walk by its role.

use crate::syntax::{Form, FormKind, Pos};

use super::core::Role;
use super::error::{ExpandError, ExpandErrorKind};
use super::expr::{finish_form, plan_expr, Expander, Finish};

/// Rebuilds a list, vector or map from its walked items.
type Rebuild = fn(Vec<Form>) -> FormKind;

/// One form whose items are being walked.
struct Frame {
    pos: Pos,
    rebuild: Rebuild,
    items: std::vec::IntoIter<Form>,
    done: Vec<Form>,
    role: Role,
    finish: Finish,
    entered: bool,
}

/// What the machine does next.
enum Step {
    /// Walk this form by this role.
    Todo(Form, Role),
    /// This form is finished; hand it to the frame below.
    Done(Form),
}

/// Walks `form` by `role`.
pub(crate) fn walk(ex: &mut Expander, form: Form, role: Role) -> Result<Form, ExpandError> {
    let mut stack: Vec<Frame> = Vec::new();
    let mut step = Step::Todo(form, role);
    loop {
        step = match step {
            Step::Todo(form, role) => start(ex, form, role, &mut stack)?,
            Step::Done(value) => match stack.pop() {
                None => return Ok(value),
                Some(mut frame) => {
                    frame.done.push(value);
                    advance(ex, frame, &mut stack)
                }
            },
        };
    }
}

/// Expands a form in expression position.
pub(crate) fn expr(ex: &mut Expander, form: Form) -> Result<Form, ExpandError> {
    walk(ex, form, Role::Expr)
}

/// Begins walking `form` by `role`.
fn start(
    ex: &mut Expander,
    form: Form,
    role: Role,
    stack: &mut Vec<Frame>,
) -> Result<Step, ExpandError> {
    match role {
        Role::Keep => Ok(Step::Done(form)),
        Role::Expr | Role::Arg | Role::Fuse => {
            ex.enter(&form.pos)?;
            let (form, role, finish) = plan_expr(ex, form, &role)?;
            Ok(open(ex, form, role, finish, true, stack))
        }
        Role::Pattern => {
            ex.enter(&form.pos)?;
            pattern(ex, form, stack)
        }
        Role::Items(..) => Ok(open(ex, form, role, Finish::Same, false, stack)),
    }
}

/// A pattern: the symbol `nil` becomes `(Nil)`; braces are an error;
/// the items of a list or of a vector pattern (§1.4, §3.6) are patterns.
fn pattern(ex: &mut Expander, form: Form, stack: &mut Vec<Frame>) -> Result<Step, ExpandError> {
    let pos = form.pos;
    match form.kind {
        FormKind::Sym(s) if s == "nil" => {
            ex.leave();
            Ok(Step::Done(Form::new(FormKind::Nil, pos)))
        }
        FormKind::Map(_) => Err(ExpandError::new(ExpandErrorKind::BraceInPattern, &pos)),
        kind @ (FormKind::List(_) | FormKind::Vec(_)) => {
            let form = Form::new(kind, pos);
            let role = Role::after(0, Role::Pattern);
            Ok(open(ex, form, role, Finish::Same, true, stack))
        }
        kind => {
            ex.leave();
            Ok(Step::Done(Form::new(kind, pos)))
        }
    }
}

/// Opens a frame to walk the items of `form` by `role`, or finishes a
/// form that has no items to walk.
fn open(
    ex: &mut Expander,
    form: Form,
    role: Role,
    finish: Finish,
    entered: bool,
    stack: &mut Vec<Frame>,
) -> Step {
    let pos = form.pos;
    let (items, rebuild): (Vec<Form>, Rebuild) = match (&role, form.kind) {
        (Role::Items(..), FormKind::List(items)) => (items, FormKind::List),
        (Role::Items(..), FormKind::Vec(items)) => (items, FormKind::Vec),
        (Role::Items(..), FormKind::Map(items)) => (items, FormKind::Map),
        (_, kind) => {
            if entered {
                ex.leave();
            }
            return Step::Done(finish_form(Form::new(kind, pos), finish));
        }
    };
    let frame = Frame {
        pos,
        rebuild,
        done: Vec::with_capacity(items.len()),
        items: items.into_iter(),
        role,
        finish,
        entered,
    };
    advance(ex, frame, stack)
}

/// The role of item `i` under the `Items` role `role`.
fn item_role(role: &Role, i: usize) -> &Role {
    match role {
        Role::Items(roles, rest) => roles.get(i).unwrap_or(rest),
        other => other,
    }
}

/// Moves `frame` on to its next item that needs walking, pushing the
/// frame back, or finishes it when none is left.
fn advance(ex: &mut Expander, mut frame: Frame, stack: &mut Vec<Frame>) -> Step {
    while let Some(item) = frame.items.next() {
        let role = item_role(&frame.role, frame.done.len());
        if *role == Role::Keep {
            frame.done.push(item);
            continue;
        }
        let role = role.clone();
        stack.push(frame);
        return Step::Todo(item, role);
    }
    if frame.entered {
        ex.leave();
    }
    let form = Form::new((frame.rebuild)(frame.done), frame.pos);
    Step::Done(finish_form(form, frame.finish))
}
