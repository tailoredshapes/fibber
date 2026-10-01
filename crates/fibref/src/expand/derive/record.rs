//! The text of a record for `derive Debug` and `derive ToStr`
//! (stdlib §2.7): `#m.P{:x 1, :y "x"}`, built as a right-nested chain of
//! `fib.prelude/str-concat` over literal pieces and the fields' `debug`.

use crate::syntax::{Form, Pos};

use crate::expand::build::{call, string};
use crate::expand::collections::prelude_name;

/// The method every field's text comes from, by the facade's name: the
/// library's `Debug` is not the prelude's.
const DEBUG: &str = "fib.core/debug";

/// `(fib.core/debug f)`.
pub(super) fn debug_of(f: Form, pos: &Pos) -> Form {
    call(DEBUG, vec![f], pos)
}

/// What a field is called in the text: its name, or its position when it
/// was written without one.
pub(super) fn label(name: Option<&str>, index: usize) -> String {
    name.map_or_else(|| index.to_string(), str::to_string)
}

/// The pieces joined with `str-concat`, nested to the right: `(a (b c))`.
/// There is at least one piece.
pub(super) fn concat(pieces: Vec<Form>, pos: &Pos) -> Form {
    let mut it = pieces.into_iter().rev();
    let mut acc = it.next().unwrap_or_else(|| string("", pos));
    for p in it {
        acc = call(&prelude_name("str-concat"), vec![p, acc], pos);
    }
    acc
}

/// `#m.Name{:l1 v1, :l2 v2}` for the labels and the already built field
/// texts, which are the same in number; `#m.Name{}` for none.
pub(super) fn record_text(
    module: &str,
    name: &str,
    labels: Vec<String>,
    texts: Vec<Form>,
    pos: &Pos,
) -> Form {
    if texts.is_empty() {
        return string(&format!("#{module}.{name}{{}}"), pos);
    }
    let mut pieces = Vec::new();
    for (i, (label, text)) in labels.into_iter().zip(texts).enumerate() {
        let lead = if i == 0 {
            format!("#{module}.{name}{{:{label} ")
        } else {
            format!(", :{label} ")
        };
        pieces.push(string(&lead, pos));
        pieces.push(text);
    }
    pieces.push(string("}", pos));
    concat(pieces, pos)
}
