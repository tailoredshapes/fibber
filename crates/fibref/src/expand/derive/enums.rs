//! `derive` on an enum: a `match` on `self` with, for `Eq` and `Ord`, a
//! nested `match` on `y`, pattern variables gensyms (§3.16). For
//! `(defenum Shape (Circle r: f64) (Rect w: f64 h: f64))`:
//!
//! ```text
//! (impl Eq Shape
//!   (= (self y)
//!     (match self
//!       ((Circle r) (match y ((Circle r2) (= r r2)) (_ false)))
//!       ((Rect w h) (match y ((Rect w2 h2) (and (= w w2) (= h h2))) (_ false)))))
//!   (!= (self y) (not (= self y))))
//! ```

use crate::syntax::{Form, Pos};

use super::{combine, eq_all, impl_form, lex_less, method, not_equal, ord_rest, show_call, Proto};
use crate::expand::build::{boolean, list, sym};
use crate::expand::ctx::ExpandCtx;
use crate::expand::types::{EnumInfo, VariantInfo};

/// Fresh pattern variables for a variant's fields, named after the
/// fields (`x` when unnamed) with `suffix` appended.
fn vars(ctx: &ExpandCtx, v: &VariantInfo, suffix: &str, pos: &Pos) -> Vec<Form> {
    v.fields
        .iter()
        .map(|(name, _)| ctx.gensym(&format!("{}{suffix}", name.as_deref().unwrap_or("x")), pos))
        .collect()
}

/// `(Variant pats...)`.
fn pattern(v: &VariantInfo, pats: Vec<Form>, pos: &Pos) -> Form {
    let mut items = vec![sym(&v.name, pos)];
    items.extend(pats);
    list(items, pos)
}

/// `(Variant _ ...)`.
fn wild(v: &VariantInfo, pos: &Pos) -> Form {
    pattern(v, v.fields.iter().map(|_| sym("_", pos)).collect(), pos)
}

/// `(match who clauses...)`.
fn match_on(who: &str, clauses: Vec<Form>, pos: &Pos) -> Form {
    let mut items = vec![sym("match", pos), sym(who, pos)];
    items.extend(clauses);
    list(items, pos)
}

/// `(pat body)`.
fn clause(pat: Form, body: Form, pos: &Pos) -> Form {
    list(vec![pat, body], pos)
}

/// The body of `=`.
fn eq_body(ctx: &ExpandCtx, info: &EnumInfo, pos: &Pos) -> Form {
    let many = info.variants.len() > 1;
    let clauses = info.variants.iter().map(|v| {
        let xs = vars(ctx, v, "", pos);
        let ys = vars(ctx, v, "2", pos);
        let pairs = xs.iter().cloned().zip(ys.iter().cloned()).collect();
        let mut inner = vec![clause(pattern(v, ys, pos), eq_all(pairs, pos), pos)];
        if many {
            inner.push(clause(sym("_", pos), boolean(false, pos), pos));
        }
        clause(pattern(v, xs, pos), match_on("y", inner, pos), pos)
    });
    match_on("self", clauses.collect(), pos)
}

/// The body of `<`: declaration order of the variants, then the fields
/// lexicographically; the inner match lists every variant of `y`.
fn less_body(ctx: &ExpandCtx, info: &EnumInfo, pos: &Pos) -> Form {
    let clauses = info.variants.iter().enumerate().map(|(i, v)| {
        let xs = vars(ctx, v, "", pos);
        let inner = info.variants.iter().enumerate().map(|(j, w)| {
            if j != i {
                return clause(wild(w, pos), boolean(j > i, pos), pos);
            }
            let ys = vars(ctx, v, "2", pos);
            let pairs = xs.iter().cloned().zip(ys.iter().cloned()).collect();
            clause(pattern(v, ys, pos), lex_less(pairs, pos), pos)
        });
        let inner = match_on("y", inner.collect(), pos);
        clause(pattern(v, xs, pos), inner, pos)
    });
    match_on("self", clauses.collect(), pos)
}

/// The body of `hash` or `show`: one clause per variant over its fields.
fn per_variant(ctx: &ExpandCtx, info: &EnumInfo, proto: Proto, pos: &Pos) -> Form {
    let clauses = info.variants.iter().enumerate().map(|(i, v)| {
        let xs = vars(ctx, v, "", pos);
        let body = match proto {
            Proto::Hash => combine(i as i64, xs.clone(), pos),
            _ => show_call(&v.name, xs.clone(), pos),
        };
        clause(pattern(v, xs, pos), body, pos)
    });
    match_on("self", clauses.collect(), pos)
}

/// The `impl` for `proto` on the enum `info`, which has at least one
/// variant with fields.
pub(super) fn derive(ctx: &ExpandCtx, proto: Proto, info: &EnumInfo, pos: &Pos) -> Form {
    let methods = match proto {
        Proto::Eq => vec![
            method("=", &["self", "y"], eq_body(ctx, info, pos), pos),
            not_equal(pos),
        ],
        Proto::Ord => {
            let mut m = vec![method("<", &["self", "y"], less_body(ctx, info, pos), pos)];
            m.extend(ord_rest(pos));
            m
        }
        Proto::Hash => vec![method(
            "hash",
            &["self"],
            per_variant(ctx, info, proto, pos),
            pos,
        )],
        Proto::Show => vec![method(
            "show",
            &["self"],
            per_variant(ctx, info, proto, pos),
            pos,
        )],
    };
    let types: Vec<&Form> = info
        .variants
        .iter()
        .flat_map(|v| v.fields.iter().map(|(_, t)| t))
        .collect();
    impl_form(proto, &info.name, &info.params, &types, methods, pos)
}
