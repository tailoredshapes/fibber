//! The model's values.

use std::cell::RefCell;
use std::rc::Rc;

use crate::ast::Expr;

use super::eval::Env;

/// A value. Immutable objects are shared `Rc`s; cells and atoms are
/// shared mutable boxes, so two names for one cell see each other's
/// writes (case 05).
#[derive(Clone, Debug)]
pub enum V {
    /// `()`.
    Unit,
    /// An integer.
    Int(i64),
    /// A boolean.
    Bool(bool),
    /// A string.
    Str(Rc<str>),
    /// A struct or an enum variant: its constructor's name and fields.
    Data(Rc<str>, Rc<Vec<V>>),
    /// `nil` or `(some v)`.
    Opt(Option<Rc<V>>),
    /// A `Vec`, first element first.
    Vector(Rc<Vec<V>>),
    /// A `List`, head first.
    List(Rc<Vec<V>>),
    /// A cell (or the private cell of an `&` parameter).
    Cell(Rc<RefCell<V>>),
    /// An atom.
    Atom(Rc<RefCell<V>>),
    /// A weak reference: the target while it is alive, `None` once dead.
    Weak(Option<Rc<V>>),
    /// A task: its body until it has run, its result after.
    Task(Rc<RefCell<TaskState>>),
    /// A closure.
    Closure(Rc<Closure>),
    /// A named function used as a value.
    Named(Rc<str>),
}

/// A closure: its parameters, body and captured environment.
#[derive(Debug)]
pub struct Closure {
    /// The self-name of a `(fn name ..)`.
    pub self_name: Option<String>,
    /// Parameter names.
    pub params: Vec<String>,
    /// The body.
    pub body: Expr,
    /// The environment at creation.
    pub env: Env,
}

/// A task's state (syntax §3.14: the body runs when joined or awaited).
#[derive(Debug)]
pub enum TaskState {
    /// Not run yet.
    Pending(Expr, Env),
    /// A spawned closure, not run yet.
    Thunk(V),
    /// Finished with this value.
    Done(V),
}

impl V {
    /// A string value.
    pub fn str(s: &str) -> V {
        V::Str(Rc::from(s))
    }
    /// A struct or variant value.
    pub fn data(name: &str, fields: Vec<V>) -> V {
        V::Data(Rc::from(name), Rc::new(fields))
    }
}
