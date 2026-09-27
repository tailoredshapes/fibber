//! `derive` on a struct: methods field by field over the struct's
//! fields, e.g. for `(defstruct Pair (a b))` (§3.16)
//!
//! ```text
//! (impl Eq (Pair a b) :where ((Eq a) (Eq b))
//!   (= (self y) (and (= (. self a) (. y a)) (= (. self b) (. y b))))
//!   (!= (self y) (not (= self y))))
//! ```

use crate::syntax::{Form, Pos};

use super::{combine, eq_all, impl_form, lex_less, method, not_equal, ord_rest, show_call, Proto};
use crate::expand::build::{call, sym};
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

/// The `impl` for `proto` on the struct `info`.
pub(super) fn derive(proto: Proto, info: &StructInfo, pos: &Pos) -> Form {
    let selfs = || {
        info.fields
            .iter()
            .map(|(f, _)| field_of("self", f, pos))
            .collect()
    };
    let methods = match proto {
        Proto::Eq => vec![
            method("=", &["self", "y"], eq_all(pairs(info, pos), pos), pos),
            not_equal(pos),
        ],
        Proto::Ord => {
            let mut m = vec![method(
                "<",
                &["self", "y"],
                lex_less(pairs(info, pos), pos),
                pos,
            )];
            m.extend(ord_rest(pos));
            m
        }
        Proto::Hash => vec![method("hash", &["self"], combine(0, selfs(), pos), pos)],
        Proto::Show => vec![method(
            "show",
            &["self"],
            show_call(&info.name, selfs(), pos),
            pos,
        )],
    };
    let types: Vec<&Form> = info.fields.iter().map(|(_, t)| t).collect();
    impl_form(proto, &info.name, &info.params, &types, methods, pos)
}
