//! The generated program as a typed tree. Every node carries its type,
//! so the shrinker can replace any node by a smaller one of the same
//! type, and the model (`crate::model`) can compute the result the spec
//! requires without the interpreter.

use crate::ty::Ty;

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
    /// `empty`, the empty `List`.
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
/// `crate::print`), the constants, the helper functions and `main`'s body.
#[derive(Clone, Debug, PartialEq)]
pub struct Program {
    /// Top-level constants, in definition order.
    pub defs: Vec<Def>,
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
            Kind::VecLit(es) | Kind::Do(es) | Kind::Recur(es) => es.iter().collect(),
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
            Kind::Fn(_, e)
            | Kind::FnNamed(_, _, e)
            | Kind::Field(e, _)
            | Kind::Deref(e)
            | Kind::SetField(_, _, e)
            | Kind::Async(e)
            | Kind::Await(e)
            | Kind::WeakDead(_, e) => vec![e],
            _ => Vec::new(),
        }
    }

    /// The direct sub-expressions, mutably, in the order of [`children`](Self::children).
    pub fn children_mut(&mut self) -> Vec<&mut Expr> {
        match &mut self.kind {
            Kind::VecLit(es) | Kind::Do(es) | Kind::Recur(es) => es.iter_mut().collect(),
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
            Kind::Fn(_, e)
            | Kind::FnNamed(_, _, e)
            | Kind::Field(e, _)
            | Kind::Deref(e)
            | Kind::SetField(_, _, e)
            | Kind::Async(e)
            | Kind::Await(e)
            | Kind::WeakDead(_, e) => vec![e],
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

impl Program {
    /// Every expression of the program: the constants, the helper
    /// bodies, then `main`.
    pub fn bodies(&self) -> Vec<&Expr> {
        let defs = self.defs.iter().map(|d| &d.init);
        defs.chain(self.funs.iter().map(|f| &f.body))
            .chain([&self.main])
            .collect()
    }

    /// The total node count.
    pub fn size(&self) -> usize {
        self.bodies().iter().map(|e| e.size()).sum()
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
