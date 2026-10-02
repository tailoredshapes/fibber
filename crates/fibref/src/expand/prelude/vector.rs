//! `vector`, `hash-set`, `array-map` and `hash-map`: the collection
//! constructors that are one rewrite each (stdlib §4.3, §4.8; tranche 2
//! X3).
//!
//! ```text
//! (vector a ..)      ⟹ [a ..]
//! (hash-set a ..)    ⟹ (fib.coll/conj (fib.coll/conj (fib.prelude/set-empty) a) ..)
//! (array-map k v ..) ⟹ {k v ..}
//! (hash-map k v ..)  ⟹ (fib.prelude/map-assoc (fib.prelude/map-assoc
//!                          (fib.prelude/map-empty-hashed) k v) ..)
//! ```
//!
//! An odd number of arguments to `array-map` or `hash-map` is `malformed
//! NAME: a key without a value`. `map-empty-hashed` is the hashed Map
//! constructor of P1: until it lands the expansion of `hash-map` does not
//! check. The function value `(map vector xs ys)` is L25's, not this
//! macro's.

use crate::syntax::{Form, FormKind, Pos};

use crate::expand::build::{call, malformed, vector};
use crate::expand::collections::prelude_name;
use crate::expand::error::ExpandError;

/// The arguments of the call, which must pair up.
fn pairs(name: &str, items: Vec<Form>, pos: &Pos) -> Result<Vec<Form>, ExpandError> {
    let args: Vec<Form> = items.into_iter().skip(1).collect();
    if args.len() % 2 == 1 {
        return Err(malformed(name, "a key without a value", pos));
    }
    Ok(args)
}

/// `(vector a ..)` is the literal.
pub(super) fn vector_macro(items: Vec<Form>, pos: &Pos) -> Form {
    vector(items.into_iter().skip(1).collect(), pos)
}

/// `(hash-set a ..)`: one `conj` per element over the empty set.
pub(super) fn hash_set(items: Vec<Form>, pos: &Pos) -> Form {
    let empty = call(&prelude_name("set-empty"), Vec::new(), pos);
    items
        .into_iter()
        .skip(1)
        .fold(empty, |set, a| call("fib.coll/conj", vec![set, a], pos))
}

/// `(array-map k v ..)` is the literal `{k v ..}`, the array shape.
pub(super) fn array_map(items: Vec<Form>, pos: &Pos) -> Result<Form, ExpandError> {
    Ok(Form::new(
        FormKind::Map(pairs("array-map", items, pos)?),
        pos.clone(),
    ))
}

/// `(hash-map k v ..)`: one `map-assoc` per pair over the empty hashed Map.
pub(super) fn hash_map(items: Vec<Form>, pos: &Pos) -> Result<Form, ExpandError> {
    let mut args = pairs("hash-map", items, pos)?.into_iter();
    let mut acc = call(&prelude_name("map-empty-hashed"), Vec::new(), pos);
    while let (Some(k), Some(v)) = (args.next(), args.next()) {
        acc = call(&prelude_name("map-assoc"), vec![acc, k, v], pos);
    }
    Ok(acc)
}
