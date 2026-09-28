//! Printing top-level items (helpers, `impl`s) and the pattern and
//! protocol forms: patterns, guarded `match`, `dyn`.

use crate::ast::{Clause, Expr, FunDef, ImplDef, Kind, Pat, Rest};
use crate::ty::Ty;

use super::{atom, expr, form, list, Sexp};

/// `(defun name (params) [:where ((P a))] -> ret body)`.
pub fn fundef(f: &FunDef) -> Sexp {
    let mut params = Vec::new();
    let mut bound = None;
    for p in &f.params {
        let name = if p.inout {
            format!("&{}:", p.name)
        } else {
            format!("{}:", p.name)
        };
        params.push(atom(name));
        params.push(atom(p.ty.to_string()));
        if let Ty::Gen(proto, true) = p.ty {
            bound = Some(proto);
        }
    }
    let mut items = vec![atom("defun"), atom(f.name.clone()), list(params)];
    if let Some(proto) = bound {
        let c = list(vec![atom(proto.name()), atom("a")]);
        items.push(atom(":where"));
        items.push(list(vec![c]));
    }
    items.extend([atom("->"), atom(f.ret.to_string()), expr(&f.body)]);
    list(items)
}

/// The head of an `impl`'s type: `Pt`, or `(Hook k)` for the colour-
/// parameterised struct (types §1.3: the colour is a name in the head).
fn impl_head(t: &Ty) -> Sexp {
    match t {
        Ty::Hook(_) => list(vec![atom("Hook"), atom("k")]),
        t => atom(t.to_string()),
    }
}

/// `(impl P T (m (self x ..) body) ..)`.
pub fn impl_def(i: &ImplDef) -> Sexp {
    let methods = i.methods.iter().map(|m| {
        let params = std::iter::once(atom("self"))
            .chain(m.params.iter().map(|(n, _)| atom(n.clone())))
            .collect();
        list(vec![atom(m.name.clone()), list(params), expr(&m.body)])
    });
    form(
        "impl",
        [atom(i.proto.name()), impl_head(&i.target)]
            .into_iter()
            .chain(methods),
    )
}

/// The forms of [`Kind::Dyn`] and [`Kind::GMatch`].
pub fn expr_new(e: &Expr) -> Sexp {
    match &e.kind {
        Kind::Dyn(p, send, x) => {
            let mut items = vec![atom("dyn"), atom(p.name())];
            if *send {
                items.push(atom(":send"));
            }
            items.push(expr(x));
            list(items)
        }
        Kind::GMatch(s, cl) => form("match", [expr(s)].into_iter().chain(cl.iter().map(clause))),
        Kind::Macro(m, args) => form(m.name(), args.iter().map(expr)),
        _ => atom("<unprintable>"),
    }
}

/// `(pat body)` or `(pat :when guard body)`.
fn clause(c: &Clause) -> Sexp {
    let mut items = vec![pat(&c.pat)];
    if let Some(g) = &c.guard {
        items.push(atom(":when"));
        items.push(expr(g));
    }
    items.push(expr(&c.body));
    list(items)
}

/// A pattern.
pub fn pat(p: &Pat) -> Sexp {
    match p {
        Pat::Wild => atom("_"),
        Pat::Bind(n) => atom(n.clone()),
        Pat::Nil => atom("nil"),
        Pat::Lit(n) => atom(n.to_string()),
        Pat::Some(q) => list(vec![atom("some"), pat(q)]),
        Pat::Ctor(c, ps) => list(
            [atom(c.clone())]
                .into_iter()
                .chain(ps.iter().map(pat))
                .collect(),
        ),
        Pat::As(q, n) => list(vec![pat(q), atom(":as"), atom(n.clone())]),
        Pat::Vector(ps, rest) => {
            let mut items: Vec<Sexp> = ps.iter().map(pat).collect();
            if let Some(r) = rest {
                items.push(atom("&"));
                items.push(match r {
                    Rest::Bind(n) => atom(n.clone()),
                    Rest::Wild => atom("_"),
                });
            }
            Sexp::Vector(items)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Method;
    use crate::print::{expr_text, layout};
    use crate::ty::Proto;

    #[test]
    fn prints_vector_patterns_guards_and_dyn() {
        let p = Pat::Vector(
            vec![Pat::Lit(0), Pat::Bind("x".into())],
            Some(Rest::Bind("r".into())),
        );
        let c = Clause {
            pat: p,
            guard: Some(Expr::new(Ty::Bool, Kind::Bool(true))),
            body: Expr::int(1),
        };
        let s = Expr::new(Ty::vec(Ty::Int), Kind::VecLit(Vec::new()));
        let m = Expr::new(Ty::Int, Kind::GMatch(Box::new(s), vec![c]));
        assert_eq!(expr_text(&m), "(match [] ([0 x & r] :when true 1))");
        let d = Expr::new(
            Ty::Dyn(Proto::Score, true),
            Kind::Dyn(Proto::Score, true, Box::new(Expr::int(1))),
        );
        assert_eq!(expr_text(&d), "(dyn Score :send 1)");
    }

    #[test]
    fn prints_impls_on_colour_parameterised_structs() {
        let i = ImplDef {
            proto: Proto::Score,
            target: Ty::Hook(false),
            methods: vec![Method {
                name: "score".into(),
                params: Vec::new(),
                body: Expr::int(2),
            }],
        };
        assert_eq!(
            layout(&impl_def(&i), 0),
            "(impl Score (Hook k) (score (self) 2))"
        );
    }
}
