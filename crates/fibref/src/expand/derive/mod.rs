//! `(derive P Name)` for `P` one of `Eq`, `Ord`, `Hash`, `Show`, `Debug`
//! and `ToStr` (§3.16, §4.4, stdlib §2.7), over the structs and enums
//! the context has seen.
//!
//! The head is `Name` applied to its parameters (bare when it has none);
//! `:where` lists `(P t)` for each parameter some field type mentions
//! (for `Ord` too: `(Ord t)` entails `(Eq t)` through the supertrait,
//! types §4.1), and is omitted when empty. The method bodies are built here with the call's position.
//!
//! Where §3.16 fixes the shape (the `Eq` examples for `Pair` and
//! `Shape`, the `Ord` rule, "combines the variant's index with the
//! hashes of its fields", "the variant name followed by the shown
//! fields, as a call form") the builders follow it; the concrete
//! choices it leaves open are:
//!
//! - `Ord`: `<` is the lexicographic comparison; `(<= a b)` is `(not (<
//!   b a))`, `(> a b)` is `(< b a)`, `(>= a b)` is `(not (< a b))`, the
//!   built-in `Ord`'s defaults written out (types §2.12); the instance
//!   needs an `Eq` instance of `Name`, `Ord`'s supertrait (types §4.1);
//! - `Hash`: `h := seed; h := (hash-combine h (hash field))` for each
//!   field in order, the seed being the variant index for an enum and
//!   `0` for a struct (a struct is its one variant); `hash-combine` is
//!   the prelude's rotate-and-xor mixer, built from `shl`, `shr`,
//!   `bit-or` and `bit-xor`, none of which traps (the `h*31 + x` this
//!   replaced trapped on integer overflow at two strings); it is
//!   written `fib.prelude/hash-combine` so that a binding of the name
//!   in the program cannot capture it;
//! - `Show`: `"(Name f1 f2)"` built with `str-concat` from `(show f)`;
//!   a field-less variant shows as its bare name, as it is written in
//!   an expression (§3.9);
//! - hygiene (stdlib design C-4): every function the bodies call (`=`,
//!   `<`, `not`, `hash`, `show`, `str-concat`) is written
//!   `fib.prelude/NAME`, as `hash-combine` is, so that a program's own
//!   `show` or `<` is not what a derived instance calls. The macros `and`
//!   and `or` and the core forms stay plain;
//! - `Debug` and `ToStr` (stdlib §2.7, L17; decided in the tranche 1 plan,
//!   R8): the text of a record, `#m.P{:x 1, :y "x"}`, `m` being the module
//!   the `derive` form is expanded in (the type table does not record the
//!   module a type was defined in, so a `derive` in another module than the
//!   type's prints the deriving module), each field's text the `debug` of
//!   the field. A variant with fields is such a record of the variant's
//!   name, `#m.Done{:v 3}`; a field written without a name is called by its
//!   position, `#m.W{:0 5}`; a variant with no field is its bare name.
//!   `ToStr` of a derived type is that same text, built from the fields'
//!   `Debug` (the page: a record's `str` is its `pr` text), so it needs
//!   no `Debug` instance of the type itself. The protocol and its methods
//!   are written with the facade's name, `fib.core/Debug`, `fib.core/debug`
//!   (a Debug is not the prelude's, so the head `fib.prelude/` does not
//!   reach it): the module needs `fib.core` in scope, which every module but
//!   the library's own has since the library modules are implicit. An enum none of whose variants
//!   has a field derives nothing for `Eq Ord Hash Show` (the compilers
//!   have those) but does derive `Debug` and `ToStr`, which they do not.

mod enums;
mod record;
mod structs;

use crate::syntax::{Form, Pos};

use super::build::{boolean, call, check_arity, int, keyword, list, string, sym};
use super::collections::prelude_name;
use super::ctx::ExpandCtx;
use super::error::{ExpandError, ExpandErrorKind};
use super::types::mentions;

/// The prelude's mixer, qualified so that no binding of the program's
/// can shadow it (as the collection literals' heads are, §1.4).
const HASH_COMBINE: &str = "fib.prelude/hash-combine";

/// A derivable protocol.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Proto {
    Eq,
    Ord,
    Hash,
    Show,
    Debug,
    ToStr,
}

impl Proto {
    fn parse(name: &str) -> Option<Proto> {
        match name {
            "Eq" => Some(Proto::Eq),
            "Ord" => Some(Proto::Ord),
            "Hash" => Some(Proto::Hash),
            "Show" => Some(Proto::Show),
            "Debug" => Some(Proto::Debug),
            "ToStr" => Some(Proto::ToStr),
            _ => None,
        }
    }

    /// The protocol as the `impl` names it: the library's two by the
    /// facade's name.
    fn head(self) -> &'static str {
        match self {
            Proto::Eq => "Eq",
            Proto::Ord => "Ord",
            Proto::Hash => "Hash",
            Proto::Show => "Show",
            Proto::Debug => "fib.core/Debug",
            Proto::ToStr => "fib.core/ToStr",
        }
    }

    /// What a type parameter must have for the instance: the protocol
    /// itself, except for `ToStr`, which is built from the fields' `Debug`.
    fn context(self) -> &'static str {
        match self {
            Proto::ToStr => Proto::Debug.head(),
            p => p.head(),
        }
    }

    /// Whether the compilers have the protocol for an enum with no field.
    fn builtin_for_fieldless(self) -> bool {
        !matches!(self, Proto::Debug | Proto::ToStr)
    }

    /// The name of the one method of `Show`, `Debug` and `ToStr`.
    fn text_method(self) -> &'static str {
        match self {
            Proto::Debug => "debug",
            Proto::ToStr => "to-str",
            _ => "show",
        }
    }
}

/// Expands `(derive P Name)`.
pub(crate) fn derive(ctx: &ExpandCtx, items: Vec<Form>, pos: &Pos) -> Result<Form, ExpandError> {
    check_arity("derive", &items, 2, Some(2), pos)?;
    let pname = items[1].as_sym().unwrap_or("");
    let Some(proto) = Proto::parse(pname) else {
        let name = items[1].to_string();
        return Err(ExpandError::new(
            ExpandErrorKind::DeriveProtocol { name },
            &items[1].pos,
        ));
    };
    let target = items[2].as_sym().unwrap_or("");
    if let Some(info) = ctx.struct_info(target) {
        return Ok(structs::derive(ctx, proto, info, pos));
    }
    if let Some(info) = ctx.enum_info(target) {
        let fieldless = info.variants.iter().all(|v| v.fields.is_empty());
        if fieldless && proto.builtin_for_fieldless() {
            return Ok(call("do", Vec::new(), pos));
        }
        return Ok(enums::derive(ctx, proto, info, pos));
    }
    let name = items[2].to_string();
    Err(ExpandError::new(
        ExpandErrorKind::DeriveTarget { name },
        &items[2].pos,
    ))
}

/// `(impl P head :where (...) methods...)`.
fn impl_form(
    proto: Proto,
    name: &str,
    params: &[Form],
    types: &[&Form],
    methods: Vec<Form>,
    pos: &Pos,
) -> Form {
    let mut out = vec![sym("impl", pos), sym(proto.head(), pos)];
    let param_names: Vec<&str> = params.iter().filter_map(Form::as_sym).collect();
    if param_names.is_empty() {
        out.push(sym(name, pos));
    } else {
        let mut head = vec![sym(name, pos)];
        head.extend(param_names.iter().map(|p| sym(p, pos)));
        out.push(list(head, pos));
    }
    let mut context = Vec::new();
    for p in param_names {
        if types.iter().any(|t| mentions(t, p)) {
            // `(Ord t)` entails `(Eq t)`, its supertrait (types §4.1).
            context.push(call(proto.context(), vec![sym(p, pos)], pos));
        }
    }
    if !context.is_empty() {
        out.push(keyword("where", pos));
        out.push(list(context, pos));
    }
    out.extend(methods);
    list(out, pos)
}

/// `(name (params...) body)`.
fn method(name: &str, params: &[&str], body: Form, pos: &Pos) -> Form {
    let params = params.iter().map(|p| sym(p, pos)).collect();
    list(vec![sym(name, pos), list(params, pos), body], pos)
}

/// `!=` from `=`.
fn not_equal(pos: &Pos) -> Form {
    let eq = call(
        &prelude_name("="),
        vec![sym("self", pos), sym("y", pos)],
        pos,
    );
    let not = call(&prelude_name("not"), vec![eq], pos);
    method("!=", &["self", "y"], not, pos)
}

/// `<=`, `>`, `>=` from `<`.
fn ord_rest(pos: &Pos) -> Vec<Form> {
    let less = |a: &str, b: &str| call(&prelude_name("<"), vec![sym(a, pos), sym(b, pos)], pos);
    let not = |f: Form| call(&prelude_name("not"), vec![f], pos);
    vec![
        method("<=", &["self", "y"], not(less("y", "self")), pos),
        method(">", &["self", "y"], less("y", "self"), pos),
        method(">=", &["self", "y"], not(less("self", "y")), pos),
    ]
}

/// The pairs compared with `=` and `and`ed: `true` for none, the one
/// comparison for one (as §3.16's `(= r r2)`).
fn eq_all(pairs: Vec<(Form, Form)>, pos: &Pos) -> Form {
    let mut tests: Vec<Form> = pairs
        .into_iter()
        .map(|(a, b)| call(&prelude_name("="), vec![a, b], pos))
        .collect();
    match tests.len() {
        0 => boolean(true, pos),
        1 => tests.pop().unwrap_or_else(|| boolean(true, pos)),
        _ => call("and", tests, pos),
    }
}

/// Lexicographic `<` over the pairs: `(or (< a1 b1) (and (= a1 b1) ...))`,
/// the last pair just `(< an bn)`, `false` for none.
fn lex_less(pairs: Vec<(Form, Form)>, pos: &Pos) -> Form {
    let mut acc: Option<Form> = None;
    for (a, b) in pairs.into_iter().rev() {
        let less = call(&prelude_name("<"), vec![a.clone(), b.clone()], pos);
        acc = Some(match acc {
            None => less,
            Some(rest) => {
                let equal = call(&prelude_name("="), vec![a, b], pos);
                let same = call("and", vec![equal, rest], pos);
                call("or", vec![less, same], pos)
            }
        });
    }
    acc.unwrap_or_else(|| boolean(false, pos))
}

/// `h := seed; h := (hash-combine h (hash f))` for each field.
fn combine(seed: i64, fields: Vec<Form>, pos: &Pos) -> Form {
    let mut acc = int(seed, pos);
    for f in fields {
        let hashed = call(&prelude_name("hash"), vec![f], pos);
        acc = call(HASH_COMBINE, vec![acc, hashed], pos);
    }
    acc
}

/// `"(Name " (show f1) " " (show f2) ")"` joined with `str-concat`, or
/// `"Name"` for no fields.
fn show_call(name: &str, fields: Vec<Form>, pos: &Pos) -> Form {
    if fields.is_empty() {
        return string(name, pos);
    }
    let mut pieces = Vec::new();
    for (i, f) in fields.into_iter().enumerate() {
        let text = if i == 0 {
            format!("({name} ")
        } else {
            " ".to_string()
        };
        pieces.push(string(&text, pos));
        pieces.push(call(&prelude_name("show"), vec![f], pos));
    }
    pieces.push(string(")", pos));
    record::concat(pieces, pos)
}
