//! §1.4: literal collections in expression position become library
//! calls after expansion.
//!
//! ```text
//! []              ⟹ (vec-empty)
//! [e1 e2 ... en]  ⟹ (conj (conj ... (conj (vec-empty) e1) ...) en)
//! {}              ⟹ (map-empty)
//! {k1 v1 ...}     ⟹ (assoc (assoc (map-empty) k1 v1) ...)
//! ```
//!
//! §1.4: "the rewrite resolves them in the prelude, not in the current
//! namespace". A form can only carry that as a qualified symbol, so the
//! heads are written `fib.prelude/conj` etc. ([`PRELUDE_NS`]), which a
//! user binding of `conj` cannot shadow. The built calls take the
//! position of the literal; the elements keep their own (§1.3).

use crate::syntax::{Form, Pos};

use super::build::call;

/// The namespace the §1.4 rewrite qualifies its heads with.
pub const PRELUDE_NS: &str = "fib.prelude";

/// `fib.prelude/name`: a head no user binding of `name` can shadow.
pub(crate) fn prelude_name(name: &str) -> String {
    format!("{PRELUDE_NS}/{name}")
}

/// The rewrite of `[items...]`, the items already expanded.
pub(crate) fn vec_literal(items: Vec<Form>, pos: &Pos) -> Form {
    let conj = prelude_name("conj");
    let mut acc = call(&prelude_name("vec-empty"), Vec::new(), pos);
    for item in items {
        acc = call(&conj, vec![acc, item], pos);
    }
    acc
}

/// The rewrite of `{k1 v1 ...}`, the items already expanded. The reader
/// guarantees an even count; a trailing odd item (only a macro can build
/// one) is paired with nothing and dropped by `chunks_exact`, so callers
/// check the count first.
pub(crate) fn map_literal(items: Vec<Form>, pos: &Pos) -> Form {
    let assoc = prelude_name("assoc");
    let mut acc = call(&prelude_name("map-empty"), Vec::new(), pos);
    let mut it = items.into_iter();
    while let (Some(k), Some(v)) = (it.next(), it.next()) {
        acc = call(&assoc, vec![acc, k, v], pos);
    }
    acc
}
