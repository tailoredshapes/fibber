//! Type and resolution errors. The message of each starts with the
//! canonical text of spec/types.md §6.14 where the catalogue has one;
//! [`ErrorKind`] names the catalogue entry so tests need not match on
//! text alone.

use std::fmt;

use crate::syntax::Pos;

/// Which rule an error comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ErrorKind {
    /// A name that resolves to nothing, a malformed definition, a
    /// duplicate definition: resolution and shape errors outside §6.14.
    Resolve,
    /// `cannot unify T₁ with T₂` (§3.2).
    Unify,
    /// `cannot construct the infinite type` (§3.2).
    Infinite,
    /// `T has no field f` (§2.5).
    NoField,
    /// `cannot infer the struct type of e for field f; annotate it` (§3.4).
    FieldUnresolved,
    /// `cannot infer whether x is a cell, an atom or a weak reference` (§3.4).
    DerefUnresolved,
    /// `no implementation of P for T` (§3.3).
    NoInstance,
    /// `no implementation of P for a; add (P a) to the :where of the impl` (§2.7).
    ImplContext,
    /// `ambiguous constraint P a in f; add an annotation` (§3.3).
    Ambiguous,
    /// `cell cannot be shared between threads: <path> has type (Cell T)` (§5.3).
    CellNotSend,
    /// `value of type T cannot be shared between threads: <path>` (§5.3).
    ValueNotSend,
    /// A closure whose colour does not fit a rigid colour of an `impl`
    /// head (§1.3, §5.4): `local closure where colour k is required`,
    /// `closure of colour k cannot be shared between threads`.
    RigidColour,
    /// `& argument must be a cell variable` (§2.14).
    AmpArgument,
    /// `& parameter v used as a value in f` (§2.14).
    AmpParamValue,
    /// `parameter v of g is &; pass &x` (§2.2).
    AmpPosition,
    /// `function with & parameters is not a value` (§2.1).
    AmpFunctionValue,
    /// `non-exhaustive match: missing V` (§2.6).
    NonExhaustive,
    /// `redundant match clause` (§2.6).
    Redundant,
    /// `await outside async` (§2.8).
    AwaitOutsideAsync,
    /// `weak requires an object type` (§2.11).
    WeakScalar,
    /// `weak of an Option is not allowed` (§2.11).
    WeakOption,
    /// `dyn requires an object type`, and `f requires an object type`
    /// for an `(Object a)` bound of `f`'s scheme (§2.11, §2.15).
    NotObject,
    /// `V is a constant, not a function; write V` (§2.2).
    ConstantCalled,
    /// `recur outside loop` (§2.4).
    RecurOutsideLoop,
    /// `recur not in tail position` (§2.4).
    RecurNotTail,
    /// `def g has an unresolved type; annotate it` (§2.16).
    DefUnresolved,
    /// `def g: initialiser is not a constant expression` (§2.16).
    DefNotConstant,
    /// `def g and defun f depend on each other` (§2.16).
    DefCycle,
    /// Any other type error: arity, `main`'s type, overlapping
    /// instances, `unsafe`-only operations, and the like.
    Other,
}

/// One error: where, which rule, and the full message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeError {
    /// Which rule.
    pub kind: ErrorKind,
    /// The form whose rule generated the failing constraint (§3.5).
    pub pos: Pos,
    /// The message, canonical part first.
    pub message: String,
}

impl TypeError {
    /// An error of `kind` at `pos`.
    pub fn new(kind: ErrorKind, pos: &Pos, message: impl Into<String>) -> Self {
        TypeError {
            kind,
            pos: pos.clone(),
            message: message.into(),
        }
    }

    /// A resolution error.
    pub fn resolve(pos: &Pos, message: impl Into<String>) -> Self {
        TypeError::new(ErrorKind::Resolve, pos, message)
    }

    /// An error outside the catalogue.
    pub fn other(pos: &Pos, message: impl Into<String>) -> Self {
        TypeError::new(ErrorKind::Other, pos, message)
    }
}

impl fmt::Display for TypeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.pos, self.message)
    }
}

impl std::error::Error for TypeError {}

/// Result of a pass that stops at its first error.
pub type TResult<T> = Result<T, TypeError>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn display_names_the_position() {
        let pos = Pos {
            file: Arc::from("f.fib"),
            line: 3,
            col: 7,
            start: 0,
            end: 1,
        };
        let e = TypeError::new(ErrorKind::Unify, &pos, "cannot unify i64 with str");
        assert_eq!(e.to_string(), "f.fib:3:7: cannot unify i64 with str");
    }
}
