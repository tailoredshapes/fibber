//! The generated program as a typed tree. Every node carries its type,
//! so the shrinker can replace any node by a smaller one of the same
//! type, and the model (`crate::model`) can compute the result the spec
//! requires without the interpreter.

use crate::macros::Mac;
use crate::pipe::Pipe;
use crate::ty::{NumTy, Proto, Ty};

/// The target type of a conversion: `i64` or another number type.
pub type NumOrInt = Option<NumTy>;

/// A typed expression.
#[derive(Clone, Debug, PartialEq)]
pub struct Expr {
    /// The node's type.
    pub ty: Ty,
    /// What the node is.
    pub kind: Kind,
}

/// The node kinds; each prints as the fibber form named in its comment.
#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    /// An `i64` literal.
    Int(i64),
    /// `true` / `false`.
    Bool(bool),
    /// A string literal.
    Str(String),
    /// `()`.
    Unit,
    /// `nil`.
    Nil,
    /// `Empty`, the empty `List`.
    Empty,
    /// `[e ...]`, a literal vector (syntax §1.4).
    VecLit(Vec<Expr>),
    /// A local variable.
    Var(String),
    /// A named function used as a value.
    Global(String),
    /// `(let ((pat e) ...) body)`.
    Let(Vec<(Pat, Expr)>, Box<Expr>),
    /// `(if c t e)`.
    If(Box<Expr>, Box<Expr>, Box<Expr>),
    /// `(do e ...)`.
    Do(Vec<Expr>),
    /// `(match e (pat body) ...)`.
    Match(Box<Expr>, Vec<(Pat, Expr)>),
    /// `(loop ((x e) ...) body)`.
    Loop(Vec<(String, Expr)>, Box<Expr>),
    /// `(recur e ...)`; its type is the loop's.
    Recur(Vec<Expr>),
    /// `(fn (x: T ...) body)`, every parameter annotated.
    Fn(Vec<(String, Ty)>, Box<Expr>),
    /// `(fn name (x: T ...) body)`: `name` is the closure itself in the
    /// body (syntax §3.2).
    FnNamed(String, Vec<(String, Ty)>, Box<Expr>),
    /// `(name arg ...)`: a builtin, constructor, prelude or helper function.
    Call(String, Vec<Arg>),
    /// `(f arg ...)` with `f` an expression of function type.
    Apply(Box<Expr>, Vec<Expr>),
    /// `(. e field)`.
    Field(Box<Expr>, String),
    /// `@e`.
    Deref(Box<Expr>),
    /// `(set! target value)`.
    Set(Box<Expr>, Box<Expr>),
    /// `(set-field! &cell field value)`.
    SetField(String, String, Box<Expr>),
    /// `(async body)`.
    Async(Box<Expr>),
    /// `(await e)`.
    Await(Box<Expr>),
    /// `(plet ((x e) ...) body)`.
    Plet(Vec<(String, Expr)>, Box<Expr>),
    /// `(let ((name e)) (weak name))`: a weak reference whose target
    /// dies at the end of the `let`, so it never upgrades (case 20).
    WeakDead(String, Box<Expr>),
    /// `(dyn P e)`, or `(dyn P :send e)` when the flag is set (syntax §3.10).
    Dyn(Proto, bool, Box<Expr>),
    /// `(match e clause ...)` whose clauses may have guards
    /// (`(pat :when g body)`, syntax §3.6).
    GMatch(Box<Expr>, Vec<Clause>),
    /// `(m arg ...)`: a call of a preamble macro (syntax §3.16).
    Macro(Mac, Vec<Expr>),
    /// An integer literal of a width other than `i64`: `5i8`.
    IntW(i64, NumTy),
    /// A float literal: `2.5`, `0.25f32`.
    Flt(f64, NumTy),
    /// `(op T e)`: a conversion (types §2.12), the target type first.
    Conv(String, NumOrInt, Box<Expr>),
    /// A library pipeline and its terminals, of type `i64` (stdlib §8.1
    /// item 3); the program that holds one uses the library's facades.
    Pipe(Box<Pipe>),
}

/// A clause of a [`Kind::GMatch`].
#[derive(Clone, Debug, PartialEq)]
pub struct Clause {
    /// The pattern.
    pub pat: Pat,
    /// The guard, if the clause has one.
    pub guard: Option<Expr>,
    /// The body.
    pub body: Expr,
}

/// The rest of a vector pattern: `& r` or `& _`.
#[derive(Clone, Debug, PartialEq)]
pub enum Rest {
    /// `& r`: binds a new vector of the remaining elements.
    Bind(String),
    /// `& _`: builds nothing.
    Wild,
}

/// A call argument.
#[derive(Clone, Debug, PartialEq)]
pub enum Arg {
    /// An ordinary argument.
    Val(Expr),
    /// `&x`: an in-out argument naming a cell variable (syntax §3.13).
    InOut(String),
}

/// A pattern (syntax §3.6).
#[derive(Clone, Debug, PartialEq)]
pub enum Pat {
    /// `_`.
    Wild,
    /// A binding.
    Bind(String),
    /// `nil`.
    Nil,
    /// `(some p)`.
    Some(Box<Pat>),
    /// `(Ctor p ...)`: a struct or a variant, positional.
    Ctor(String, Vec<Pat>),
    /// `(p :as x)`.
    As(Box<Pat>, String),
    /// `[p ...]` or `[p ... & rest]` (syntax §3.6).
    Vector(Vec<Pat>, Option<Rest>),
    /// An integer literal pattern.
    Lit(i64),
}

/// One method of an `impl`: `(name (self param ...) body)`.
#[derive(Clone, Debug, PartialEq)]
pub struct Method {
    /// The method's name.
    pub name: String,
    /// The parameters after `self`, with their types.
    pub params: Vec<(String, Ty)>,
    /// The body, of type `i64`.
    pub body: Expr,
}

/// `(impl P T method ...)`: the methods it gives (a method with a
/// default that is left out takes the default, types §4.1).
#[derive(Clone, Debug, PartialEq)]
pub struct ImplDef {
    /// The protocol.
    pub proto: Proto,
    /// The implementing type.
    pub target: Ty,
    /// The methods given.
    pub methods: Vec<Method>,
    /// For a `Hook` target: whether the head gives the colour parameter
    /// a variable, `(Hook k)`, rigid in the bodies and covering both
    /// colours, rather than `(Hook :local)` (types §1.3). Unused for
    /// other targets.
    pub colour_var: bool,
}

/// A top-level parameter.
#[derive(Clone, Debug, PartialEq)]
pub struct Param {
    /// Its name.
    pub name: String,
    /// Its type (the content type for an `&` parameter).
    pub ty: Ty,
    /// Whether it is `&name`.
    pub inout: bool,
}

/// A generated `defun`.
#[derive(Clone, Debug, PartialEq)]
pub struct FunDef {
    /// Its name.
    pub name: String,
    /// Its parameters, all annotated.
    pub params: Vec<Param>,
    /// Its annotated result type.
    pub ret: Ty,
    /// Its body.
    pub body: Expr,
}

/// A top-level constant (syntax §3.19): immortal once evaluated.
#[derive(Clone, Debug, PartialEq)]
pub struct Def {
    /// Its name.
    pub name: String,
    /// Its annotated type.
    pub ty: Ty,
    /// Its constant initialiser.
    pub init: Expr,
}

/// A whole generated program: the fixed preamble (printed by
/// `crate::print`), the constants, the `impl`s, the helper functions
/// and `main`'s body.
#[derive(Clone, Debug, PartialEq)]
pub struct Program {
    /// Top-level constants, in definition order.
    pub defs: Vec<Def>,
    /// Protocol instances.
    pub impls: Vec<ImplDef>,
    /// Helper functions, in definition order.
    pub funs: Vec<FunDef>,
    /// The body of `(defun main () -> i64 ...)`.
    pub main: Expr,
}

impl Expr {
    /// A node of type `ty`.
    pub fn new(ty: Ty, kind: Kind) -> Self {
        Expr { ty, kind }
    }
    /// An `i64` literal.
    pub fn int(n: i64) -> Self {
        Expr::new(Ty::Int, Kind::Int(n))
    }
    /// A variable.
    pub fn var(name: &str, ty: Ty) -> Self {
        Expr::new(ty, Kind::Var(name.to_string()))
    }
    /// A call whose arguments are all ordinary.
    pub fn call(ty: Ty, head: &str, args: Vec<Expr>) -> Self {
        Expr::new(
            ty,
            Kind::Call(head.into(), args.into_iter().map(Arg::Val).collect()),
        )
    }

    /// The direct sub-expressions, in evaluation order.
    pub fn children(&self) -> Vec<&Expr> {
        match &self.kind {
            Kind::VecLit(es) | Kind::Do(es) | Kind::Recur(es) | Kind::Macro(_, es) => {
                es.iter().collect()
            }
            Kind::Let(bs, b) => bs.iter().map(|(_, e)| e).chain([b.as_ref()]).collect(),
            Kind::Loop(bs, b) | Kind::Plet(bs, b) => {
                bs.iter().map(|(_, e)| e).chain([b.as_ref()]).collect()
            }
            Kind::If(c, t, e) => vec![c, t, e],
            Kind::Match(s, cl) => [s.as_ref()]
                .into_iter()
                .chain(cl.iter().map(|(_, e)| e))
                .collect(),
            Kind::Call(_, args) => args
                .iter()
                .filter_map(|a| match a {
                    Arg::Val(e) => Some(e),
                    Arg::InOut(_) => None,
                })
                .collect(),
            Kind::Apply(f, args) => [f.as_ref()].into_iter().chain(args.iter()).collect(),
            Kind::Set(a, b) => vec![a, b],
            Kind::GMatch(s, cl) => {
                let parts = cl.iter().flat_map(|c| c.guard.iter().chain([&c.body]));
                [s.as_ref()].into_iter().chain(parts).collect()
            }
            Kind::Dyn(_, _, e)
            | Kind::Conv(_, _, e)
            | Kind::Fn(_, e)
            | Kind::FnNamed(_, _, e)
            | Kind::Field(e, _)
            | Kind::Deref(e)
            | Kind::SetField(_, _, e)
            | Kind::Async(e)
            | Kind::Await(e)
            | Kind::WeakDead(_, e) => vec![e],
            Kind::Pipe(p) => p.exprs(),
            _ => Vec::new(),
        }
    }

    /// The direct sub-expressions, mutably, in the order of [`children`](Self::children).
    pub fn children_mut(&mut self) -> Vec<&mut Expr> {
        match &mut self.kind {
            Kind::VecLit(es) | Kind::Do(es) | Kind::Recur(es) | Kind::Macro(_, es) => {
                es.iter_mut().collect()
            }
            Kind::Let(bs, b) => {
                let mut v: Vec<&mut Expr> = bs.iter_mut().map(|(_, e)| e).collect();
                v.push(b);
                v
            }
            Kind::Loop(bs, b) | Kind::Plet(bs, b) => {
                let mut v: Vec<&mut Expr> = bs.iter_mut().map(|(_, e)| e).collect();
                v.push(b);
                v
            }
            Kind::If(c, t, e) => vec![c, t, e],
            Kind::Match(s, cl) => {
                let mut v: Vec<&mut Expr> = vec![s];
                v.extend(cl.iter_mut().map(|(_, e)| e));
                v
            }
            Kind::Call(_, args) => args
                .iter_mut()
                .filter_map(|a| match a {
                    Arg::Val(e) => Some(e),
                    Arg::InOut(_) => None,
                })
                .collect(),
            Kind::Apply(f, args) => {
                let mut v: Vec<&mut Expr> = vec![f];
                v.extend(args.iter_mut());
                v
            }
            Kind::Set(a, b) => vec![a, b],
            Kind::GMatch(s, cl) => clause_parts_mut(s, cl),
            Kind::Dyn(_, _, e)
            | Kind::Conv(_, _, e)
            | Kind::Fn(_, e)
            | Kind::FnNamed(_, _, e)
            | Kind::Field(e, _)
            | Kind::Deref(e)
            | Kind::SetField(_, _, e)
            | Kind::Async(e)
            | Kind::Await(e)
            | Kind::WeakDead(_, e) => vec![e],
            Kind::Pipe(p) => p.exprs_mut(),
            _ => Vec::new(),
        }
    }

    /// Calls `f` on this node and every node below it, pre-order.
    pub fn walk(&self, f: &mut dyn FnMut(&Expr)) {
        f(self);
        for c in self.children() {
            c.walk(f);
        }
    }

    /// The number of nodes in the tree.
    pub fn size(&self) -> usize {
        let mut n = 0;
        self.walk(&mut |_| n += 1);
        n
    }
}

/// The scrutinee, guards and bodies of a guarded `match`, mutably.
fn clause_parts_mut<'a>(s: &'a mut Expr, cl: &'a mut [Clause]) -> Vec<&'a mut Expr> {
    let parts = cl
        .iter_mut()
        .flat_map(|c| c.guard.iter_mut().chain([&mut c.body]));
    [s].into_iter().chain(parts).collect()
}

impl Program {
    /// Every expression of the program: the constants, the method
    /// bodies, the helper bodies, then `main`.
    pub fn bodies(&self) -> Vec<&Expr> {
        let defs = self.defs.iter().map(|d| &d.init);
        let methods = self
            .impls
            .iter()
            .flat_map(|i| i.methods.iter().map(|m| &m.body));
        defs.chain(methods)
            .chain(self.funs.iter().map(|f| &f.body))
            .chain([&self.main])
            .collect()
    }

    /// Every expression of the program, mutably, in the order of
    /// [`bodies`](Self::bodies).
    pub fn bodies_mut(&mut self) -> Vec<&mut Expr> {
        let defs = self.defs.iter_mut().map(|d| &mut d.init);
        let methods = self
            .impls
            .iter_mut()
            .flat_map(|i| i.methods.iter_mut().map(|m| &mut m.body));
        defs.chain(methods)
            .chain(self.funs.iter_mut().map(|f| &mut f.body))
            .chain([&mut self.main])
            .collect()
    }

    /// The total node count.
    pub fn size(&self) -> usize {
        self.bodies().iter().map(|e| e.size()).sum()
    }

    /// Whether the program holds a library pipeline, and so must say
    /// which facades of the library it uses.
    pub fn uses_library(&self) -> bool {
        let mut found = false;
        for b in self.bodies() {
            b.walk(&mut |e| found |= matches!(e.kind, Kind::Pipe(_)));
        }
        found
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn children_skip_inout_arguments() {
        let e = Expr::new(
            Ty::Unit,
            Kind::Call(
                "f".into(),
                vec![Arg::InOut("x".into()), Arg::Val(Expr::int(1))],
            ),
        );
        assert_eq!(e.children(), vec![&Expr::int(1)]);
        assert_eq!(e.size(), 2);
    }
}
