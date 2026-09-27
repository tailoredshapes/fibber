//! Expansion errors. Every malformed input to the expander is one of
//! these values, with the position of the offending form (spec/syntax.md
//! §1.3: "Every compile error names a position"); the expander never
//! panics on input.

use std::fmt;

use crate::syntax::Pos;

/// An expansion error: what went wrong and where.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExpandError {
    /// What went wrong.
    pub kind: ExpandErrorKind,
    /// The form at fault (for a macro, the call).
    pub pos: Pos,
}

impl ExpandError {
    /// An error of `kind` at `pos`.
    pub fn new(kind: ExpandErrorKind, pos: &Pos) -> Self {
        ExpandError {
            kind,
            pos: pos.clone(),
        }
    }
}

/// Every way expansion can fail. The section each comes from is named
/// on the variant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExpandErrorKind {
    /// A user macro was called but no evaluator is available to run its
    /// body ([`NoRunner`](super::NoRunner)). This is *pending*, not a
    /// failure of the program: the evaluator does not exist yet.
    MacroNeedsEvaluator {
        /// The macro.
        name: String,
    },
    /// A macro (user or prelude) called with a number of arguments its
    /// parameters do not admit (§3.16, §4.4).
    MacroArity {
        /// The macro.
        name: String,
        /// What it accepts, e.g. `"2"` or `"at least 1"`.
        expected: String,
        /// How many arguments the call had.
        found: usize,
    },
    /// `(unquote e)` or `(unquote-splicing e)` outside any quasiquote
    /// (§1.2: "inside a quasiquote").
    UnquoteOutsideQuasiquote {
        /// `unquote` or `unquote-splicing`.
        head: &'static str,
    },
    /// `,@e` whose value would have to stand for the whole template
    /// rather than for items of a list, vector or map (§3.16).
    SpliceOutsideList,
    /// A core form, prelude macro or definition whose shape its grammar
    /// in §3 or §4.4 does not admit.
    Malformed {
        /// The head of the form.
        head: String,
        /// What is wrong.
        reason: &'static str,
    },
    /// A `defmacro` that names a core form, or `quasiquote`, `unquote`,
    /// `unquote-splicing` (§4.2): those are never macro calls.
    MacroNamesCoreForm {
        /// The name.
        name: String,
    },
    /// `(derive P Name)` with `P` not one of `Eq Ord Hash Show` (§4.4).
    DeriveProtocol {
        /// The protocol named.
        name: String,
    },
    /// `(derive P Name)` with `Name` neither a struct nor an enum seen so
    /// far (§3.16: "defined earlier in the module").
    DeriveTarget {
        /// The name.
        name: String,
    },
    /// A struct reflection call on a name that is not a struct (§3.16:
    /// "an error at expansion when `Name` is not a struct").
    NotAStruct {
        /// The reflection call.
        op: &'static str,
        /// The name.
        name: String,
    },
    /// An enum reflection call on a name that is not an enum (§3.16).
    NotAnEnum {
        /// The reflection call.
        op: &'static str,
        /// The name.
        name: String,
    },
    /// A reflection call whose operand is not a `Sym` form, or a name
    /// that is not a reflection call.
    BadReflection {
        /// The call.
        op: String,
    },
    /// A `->`, `->>` or `doto` step that is neither a symbol nor a
    /// non-empty list, so no call can be built from it (§4.4).
    ThreadStep,
    /// `(& x)` other than as an argument of a call (§1.2: "anywhere else
    /// `(& x)` is an error at expansion"), or `&` applied to a
    /// non-symbol.
    InOutOutsideArgument,
    /// `(nil ...)` in an expression (§3.9: "`(nil)` in an expression is
    /// the error `nil is a constant, not a function; write nil`").
    NilCalled,
    /// `[..]` or `{..}` in a pattern (§1.4: "brackets are not allowed in
    /// v1").
    BracketInPattern,
    /// An expression at top level (§2: "an expression at top level is an
    /// error").
    ExpressionAtTopLevel,
    /// A definition (`defun`, `def`, `defstruct`, ...) inside an
    /// expression; §2 admits them only as top-level forms.
    DefinitionInExpression {
        /// The head.
        head: String,
    },
    /// An `ns` that is not the first form of the module (§5: "One `ns`
    /// form, first in the file").
    NsNotFirst,
    /// More macro expansions for one top-level form than
    /// [`Limits::max_steps`](super::Limits).
    TooManySteps {
        /// The limit.
        limit: usize,
    },
    /// Forms nested deeper than [`Limits::max_depth`](super::Limits)
    /// during expansion.
    TooDeep {
        /// The limit.
        limit: usize,
    },
}

impl fmt::Display for ExpandErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use ExpandErrorKind as K;
        match self {
            K::MacroNeedsEvaluator { name } => {
                write!(f, "macro {name} needs the evaluator to expand (pending)")
            }
            K::MacroArity {
                name,
                expected,
                found,
            } => {
                write!(f, "macro {name} takes {expected} argument(s), got {found}")
            }
            K::UnquoteOutsideQuasiquote { head } => write!(f, "{head} outside quasiquote"),
            K::SpliceOutsideList => write!(f, "unquote-splicing outside a list, vector or map"),
            K::Malformed { head, reason } => write!(f, "malformed {head}: {reason}"),
            K::MacroNamesCoreForm { name } => {
                write!(f, "defmacro cannot redefine core form {name}")
            }
            K::DeriveProtocol { name } => {
                write!(f, "cannot derive {name}: only Eq, Ord, Hash and Show")
            }
            K::DeriveTarget { name } => write!(f, "cannot derive for {name}: not a struct or enum"),
            K::NotAStruct { op, name } => write!(f, "{op}: {name} is not a struct"),
            K::NotAnEnum { op, name } => write!(f, "{op}: {name} is not an enum"),
            K::BadReflection { op } => write!(f, "bad reflection call {op}: expects one symbol"),
            K::ThreadStep => write!(f, "threading step must be a symbol or a non-empty list"),
            K::InOutOutsideArgument => {
                write!(
                    f,
                    "& x is only allowed on a symbol, as an argument of a call"
                )
            }
            K::NilCalled => write!(f, "nil is a constant, not a function; write nil"),
            K::BracketInPattern => write!(f, "brackets are not allowed in patterns"),
            K::ExpressionAtTopLevel => write!(f, "expression at top level"),
            K::DefinitionInExpression { head } => write!(f, "{head} is only allowed at top level"),
            K::NsNotFirst => write!(f, "ns must be the first form of the module"),
            K::TooManySteps { limit } => {
                write!(
                    f,
                    "more than {limit} macro expansions in one top-level form"
                )
            }
            K::TooDeep { limit } => write!(f, "expansion nested deeper than {limit} levels"),
        }
    }
}

impl fmt::Display for ExpandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.pos, self.kind)
    }
}

impl std::error::Error for ExpandError {}
