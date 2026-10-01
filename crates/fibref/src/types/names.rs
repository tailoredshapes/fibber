//! The names each module binds and how a name used in a module is
//! resolved (syntax §5): the chain builtins → prelude → `:use`s →
//! program (a later one shadows an earlier), a qualified `fib.prelude/x`,
//! `:private` definitions, which no other module sees except through
//! `(var m/x)` (syntax §3.20), the modules a module re-exports
//! (`:export-from`), and the error when two `:use`d modules export
//! different definitions of one name that is used bare.

use std::collections::{HashMap, HashSet};

use super::ast::GlobalRef;
use super::decls::{Globals, ModuleId};
use super::ty::{ProtoId, TypeId};

/// The names one module binds.
#[derive(Clone, Debug, Default)]
pub struct Names {
    /// Functions, constructors, variants, methods, builtins, defs.
    pub values: HashMap<String, GlobalRef>,
    /// Structs and enums.
    pub types: HashMap<String, TypeId>,
    /// Protocols.
    pub protos: HashMap<String, ProtoId>,
    /// The names of each space that are `:private` (syntax §5): not
    /// visible from any other module.
    pub private: [HashSet<String>; 3],
}

/// The three spaces of names a module binds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Space {
    /// Values: functions, constructors, variants, methods, `def`s.
    Value,
    /// Structs and enums.
    Type,
    /// Protocols.
    Proto,
}

impl Space {
    fn index(self) -> usize {
        match self {
            Space::Value => 0,
            Space::Type => 1,
            Space::Proto => 2,
        }
    }
}

impl Names {
    /// Whether `name` is private in `space`.
    pub fn is_private(&self, space: Space, name: &str) -> bool {
        self.private[space.index()].contains(name)
    }

    /// Makes `name` private in `space`.
    pub fn make_private(&mut self, space: Space, name: &str) {
        self.private[space.index()].insert(name.to_string());
    }
}

/// What a name resolved to along a module's chain.
enum Found<T> {
    /// No module of the chain binds it (visibly).
    Nowhere,
    /// One definition.
    One(T),
    /// Two `:use`d modules export different definitions of it (syntax
    /// §5: an error when it is referenced unqualified).
    Twice(ModuleId, ModuleId),
}

impl Globals {
    /// The modules a name used in `m` is looked up in, in order.
    pub fn chain(&self, m: ModuleId) -> &[ModuleId] {
        &self.module(m).chain
    }

    /// The module whose chain a name used in `m` is looked up in, with
    /// the name itself: `ns/x` is looked up in `ns`'s chain, where `ns`
    /// is an alias or the full name of a module `m` requires or uses,
    /// or the prelude's name from anywhere.
    fn lookup<'n>(&self, m: ModuleId, name: &'n str) -> (ModuleId, &'n str) {
        let Some((ns, base)) = name.split_once('/').filter(|(_, b)| !b.is_empty()) else {
            return (m, name);
        };
        match self.module(m).aliases.get(ns) {
            Some(t) => (*t, base),
            None if ns == crate::expand::PRELUDE_NS => (ModuleId::PRELUDE, base),
            None => (m, name),
        }
    }

    /// Whether a definition of `owner` named `base` in `space` is
    /// visible from `m`: always in its own module, else unless private
    /// (syntax §5).
    fn visible(&self, m: ModuleId, owner: ModuleId, space: Space, base: &str) -> bool {
        owner == m || !self.names(owner).is_private(space, base)
    }

    /// What module `o` exports under `base` to the module `from`: its own
    /// definition, else what the modules it re-exports (`:export-from`)
    /// export, which must not disagree (syntax §5) any more than two
    /// `:use`d modules may. `private_ok`: `(var m/x)` sees private
    /// definitions too.
    fn exported<T: Copy + PartialEq>(
        &self,
        (from, o): (ModuleId, ModuleId),
        (space, base, private_ok): (Space, &str, bool),
        get: &impl Fn(&Names, &str) -> Option<T>,
    ) -> Found<T> {
        if private_ok || self.visible(from, o, space, base) {
            if let Some(t) = get(self.names(o), base) {
                return Found::One(t);
            }
        }
        let mut first: Option<(ModuleId, T)> = None;
        for &r in &self.module(o).reexports {
            match self.exported((from, r), (space, base, private_ok), get) {
                Found::One(t) => match first {
                    Some((f, other)) if other != t => return Found::Twice(f, r),
                    Some(_) => {}
                    None => first = Some((r, t)),
                },
                twice @ Found::Twice(..) => return twice,
                Found::Nowhere => {}
            }
        }
        first.map_or(Found::Nowhere, |(_, t)| Found::One(t))
    }

    /// Resolves `name`, used in module `m`, in `space` (`get` reads one
    /// module's binding of a base name; `private_ok`: `(var m/x)` sees
    /// private definitions too). The module's own definition shadows
    /// every other; else what its `:use`d modules export, which must not
    /// disagree (syntax §5), shadows the prelude and the builtins.
    fn find<T: Copy + PartialEq>(
        &self,
        (m, name): (ModuleId, &str),
        (space, private_ok): (Space, bool),
        get: impl Fn(&Names, &str) -> Option<T>,
    ) -> Found<T> {
        let (owner, base) = self.lookup(m, name);
        let info = self.module(owner);
        let own = |o: ModuleId| {
            let seen = private_ok || self.visible(m, o, space, base);
            seen.then(|| get(self.names(o), base)).flatten()
        };
        if let Some(t) = own(owner) {
            return Found::One(t);
        }
        let mut first: Option<(ModuleId, T)> = None;
        for &u in &info.uses {
            match self.exported((m, u), (space, base, private_ok), &get) {
                Found::One(t) => match first {
                    Some((f, other)) if other != t => return Found::Twice(f, u),
                    Some(_) => {}
                    None => first = Some((u, t)),
                },
                twice @ Found::Twice(..) => return twice,
                Found::Nowhere => {}
            }
        }
        if let Some((_, t)) = first {
            return Found::One(t);
        }
        info.chain[1 + info.uses.len()..]
            .iter()
            .find_map(|o| own(*o))
            .map_or(Found::Nowhere, Found::One)
    }

    fn values(&self, m: ModuleId, name: &str, private_ok: bool) -> Found<GlobalRef> {
        let get = |n: &Names, b: &str| n.values.get(b).copied();
        self.find((m, name), (Space::Value, private_ok), get)
    }

    fn types(&self, m: ModuleId, name: &str) -> Found<TypeId> {
        let get = |n: &Names, b: &str| n.types.get(b).copied();
        self.find((m, name), (Space::Type, false), get)
    }

    fn protos(&self, m: ModuleId, name: &str) -> Found<ProtoId> {
        let get = |n: &Names, b: &str| n.protos.get(b).copied();
        self.find((m, name), (Space::Proto, false), get)
    }

    /// Resolves a value name used in module `m`; a private value of
    /// another module is not visible (syntax §5), and a name that two
    /// `:use`d modules export differently is not resolved (see
    /// [`Globals::unknown`]).
    pub fn value(&self, m: ModuleId, name: &str) -> Option<GlobalRef> {
        match self.values(m, name, false) {
            Found::One(r) => Some(r),
            _ => None,
        }
    }

    /// Resolves a value name as `(var name)` does (syntax §3.20):
    /// private values of other modules included.
    pub fn value_any(&self, m: ModuleId, name: &str) -> Option<GlobalRef> {
        match self.values(m, name, true) {
            Found::One(r) => Some(r),
            _ => None,
        }
    }

    /// Resolves a type name used in module `m` (private ones of other
    /// modules are not visible).
    pub fn type_name(&self, m: ModuleId, name: &str) -> Option<TypeId> {
        match self.types(m, name) {
            Found::One(t) => Some(t),
            _ => None,
        }
    }

    /// Resolves a protocol name used in module `m` (private ones of
    /// other modules are not visible).
    pub fn proto_name(&self, m: ModuleId, name: &str) -> Option<ProtoId> {
        match self.protos(m, name) {
            Found::One(p) => Some(p),
            _ => None,
        }
    }

    /// The two `:use`d modules that export different definitions of
    /// `name` in `space`, which make it an error to use it from `m`
    /// unqualified (syntax §5).
    pub fn ambiguity(&self, m: ModuleId, space: Space, name: &str) -> Option<(ModuleId, ModuleId)> {
        fn twice<T>(f: Found<T>) -> Option<(ModuleId, ModuleId)> {
            match f {
                Found::Twice(a, b) => Some((a, b)),
                _ => None,
            }
        }
        match space {
            Space::Value => twice(self.values(m, name, false)),
            Space::Type => twice(self.types(m, name)),
            Space::Proto => twice(self.protos(m, name)),
        }
    }

    /// The module whose private definition `name` in `space` would have
    /// resolved a name used in `m` that did not resolve, for the
    /// message `x is private to M`.
    pub fn private_owner(&self, m: ModuleId, space: Space, name: &str) -> Option<ModuleId> {
        let (owner, base) = self.lookup(m, name);
        self.chain(owner)
            .iter()
            .copied()
            .find(|o| *o != m && self.names(*o).is_private(space, base))
    }

    /// `unbound name x`; `x is exported by both A and B; write A/x or
    /// B/x` when two `:use`d modules export it (syntax §5); or `x is
    /// private to M` when a private definition of another module has the
    /// name.
    pub fn unknown(&self, m: ModuleId, space: Space, name: &str, what: &str) -> String {
        if let Some((a, b)) = self.ambiguity(m, space, name) {
            let (a, b) = (self.module_name(a), self.module_name(b));
            return format!(
                "{name} is exported by both {a} and {b}; write {a}/{name} or {b}/{name}"
            );
        }
        match self.private_owner(m, space, name) {
            Some(o) => format!(
                "{name} is private to {}; it is not exported",
                self.module_name(o)
            ),
            None => format!("{what} {name}"),
        }
    }
}
