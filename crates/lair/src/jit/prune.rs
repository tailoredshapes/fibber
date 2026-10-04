//! Dead `internal` and `private` definitions, dropped before a module is lowered.
//!
//! The compiler gives the JIT whole modules (the runtime and every function a program asks
//! for) of which a session calls a handful of exported entries. A definition that is
//! neither exported nor reachable from an exported one can never run, so it is not lowered,
//! optimised or compiled: the module a session compiles is the closure of its entries.
//! Structs, declarations and every exported definition are kept as they are.

use std::collections::{HashMap, HashSet};

use lir::ast::{Callee, Expr, Item, Kind, Module};

/// `m` without the `internal` and `private` functions and globals no exported definition
/// reaches (through calls, global references and initialisers).
pub fn dead_internals(m: &Module) -> Module {
    let defined: HashMap<&str, &Item> = m
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Define(f) => Some((f.name.as_str(), i)),
            Item::Global(g) => Some((g.name.as_str(), i)),
            _ => None,
        })
        .collect();
    let mut live: HashSet<&str> = HashSet::new();
    let mut work: Vec<&Item> = Vec::new();
    for item in &m.items {
        let exported = match item {
            Item::Define(f) => f.mods.linkage.exported(),
            Item::Global(g) => g.mods.linkage.exported(),
            _ => false,
        };
        if exported {
            work.push(item);
        }
    }
    while let Some(item) = work.pop() {
        let mut reach = |name: &str| {
            if let Some((&n, &it)) = defined.get_key_value(name) {
                if live.insert(n) {
                    work.push(it);
                }
            }
        };
        match item {
            Item::Define(f) => {
                for b in &f.blocks {
                    for e in &b.body {
                        visit(e, &mut reach);
                    }
                }
            }
            Item::Global(g) => visit(&g.init, &mut reach),
            _ => {}
        }
    }
    let items = m
        .items
        .iter()
        .filter(|i| match i {
            Item::Define(f) => f.mods.linkage.exported() || live.contains(f.name.as_str()),
            Item::Global(g) => g.mods.linkage.exported() || live.contains(g.name.as_str()),
            _ => true,
        })
        .cloned()
        .collect();
    Module { items }
}

/// Calls `reach` with every global name `e` mentions: a direct callee or a `@name` operand.
fn visit(e: &Expr, reach: &mut impl FnMut(&str)) {
    use Kind::*;
    match &e.kind {
        Global(n) => reach(n),
        Local(_) | Int(..) | Float(..) | Null | Str(_) | Zero(_) | Fence(..) | Trap
        | Unreachable | Br(_) | Ret(None) => {}
        Vector(_, es) | Struct(_, es) | Array(_, es) => es.iter().for_each(|x| visit(x, reach)),
        Un(_, a)
        | Cast(_, _, a)
        | ExtractValue(a, _)
        | AtomicLoad(_, _, _, a)
        | Load { ptr: a, .. }
        | Ret(Some(a))
        | CondBr(a, _, _) => visit(a, reach),
        Alloca { count, .. } => {
            if let Some(c) = count {
                visit(c, reach);
            }
        }
        Bin(_, a, b)
        | Overflow(_, a, b)
        | ICmp(_, a, b)
        | FCmp(_, a, b)
        | ExtractElement(a, b)
        | InsertValue(a, b, _)
        | AtomicStore(_, _, a, b)
        | AtomicRmw(_, _, _, a, b)
        | Store {
            value: a, ptr: b, ..
        } => {
            visit(a, reach);
            visit(b, reach);
        }
        Select(a, b, c) | InsertElement(a, b, c) | Shuffle(a, b, c) => {
            visit(a, reach);
            visit(b, reach);
            visit(c, reach);
        }
        Gep { ptr, indices, .. } => {
            visit(ptr, reach);
            indices.iter().for_each(|x| visit(x, reach));
        }
        CmpXchg {
            ptr, expected, new, ..
        } => {
            visit(ptr, reach);
            visit(expected, reach);
            visit(new, reach);
        }
        Call { callee, args, .. } => {
            match callee {
                Callee::Direct(n) => reach(n),
                Callee::Indirect(p, _) => visit(p, reach),
            }
            args.iter().for_each(|x| visit(x, reach));
        }
        Switch(v, _, cases) => {
            visit(v, reach);
            cases.iter().for_each(|(c, _)| visit(c, reach));
        }
        Phi(_, inc) => inc.iter().for_each(|b| visit(&b.value, reach)),
        Let(binds, body) => {
            binds.iter().for_each(|b| visit(&b.value, reach));
            body.iter().for_each(|x| visit(x, reach));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kept(src: &str) -> Vec<String> {
        let m = lir::parse_and_check(src).unwrap();
        dead_internals(&m)
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Define(f) => Some(f.name.clone()),
                Item::Global(g) => Some(g.name.clone()),
                _ => None,
            })
            .collect()
    }

    const SRC: &str = "
      (global internal g.used i64 (i64 1))
      (global internal g.dead i64 (i64 2))
      (global internal g.via-init ptr @g.used)
      (define internal (leaf i64) () (block entry (ret (load i64 (load ptr @g.via-init)))))
      (define internal (mid i64) () (block entry (ret (call @leaf))))
      (define private (dead i64) () (block entry (ret (call @mid))))
      (define internal (by-pointer i64) () (block entry (ret (i64 3))))
      (define (entry i64) () (block entry (ret (call @mid))))
      (define (table ptr) () (block entry (ret @by-pointer)))
      (define (also-exported i64) () (block entry (ret (i64 0))))";

    #[test]
    fn exported_definitions_and_what_they_reach_stay() {
        let k = kept(SRC);
        for n in [
            "entry",
            "table",
            "also-exported",
            "mid",
            "leaf",
            "by-pointer",
            "g.via-init",
        ] {
            assert!(k.iter().any(|x| x == n), "{n} must stay: {k:?}");
        }
    }

    #[test]
    fn a_definition_nothing_exported_reaches_goes() {
        let k = kept(SRC);
        for n in ["dead", "g.dead"] {
            assert!(!k.iter().any(|x| x == n), "{n} must go: {k:?}");
        }
        // g.used is reached only through g.via-init's initialiser: a chain of globals stays.
        assert!(k.iter().any(|x| x == "g.used"), "{k:?}");
    }

    #[test]
    fn a_module_with_no_exports_keeps_no_definitions() {
        let k = kept("(define internal (f i64) () (block entry (ret (i64 1))))");
        assert!(k.is_empty(), "{k:?}");
    }
}
