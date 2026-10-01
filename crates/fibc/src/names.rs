//! Names of emitted things (spec/compiler.md §2): a fibber type's
//! mangled form and the names of specialisations, methods, closures
//! and object structs.

use fibref::own::program::BodyKey;
use fibref::types::decls::{Globals, ModuleId};
use fibref::types::display::Printer;
use fibref::types::ty::{Colour, ProtoId, Ty};

/// The mangled form of a type: `(` is `$`, `)` is `_`, a space is `.`.
/// Colours are erased first, since they have no representation. A
/// nominal type (and the protocol of a `dyn`) is written behind the
/// module that defines it and a `/` (none for the main module), so that
/// two modules' types of one name, each a different object layout with
/// its own methods, have different symbols: `$Vec.i64_` of the prelude
/// is `$fib.prelude/Vec.i64_`, a main module's own `Pt` is `Pt`, and
/// `a`'s is `a/Pt`. The `/` (not the `.` of the protocols' prefix) keeps
/// a module's type from reading as a type of the main module applied to
/// arguments: `M.T.x` would be both module `M`'s `T` at `x` and `M` at
/// `T` and `x`, and a type's name need not differ in case from a module's.
pub fn mangle(g: &Globals, t: &Ty) -> String {
    let t = t.map_colours(&mut |_| Colour::Local);
    let prefix = |m: ModuleId| {
        let p = module_prefix(g, m);
        match p.strip_suffix('.') {
            Some(q) => format!("{q}/"),
            None => p,
        }
    };
    let text = Printer::qualified(g, &prefix).ty(&t);
    text.chars()
        .map(|c| match c {
            '(' => '$',
            ')' => '_',
            ' ' => '.',
            c => c,
        })
        .collect()
}

/// The prefix a definition of module `m` carries: none in the main
/// module, else its `ns` and a dot, so that two modules' `f` differ.
fn module_prefix(g: &Globals, m: ModuleId) -> String {
    if m == g.main {
        String::new()
    } else if m == ModuleId::BUILTIN {
        // Its name is for messages ("the builtins"); the symbol has no
        // spaces. The protocols of the checker (`Eq`, `Show`, ..) are its.
        "fib.builtin.".to_string()
    } else {
        format!("{}.", g.module_name(m))
    }
}

/// A protocol's name as a symbol: its own, behind its module's prefix
/// (none for the main module), so that two modules' protocols of one
/// name, each with a method of one name, have different methods' symbols
/// (a user `Collection` with a `conj`, and the prelude's).
pub fn proto_qualified(g: &Globals, p: ProtoId) -> String {
    let proto = g.proto(p);
    format!("{}{}", module_prefix(g, proto.module), proto.name)
}

/// The name of a body's specialisation at `tys`.
pub fn body_name(g: &Globals, key: BodyKey, tys: &[Ty]) -> String {
    let mut name = match key {
        BodyKey::Fun(f) => {
            let d = g.fun(f);
            format!("f.{}{}", module_prefix(g, d.module), d.name)
        }
        BodyKey::AllOwned(f) => {
            let d = g.fun(f);
            format!("f.{}{}.owned", module_prefix(g, d.module), d.name)
        }
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
                proto_qualified(g, inst.proto),
                method,
                mangle(g, &inst.head)
            )
        }
        BodyKey::Def(d) => {
            let def = g.def(d);
            format!("d.{}{}", module_prefix(g, def.module), def.name)
        }
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
        assert_eq!(mangle(g, &t2), "$fib.prelude/Vec.i64_");
        assert_eq!(mangle(g, &t1), "$fib.prelude/Vec.$fib.prelude/Vec.i64__");
        assert_ne!(mangle(g, &t1), mangle(g, &t2));
        assert_eq!(mangle(g, &Ty::str()), "str");
    }

    /// A type is written behind the module that defines it, none for
    /// the main module, so a main module's `Box` and the prelude's differ
    /// (as do two modules' types of one name: cases/modules/025).
    #[test]
    fn a_type_is_mangled_behind_its_module() {
        let checked = fibref::own::check_source(
            "(defstruct (Box a) (v: a w: a)) (defun main () -> i64 1)",
            "t",
        )
        .expect("checks");
        let g = &checked.typed.globals;
        let ours = g.type_name(g.main, "Box").expect("the program's Box");
        let theirs = g
            .type_name(fibref::types::decls::ModuleId::PRELUDE, "Box")
            .expect("the prelude's Box");
        assert_ne!(ours, theirs);
        assert_eq!(mangle(g, &Ty::nominal(ours, vec![Ty::i64()])), "$Box.i64_");
        assert_eq!(
            mangle(g, &Ty::nominal(theirs, vec![Ty::i64()])),
            "$fib.prelude/Box.i64_"
        );
    }
}
