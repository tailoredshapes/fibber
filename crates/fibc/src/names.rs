//! Names of emitted things (spec/compiler.md §2): a fibber type's
//! mangled form and the names of specialisations, methods, closures
//! and object structs.

use fibref::own::program::BodyKey;
use fibref::types::decls::Globals;
use fibref::types::display::Printer;
use fibref::types::ty::{Colour, Ty};

/// The mangled form of a type: `(` is `$`, `)` is `_`, a space is `.`.
/// Colours are erased first, since they have no representation.
pub fn mangle(g: &Globals, t: &Ty) -> String {
    let t = t.map_colours(&mut |_| Colour::Local);
    let text = Printer::with_names(g, &[], &[]).ty(&t);
    text.chars()
        .map(|c| match c {
            '(' => '$',
            ')' => '_',
            ' ' => '.',
            c => c,
        })
        .collect()
}

/// The name of a body's specialisation at `tys`.
pub fn body_name(g: &Globals, key: BodyKey, tys: &[Ty]) -> String {
    let mut name = match key {
        BodyKey::Fun(f) => format!("f.{}", g.fun(f).name),
        BodyKey::AllOwned(f) => format!("f.{}.owned", g.fun(f).name),
        BodyKey::Method(i, m) | BodyKey::MethodOwned(i, m) => {
            let inst = &g.instances[i];
            let proto = g.proto(inst.proto);
            let method = &proto.methods[inst.methods[m].index].name;
            let owned = if matches!(key, BodyKey::MethodOwned(..)) {
                ".owned"
            } else {
                ""
            };
            format!(
                "m.{}.{}.{}{owned}",
                proto.name,
                method,
                mangle(g, &inst.head)
            )
        }
        BodyKey::Def(d) => format!("d.{}", g.def(d).name),
    };
    for t in tys {
        name.push('.');
        name.push_str(&mangle(g, t));
    }
    name
}

/// The name of the closure code for literal `lit` inside body `owner`.
pub fn closure_name(owner: &str, lit: u32) -> String {
    format!("l.{owner}.{lit}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mangling_is_injective_on_nested_types() {
        let checked = fibref::own::check_source("(defun main () -> i64 1)", "t").expect("checks");
        let g = &checked.typed.globals;
        let vec = g.vec.expect("Vec");
        let t1 = Ty::nominal(vec, vec![Ty::nominal(vec, vec![Ty::i64()])]);
        let t2 = Ty::nominal(vec, vec![Ty::i64()]);
        assert_eq!(mangle(g, &t2), "$Vec.i64_");
        assert_eq!(mangle(g, &t1), "$Vec.$Vec.i64__");
        assert_ne!(mangle(g, &t1), mangle(g, &t2));
        assert_eq!(mangle(g, &Ty::str()), "str");
    }
}
