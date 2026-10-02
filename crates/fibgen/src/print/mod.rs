//! Printing a generated program as fibber source (spec/syntax.md).

use crate::ast::{Arg, Expr, Kind, Program};
use crate::ty::Ty;

mod items;
mod pipelines;

pub use items::is_private;

use items::{fundef, impl_def, pat};

/// The preamble's declarations: (the names that use them, the text).
const PREAMBLE: [(&[&str], &str); 13] = [
    (&["Pt", "Shape", "Rect", "Circle", "Named"], "(defstruct Pt (x: i64 y: i64))"),
    (&["Wrap", "Shape", "Rect", "Circle", "Named"], "(defstruct Wrap (s: str v: (Vec i64)))"),
    (&["Holder"], "(defstruct Holder (f: (fn (i64) i64) c: (Cell i64)))"),
    (
        &["Shape", "Circle", "Rect", "Named"],
        "(defenum Shape (Circle r: i64) (Rect a: Pt b: Pt) (Named n: str w: Wrap))",
    ),
    (&["Hook"], "(defstruct (Hook k :colour) (f: (fn k (i64) i64) tag: i64))"),
    (
        &["Job", "Idle", "Ready"],
        "(defenum (Job k :colour) (Idle) (Ready run: (fn k () i64)))",
    ),
    (
        &["Score", "Rank", "score", "bonus", "rank", "tier"],
        "(defprotocol Score\n  (score (self) -> i64)\n  (bonus (self k: i64) -> i64 (+ (score self) k)))",
    ),
    (
        &["Rank", "rank", "tier"],
        "(defprotocol Rank :requires (Score)\n  (rank (self) -> i64)\n  (tier (self) -> i64 (+ (rank self) (score self))))",
    ),
    (
        &["Ver"],
        "(defstruct Ver (major: i64 minor: i64))\n(derive Eq Ver)\n(derive Ord Ver)",
    ),
    (
        &["Lvl", "Low", "Mid", "High"],
        "(defenum Lvl (Low) (Mid n: i64) (High a: i64 b: str))\n(derive Eq Lvl)\n(derive Ord Lvl)",
    ),
    (&["inc1"], "(defun inc1 (x: i64) -> i64 (+ x 1))"),
    (
        // A fold that sees every element and its place; `fib.prelude/..`
        // so that it does not use the library the pipeline tests.
        &["digest"],
        "(defun digest (v: (Vec i64)) -> i64\n  (loop ((i 0) (h 7))\n    (if (< i (vec-count v))\n        (recur (+ i 1) (rem (+ (* h 31) (vec-nth v i)) 1000003))\n        h)))",
    ),
    (
        &["sum-vec"],
        "(defun sum-vec (v: (Vec i64)) -> i64\n  (loop ((i 0) (s 0))\n    (if (< i (count v)) (recur (+ i 1) (+ s (nth v i))) s)))",
    ),
];

/// An s-expression ready to lay out.
#[derive(Clone, Debug)]
pub(crate) enum Sexp {
    Atom(String),
    List(Vec<Sexp>),
    Vector(Vec<Sexp>),
    Prefix(&'static str, Box<Sexp>),
}

pub(crate) fn atom(s: impl Into<String>) -> Sexp {
    Sexp::Atom(s.into())
}

pub(crate) fn list(items: Vec<Sexp>) -> Sexp {
    Sexp::List(items)
}

/// The whole program's source, preamble declarations included only when
/// the rest of the program names them.
pub fn program(p: &Program) -> String {
    let mut body = String::new();
    for d in &p.defs {
        let def = list(vec![
            atom("def"),
            atom(format!("{}:", d.name)),
            atom(d.ty.to_string()),
            expr(&d.init),
        ]);
        body.push_str(&layout(&def, 0));
        body.push('\n');
    }
    if !p.defs.is_empty() {
        body.push('\n');
    }
    for i in &p.impls {
        body.push_str(&layout(&impl_def(i), 0));
        body.push_str("\n\n");
    }
    for f in &p.funs {
        body.push_str(&layout(&fundef(f), 0));
        body.push_str("\n\n");
    }
    let main = list(vec![
        atom("defun"),
        atom("main"),
        atom("()"),
        atom("->"),
        atom("i64"),
        expr(&p.main),
    ]);
    body.push_str(&layout(&main, 0));
    body.push('\n');
    let uses = if p.uses_library() { LIBRARY_NS } else { "" };
    format!("{uses}{}{body}", preamble(&body))
}

/// The line a program with a library pipeline starts with: the four
/// facades, so that it means the same before the library is implicit
/// and after (tranche 1 plan §4.4).
const LIBRARY_NS: &str = "(ns main (:use fib.core fib.seq fib.coll fib.print))\n\n";

/// The preamble declarations and macros that `body` names.
fn preamble(body: &str) -> String {
    let tokens: std::collections::HashSet<&str> = body
        .split(|c: char| c.is_whitespace() || "()[]@&".contains(c))
        .collect();
    let mut out = String::new();
    for (names, text) in PREAMBLE {
        if names.iter().any(|n| tokens.contains(n)) {
            out.push_str(text);
            out.push('\n');
        }
    }
    for m in crate::macros::ALL {
        if tokens.contains(m.name()) {
            out.push_str(m.definition());
            out.push('\n');
        }
    }
    if !out.is_empty() {
        out.push('\n');
    }
    out
}

/// One expression's source, laid out.
pub fn expr_text(e: &Expr) -> String {
    layout(&expr(e), 0)
}

/// `(head item ...)`.
pub(crate) fn form(head: &str, items: impl IntoIterator<Item = Sexp>) -> Sexp {
    list([atom(head)].into_iter().chain(items).collect())
}

/// `((name e) ...)` for `loop` and `plet` bindings.
fn name_binds(bs: &[(String, Expr)]) -> Sexp {
    list(
        bs.iter()
            .map(|(n, e)| list(vec![atom(n.clone()), expr(e)]))
            .collect(),
    )
}

pub(crate) fn expr(e: &Expr) -> Sexp {
    match &e.kind {
        Kind::Int(n) => atom(n.to_string()),
        Kind::Bool(b) => atom(b.to_string()),
        Kind::Str(s) => atom(format!("{s:?}")),
        Kind::Unit => atom("()"),
        Kind::Nil => atom("nil"),
        Kind::Empty => atom("Empty"),
        Kind::VecLit(es) => Sexp::Vector(es.iter().map(expr).collect()),
        Kind::Var(n) | Kind::Global(n) => atom(n.clone()),
        Kind::Let(bs, b) => {
            let binds = bs.iter().map(|(p, e)| list(vec![pat(p), expr(e)]));
            form("let", [list(binds.collect()), expr(b)])
        }
        Kind::If(c, t, f) => form("if", [expr(c), expr(t), expr(f)]),
        Kind::Do(es) => form("do", es.iter().map(expr)),
        Kind::Match(s, cl) => {
            let clauses = cl.iter().map(|(p, b)| list(vec![pat(p), expr(b)]));
            form("match", [expr(s)].into_iter().chain(clauses))
        }
        Kind::Loop(bs, b) => form("loop", [name_binds(bs), expr(b)]),
        Kind::Recur(es) => form("recur", es.iter().map(expr)),
        Kind::Fn(ps, b) => form("fn", [fn_params(ps), expr(b)]),
        Kind::FnNamed(n, ps, b) => form("fn", [atom(n.clone()), fn_params(ps), expr(b)]),
        _ => expr_more(e),
    }
}

fn expr_more(e: &Expr) -> Sexp {
    match &e.kind {
        Kind::Call(h, args) => {
            let args = args.iter().map(|a| match a {
                Arg::Val(e) => expr(e),
                Arg::InOut(n) => Sexp::Prefix("&", Box::new(atom(n.clone()))),
            });
            list([atom(h.clone())].into_iter().chain(args).collect())
        }
        Kind::Apply(f, args) => list([expr(f)].into_iter().chain(args.iter().map(expr)).collect()),
        Kind::Field(s, f) => list(vec![atom("."), expr(s), atom(f.clone())]),
        Kind::Deref(c) => Sexp::Prefix("@", Box::new(expr(c))),
        Kind::Set(t, v) => list(vec![atom("set!"), expr(t), expr(v)]),
        Kind::SetField(c, f, v) => list(vec![
            atom("set-field!"),
            Sexp::Prefix("&", Box::new(atom(c.clone()))),
            atom(f.clone()),
            expr(v),
        ]),
        Kind::Async(b) => list(vec![atom("async"), expr(b)]),
        Kind::Await(t) => list(vec![atom("await"), expr(t)]),
        Kind::Plet(bs, b) => form("plet", [name_binds(bs), expr(b)]),
        Kind::Dyn(..)
        | Kind::GMatch(..)
        | Kind::Macro(..)
        | Kind::IntW(..)
        | Kind::Flt(..)
        | Kind::Conv(..) => items::expr_new(e),
        Kind::Pipe(p) => pipelines::pipe(p),
        Kind::WeakDead(n, t) => {
            let bind = list(vec![list(vec![atom(n.clone()), expr(t)])]);
            list(vec![
                atom("let"),
                bind,
                list(vec![atom("weak"), atom(n.clone())]),
            ])
        }
        _ => atom("<unprintable>"),
    }
}

/// `(x: T ...)`, or `()`.
pub(crate) fn fn_params(ps: &[(String, Ty)]) -> Sexp {
    let params = ps
        .iter()
        .flat_map(|(p, t)| [atom(format!("{p}:")), atom(t.to_string())])
        .collect();
    list_or_unit(params)
}

/// `()` for an empty parameter list, else the list.
fn list_or_unit(items: Vec<Sexp>) -> Sexp {
    if items.is_empty() {
        atom("()")
    } else {
        list(items)
    }
}

/// An expression's source on one line, whatever its length.
#[cfg(test)]
pub(crate) fn expr_flat(e: &Expr) -> String {
    flat(&expr(e))
}

/// The text of `s` on one line.
fn flat(s: &Sexp) -> String {
    match s {
        Sexp::Atom(a) => a.clone(),
        Sexp::List(xs) => format!("({})", xs.iter().map(flat).collect::<Vec<_>>().join(" ")),
        Sexp::Vector(xs) => format!("[{}]", xs.iter().map(flat).collect::<Vec<_>>().join(" ")),
        Sexp::Prefix(p, x) => format!("{p}{}", flat(x)),
    }
}

/// `s` laid out at `indent`: on one line when it fits in 80 columns,
/// else with its head on the first line and one item per line after.
pub(crate) fn layout(s: &Sexp, indent: usize) -> String {
    let one = flat(s);
    if indent + one.len() <= 80 {
        return one;
    }
    let (open, close, items) = match s {
        Sexp::List(xs) => ("(", ")", xs),
        Sexp::Vector(xs) => ("[", "]", xs),
        Sexp::Prefix(p, x) => return format!("{p}{}", layout(x, indent + p.len())),
        Sexp::Atom(_) => return one,
    };
    if items.is_empty() {
        return one;
    }
    let keep = match items.first() {
        Some(Sexp::Atom(h)) if h == "defun" => {
            let private = matches!(items.get(2), Some(Sexp::Atom(p)) if p == ":private");
            let at = 3 + usize::from(private);
            let bounded = matches!(items.get(at), Some(Sexp::Atom(w)) if w == ":where");
            5 + usize::from(private) + 2 * usize::from(bounded)
        }
        Some(Sexp::Atom(h)) if h == "impl" => 3,
        Some(Sexp::Atom(h))
            if ["let", "loop", "plet", "match", "fn", "if"].contains(&h.as_str()) =>
        {
            2
        }
        _ => 1,
    }
    .min(items.len());
    let head_width: usize = items[..keep].iter().map(|i| flat(i).len() + 1).sum();
    let keep = if indent + head_width <= 80 {
        keep
    } else {
        keep.min(2)
    };
    let (head, rest) = items.split_at(keep);
    let pad = " ".repeat(indent + 2);
    let first: Vec<String> = head.iter().map(flat).collect();
    let mut out = format!("{open}{}", first.join(" "));
    for item in rest {
        out.push('\n');
        out.push_str(&pad);
        out.push_str(&layout(item, indent + 2));
    }
    out.push_str(close);
    out
}

/// The type annotation text of `t` (for comments and reports).
pub fn ty_text(t: &Ty) -> String {
    t.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{FunDef, Param};

    #[test]
    fn prints_calls_derefs_and_inout() {
        let e = Expr::new(
            Ty::Unit,
            Kind::Call(
                "push!".into(),
                vec![Arg::InOut("c".into()), Arg::Val(Expr::int(3))],
            ),
        );
        assert_eq!(expr_text(&e), "(push! &c 3)");
        let d = Expr::new(
            Ty::Int,
            Kind::Deref(Box::new(Expr::var("c", Ty::cell(Ty::Int)))),
        );
        assert_eq!(expr_text(&d), "@c");
    }

    #[test]
    fn program_includes_only_named_preamble() {
        let f = FunDef {
            name: "f2".into(),
            params: vec![Param {
                name: "p".into(),
                ty: Ty::Pt,
                inout: false,
            }],
            ret: Ty::Int,
            body: Expr::new(
                Ty::Int,
                Kind::Field(Box::new(Expr::var("p", Ty::Pt)), "x".into()),
            ),
        };
        let p = Program {
            defs: Vec::new(),
            impls: Vec::new(),
            funs: vec![f],
            main: Expr::int(1),
        };
        let text = program(&p);
        assert!(
            text.starts_with("(defstruct Pt (x: i64 y: i64))\n\n(defun f2 (p: Pt) -> i64 (. p x))")
        );
        assert!(!text.contains("Wrap"));
    }

    #[test]
    fn long_forms_break_across_lines() {
        let long = Expr::new(Ty::Str, Kind::Str("a".repeat(90)));
        let e = Expr::call(Ty::Int, "str-len", vec![long]);
        assert!(expr_text(&e).contains('\n'));
    }
}
