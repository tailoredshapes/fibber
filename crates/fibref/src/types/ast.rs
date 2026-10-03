//! The resolved core AST that lowering produces from expanded forms.
//!
//! Every expression has an [`ExprId`] and a position; every binding
//! site (parameter, `&` parameter, `let`/`match` pattern variable,
//! `loop` variable, a named `fn`'s self-name) has a [`BindingId`] that
//! is unique in the whole program; every name is resolved to a binding
//! or a [`GlobalRef`]. The primitive forms keep their non-expression
//! operand typed as what it is: a field name ([`ExprKind::Field`],
//! [`ExprKind::SetField`]), a protocol ([`ExprKind::Dyn`]) or a target
//! type ([`ExprKind::Convert`]).

use crate::syntax::{FltWidth, Form, IntWidth, Pos};

use super::ty::{Colour, ProtoId, Scalar, TypeId};

/// An expression's identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ExprId(pub u32);

/// A binding site's identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BindingId(pub u32);

/// A `defun`: an index into [`Globals::funs`](super::decls::Globals).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FunId(pub u32);

/// A `def`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DefId(pub u32);

/// An `extern`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ExternId(pub u32);

/// An entry of the builtin table ([`BUILTINS`](super::builtins::BUILTINS)).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BuiltinId(pub u32);

/// What a global name denotes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GlobalRef {
    /// A `defun`.
    Fun(FunId),
    /// A struct's constructor (`variant: None`) or an enum variant.
    Ctor(TypeId, Option<usize>),
    /// Method `index` of a protocol.
    Method(ProtoId, usize),
    /// A builtin function.
    Builtin(BuiltinId),
    /// A `def`.
    Def(DefId),
    /// An `extern`.
    Extern(ExternId),
}

/// What kind of binding a [`BindingId`] is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BindingKind {
    /// A `defun`, method or `fn` parameter.
    Param,
    /// An `&` parameter of a `defun`: a cell, never a value (§2.14).
    AmpParam,
    /// A variable of a `let` pattern.
    Let,
    /// A variable of a `match` pattern.
    Pattern,
    /// A `loop` variable.
    Loop,
    /// The self-name of a named `fn`.
    FnSelf,
}

/// One binding site.
#[derive(Clone, Debug)]
pub struct BindingInfo {
    /// The name as written (a gensym starts with `#`).
    pub name: String,
    /// What kind of binding it is.
    pub kind: BindingKind,
    /// Where it is bound.
    pub pos: Pos,
    /// The annotation `x: T` of a `let` or `loop` binding (syntax
    /// §1.5); a parameter's is in its `defun` or `fn` literal.
    pub ann: Option<TypeAnn>,
}

/// A literal.
#[derive(Clone, Debug, PartialEq)]
pub enum Lit {
    /// An integer of its width.
    Int(i64, IntWidth),
    /// A float of its width.
    Float(f64, FltWidth),
    /// A string.
    Str(String),
    /// A character.
    Char(char),
    /// `true`/`false`.
    Bool(bool),
    /// A keyword, without the colon.
    Keyword(String),
    /// `()`.
    Unit,
}

/// An expression.
#[derive(Clone, Debug)]
pub struct Expr {
    /// Its identity (key of the type tables of the typed program).
    pub id: ExprId,
    /// Where it is.
    pub pos: Pos,
    /// What it is.
    pub kind: ExprKind,
}

/// The target of `deref` or `set!`: an expression, or the name of an
/// `&` parameter, which is not an expression (§2.9, §2.14).
#[derive(Clone, Debug)]
pub enum Place {
    /// An expression of cell (atom, weak) type.
    Expr(Box<Expr>),
    /// An `&` parameter.
    Amp(BindingId),
}

/// A call argument.
#[derive(Clone, Debug)]
pub enum Arg {
    /// An ordinary argument.
    Expr(Expr),
    /// `&x`: `x` a variable of cell type (§3.13).
    Amp(BindingId, Pos),
}

/// A `fn` literal.
#[derive(Clone, Debug)]
pub struct FnLit {
    /// The self-name of a named `fn`.
    pub name: Option<BindingId>,
    /// The parameters, with their annotations.
    pub params: Vec<(BindingId, Option<TypeAnn>)>,
    /// The result annotation.
    pub ret: Option<TypeAnn>,
    /// The body.
    pub body: Expr,
    /// The capture set: free variables of the body bound in enclosing
    /// scopes (§3.7), in order of first occurrence.
    pub captures: Vec<BindingId>,
}

/// A conversion primitive (§2.12). The groups are what typing needs;
/// the payloads say which primitive of the group was written, which is
/// what evaluating it needs (`zext` and `sext` differ on a negative
/// operand).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConvOp {
    /// `trunc`, `zext`, `sext`: integer to integer.
    IntToInt(IntConv),
    /// `fptrunc`, `fpext`: float to float.
    FloatToFloat,
    /// `fptosi` (`signed`), `fptoui`: float to integer.
    FloatToInt {
        /// `fptosi`.
        signed: bool,
    },
    /// `sitofp` (`signed`), `uitofp`: integer to float.
    IntToFloat {
        /// `sitofp`.
        signed: bool,
    },
}

/// Which integer-to-integer conversion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntConv {
    /// `trunc`.
    Trunc,
    /// `zext`: the operand read as unsigned.
    Zext,
    /// `sext`: the operand read as signed.
    Sext,
}

/// The kinds of expression.
#[derive(Clone, Debug)]
pub enum ExprKind {
    /// A literal.
    Lit(Lit),
    /// A local variable.
    Local(BindingId),
    /// A global name (`nil` is the `Option` variant constant).
    Global(GlobalRef),
    /// `(head args..)`.
    Call(Box<Expr>, Vec<Arg>),
    /// `(fn ..)`.
    Fn(Box<FnLit>),
    /// `(let ((pat e)..) body)`.
    Let(Vec<(Pattern, Expr)>, Box<Expr>),
    /// `(if c t e)`.
    If(Box<Expr>, Box<Expr>, Box<Expr>),
    /// `(do e..)`.
    Do(Vec<Expr>),
    /// `(match s clause..)`.
    Match(Box<Expr>, Vec<Clause>),
    /// `(loop ((x e)..) body)`.
    Loop(Vec<(BindingId, Expr)>, Box<Expr>),
    /// `(recur e..)`, in tail position of the innermost loop.
    Recur(Vec<Expr>),
    /// `(. e f)`; `text` is the printed `e` for error messages.
    Field(Box<Expr>, String, String),
    /// `(deref p)`, `@p`; `text` is the printed operand.
    Deref(Place, String),
    /// `(set! p e)`.
    Set(Place, Box<Expr>),
    /// `(set-field! &x f e)`; the cell variable, the field name, the
    /// value.
    SetField(BindingId, String, Box<Expr>),
    /// `(async body)` with its capture set.
    Async(Box<Expr>, Vec<BindingId>),
    /// `(await e)`.
    Await(Box<Expr>),
    /// `(unsafe body)`.
    Unsafe(Box<Expr>),
    /// `(quote f)`.
    Quote(Form),
    /// `(dyn P e)` / `(dyn (P D..) e)`, or with `:send` after the
    /// protocol (the `bool`, types §2.15).
    Dyn(ProtoId, Vec<TypeAnn>, bool, Box<Expr>),
    /// A conversion `(op T e)`.
    Convert(ConvOp, Scalar, Box<Expr>),
    /// `(concat v..)`, the variadic `(Vec a)` concatenation the
    /// quasiquote rewrite emits (syntax §3.16).
    Concat(Vec<Expr>),
    /// The one-armed `if` and the chain of one-armed `if`s that
    /// `(cond ..)` without a default expands to, `(test body)` pairs
    /// tried in order: unit when the bodies are, else `(Option T)` with
    /// `some` around the value (stdlib §7 L20). Elaborated away after
    /// inference (`elab`): no later pass sees it.
    Guarded(Vec<(Expr, Expr)>),
    /// `(and a b ..)`, two or more operands: typed by the last, tests
    /// `bool` or `(Option T)` (L20). Elaborated away.
    And(Vec<Expr>),
    /// `(or a b ..)`, two or more operands (L20). Elaborated away.
    Or(Vec<Expr>),
    /// `(fib.prelude/elide)`: the absent arm of the `when-let` match,
    /// `()` or `nil` as the other arm's type says. Elaborated away.
    Elided,
}

/// A `match` clause: `(pat body+)` or `(pat :when guard body+)`
/// (syntax §3.6).
#[derive(Clone, Debug)]
pub struct Clause {
    /// The pattern.
    pub pat: Pattern,
    /// The guard, evaluated after the pattern has bound its variables.
    pub guard: Option<Expr>,
    /// The body.
    pub body: Expr,
    /// The clause the checker adds to a refutable `let` pattern, whose
    /// body traps (stdlib spec §7 L8): it is not reported as redundant
    /// when the pattern above it already covers every value.
    pub fallback: bool,
}

/// What follows the elements of a vector pattern (syntax §3.6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rest {
    /// `[p..]`: exactly that many elements.
    Exact,
    /// `[p.. & _]`: at least that many; nothing is built.
    Ignore,
    /// `[p.. & r]`: at least that many; `r` owns a new vector of the
    /// remaining elements.
    Bind(BindingId),
}

/// A pattern (§2.6).
#[derive(Clone, Debug)]
pub struct Pattern {
    /// Where it is.
    pub pos: Pos,
    /// What it is.
    pub kind: PatKind,
}

/// The kinds of pattern.
#[derive(Clone, Debug)]
pub enum PatKind {
    /// `_`
    Wild,
    /// A variable.
    Bind(BindingId),
    /// A literal.
    Lit(Lit),
    /// A variant (`nil` and `(some p)` are `Option`'s) or a struct
    /// (`variant: None`), positional.
    Ctor(TypeId, Option<usize>, Vec<Pattern>),
    /// `(p :as x)`.
    As(Box<Pattern>, BindingId),
    /// A vector pattern `[p.. & r]` over the prelude's `Vec`.
    Vec(Vec<Pattern>, Rest),
}

/// A type annotation, resolved against the type names in scope.
#[derive(Clone, Debug, PartialEq)]
pub enum TypeAnn {
    /// A type variable by name.
    Var(String),
    /// `Self` in a protocol method signature.
    SelfTy,
    /// A scalar.
    Scalar(Scalar),
    /// `str`
    Str,
    /// A built-in constructor (`Array`, `Cell`, `Atom`, `Weak`, `Task`)
    /// applied to one argument.
    Builtin(super::ty::Con, Box<TypeAnn>),
    /// A struct or enum applied to its arguments.
    Nominal(TypeId, Vec<TypeAnn>),
    /// `(fn κ? (A..) R)`; `None` when the colour is omitted (§1.4).
    Fn(Option<ColourAnn>, Vec<TypeAnn>, Box<TypeAnn>),
    /// The argument of a nominal type at a colour parameter (§1.3).
    ColourArg(ColourAnn),
    /// `(dyn P)` / `(dyn (P D..))`, or with `:send` (the `bool`).
    Dyn(ProtoId, Vec<TypeAnn>, bool),
}

/// A colour as written in an annotation (§1.3, §1.4): `:send`,
/// `:local`, or a named colour variable.
#[derive(Clone, Debug, PartialEq)]
pub enum ColourAnn {
    /// `:send` or `:local`.
    Fixed(Colour),
    /// A colour variable or colour parameter, by name.
    Named(String),
}

impl Expr {
    /// Calls `f` on every direct sub-expression.
    pub fn children(&self, f: &mut dyn FnMut(&Expr)) {
        match &self.kind {
            ExprKind::Lit(_)
            | ExprKind::Local(_)
            | ExprKind::Global(_)
            | ExprKind::Quote(_)
            | ExprKind::Elided => {}
            ExprKind::Guarded(cs) => {
                for (t, b) in cs {
                    f(t);
                    f(b);
                }
            }
            ExprKind::And(es) | ExprKind::Or(es) => es.iter().for_each(f),
            ExprKind::Call(h, args) => {
                f(h);
                for a in args {
                    if let Arg::Expr(e) = a {
                        f(e);
                    }
                }
            }
            ExprKind::Fn(lit) => f(&lit.body),
            ExprKind::Let(bs, body) => {
                bs.iter().for_each(|(_, e)| f(e));
                f(body);
            }
            ExprKind::If(c, t, e) => {
                f(c);
                f(t);
                f(e);
            }
            ExprKind::Do(es) | ExprKind::Recur(es) | ExprKind::Concat(es) => es.iter().for_each(f),
            ExprKind::Match(s, cls) => {
                f(s);
                for c in cls {
                    if let Some(g) = &c.guard {
                        f(g);
                    }
                    f(&c.body);
                }
            }
            ExprKind::Loop(vs, body) => {
                vs.iter().for_each(|(_, e)| f(e));
                f(body);
            }
            ExprKind::Deref(p, _) => place_child(p, f),
            ExprKind::Set(p, v) => {
                place_child(p, f);
                f(v);
            }
            ExprKind::Field(e, _, _)
            | ExprKind::SetField(_, _, e)
            | ExprKind::Async(e, _)
            | ExprKind::Await(e)
            | ExprKind::Unsafe(e)
            | ExprKind::Dyn(_, _, _, e)
            | ExprKind::Convert(_, _, e) => f(e),
        }
    }
}

fn place_child(p: &Place, f: &mut dyn FnMut(&Expr)) {
    if let Place::Expr(e) = p {
        f(e);
    }
}
