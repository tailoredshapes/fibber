//! `derive` on a struct: methods field by field over the struct's
//! fields, e.g. for `(defstruct Pair (a b))` (§3.16)
//!
//! ```text
//! (impl Eq (Pair a b) :where ((Eq a) (Eq b))
//!   (= (self y) (and (= (. self a) (. y a)) (= (. self b) (. y b))))
//!   (!= (self y) (not (= self y))))
//! ```

use crate::syntax::{Form, Pos};

use super::record::{debug_of, label, record_text};
use super::{combine, eq_all, impl_form, lex_less, method, not_equal, ord_rest, show_call, Proto};
use crate::expand::build::{call, sym};
use crate::expand::ctx::ExpandCtx;
use crate::expand::types::StructInfo;

/// `(. who field)`.
fn field_of(who: &str, field: &Form, pos: &Pos) -> Form {
    let name = field.as_sym().unwrap_or("");
    call(".", vec![sym(who, pos), sym(name, pos)], pos)
}

/// `((. self f) (. y f))` for each field.
fn pairs(info: &StructInfo, pos: &Pos) -> Vec<(Form, Form)> {
    info.fields
        .iter()
        .map(|(f, _)| (field_of("self", f, pos), field_of("y", f, pos)))
        .collect()
}

/// `#m.P{:x .., :y ..}`, `m` the module being expanded: the text of
/// `Debug` and `ToStr`.
fn record(ctx: &ExpandCtx, info: &StructInfo, pos: &Pos) -> Form {
    let names = info.fields.iter().enumerate();
    let labels = names.map(|(i, (f, _))| label(f.as_sym(), i)).collect();
    let texts = info
        .fields
        .iter()
        .map(|(f, _)| debug_of(field_of("self", f, pos), pos))
        .collect();
    record_text(&ctx.scope().ns, &info.name, labels, texts, pos)
}

/// The methods of `proto` on the struct `info`.
fn methods(ctx: &ExpandCtx, proto: Proto, info: &StructInfo, pos: &Pos) -> Vec<Form> {
    let selfs = || {
        info.fields
            .iter()
            .map(|(f, _)| field_of("self", f, pos))
            .collect()
    };
    match proto {
        Proto::Eq => vec![
            method("=", &["self", "y"], eq_all(pairs(info, pos), pos), pos),
            not_equal(pos),
        ],
        Proto::Ord => {
            let less = lex_less(pairs(info, pos), pos);
            let mut m = vec![method("<", &["self", "y"], less, pos)];
            m.extend(ord_rest(pos));
            m
        }
        Proto::Hash => vec![method("hash", &["self"], combine(0, selfs(), pos), pos)],
        Proto::Show => {
            let text = show_call(&info.name, selfs(), pos);
            vec![method("show", &["self"], text, pos)]
        }
        Proto::Debug | Proto::ToStr => {
            let text = record(ctx, info, pos);
            vec![method(proto.text_method(), &["self"], text, pos)]
        }
    }
}

/// The `impl` for `proto` on the struct `info`.
pub(super) fn derive(ctx: &ExpandCtx, proto: Proto, info: &StructInfo, pos: &Pos) -> Form {
    let methods = methods(ctx, proto, info, pos);
    let types: Vec<&Form> = info.fields.iter().map(|(_, t)| t).collect();
    impl_form(proto, &info.name, &info.params, &types, methods, pos)
}
