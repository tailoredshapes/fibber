//! The names each module binds and how a name used in a module is
//! resolved (syntax §5): the chain builtins → prelude → program, a
//! qualified `fib.prelude/x`, and `:private` definitions, which no other
//! module sees except through `(var m/x)` (syntax §3.20).

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

impl Globals {
    /// The modules a name used in `m` is looked up in, in order.
    pub fn chain(&self, m: ModuleId) -> &[ModuleId] {
        &self.module(m).chain
    }

    /// The modules a qualified name `ns/x` used in `m` is looked up in:
    /// `ns` an alias or the full name of a module `m` requires or uses,
    /// or the prelude's name from anywhere.
    fn qualified<'n>(&self, m: ModuleId, name: &'n str) -> Option<(&[ModuleId], &'n str)> {
        let (ns, base) = name.split_once('/')?;
        if base.is_empty() {
            return None;
        }
        let target = match self.module(m).aliases.get(ns) {
            Some(t) => *t,
            None if ns == crate::expand::PRELUDE_NS => ModuleId::PRELUDE,
            None => return None,
        };
        Some((self.chain(target), base))
    }

    /// The modules `name`, used in `m`, is looked up in, with the name
    /// itself: `ns/x` is looked up in `ns`'s chain.
    fn lookup<'n>(&self, m: ModuleId, name: &'n str) -> (&[ModuleId], &'n str) {
        self.qualified(m, name).unwrap_or((self.chain(m), name))
    }

    /// Whether a definition of `owner` named `base` in `space` is
    /// visible from `m`: always in its own module, else unless private
    /// (syntax §5).
    fn visible(&self, m: ModuleId, owner: ModuleId, space: Space, base: &str) -> bool {
        owner == m || !self.names(owner).is_private(space, base)
    }

    /// Resolves a value name used in module `m`; a private value of
    /// another module is not visible (syntax §5).
    pub fn value(&self, m: ModuleId, name: &str) -> Option<GlobalRef> {
        let (chain, base) = self.lookup(m, name);
        chain
            .iter()
            .filter(|o| self.visible(m, **o, Space::Value, base))
            .find_map(|o| self.names(*o).values.get(base).copied())
    }

    /// Resolves a value name as `(var name)` does (syntax §3.20):
    /// private values of other modules included.
    pub fn value_any(&self, m: ModuleId, name: &str) -> Option<GlobalRef> {
        let (chain, base) = self.lookup(m, name);
        chain
            .iter()
            .find_map(|o| self.names(*o).values.get(base).copied())
    }

    /// Resolves a type name used in module `m` (private ones of other
    /// modules are not visible).
    pub fn type_name(&self, m: ModuleId, name: &str) -> Option<TypeId> {
        let (chain, base) = self.lookup(m, name);
        chain
            .iter()
            .filter(|o| self.visible(m, **o, Space::Type, base))
            .find_map(|o| self.names(*o).types.get(base).copied())
    }

    /// Resolves a protocol name used in module `m` (private ones of
    /// other modules are not visible).
    pub fn proto_name(&self, m: ModuleId, name: &str) -> Option<ProtoId> {
        let (chain, base) = self.lookup(m, name);
        chain
            .iter()
            .filter(|o| self.visible(m, **o, Space::Proto, base))
            .find_map(|o| self.names(*o).protos.get(base).copied())
    }

    /// The module whose private definition `name` in `space` would have
    /// resolved a name used in `m` that did not resolve, for the
    /// message `x is private to M`.
    pub fn private_owner(&self, m: ModuleId, space: Space, name: &str) -> Option<ModuleId> {
        let (chain, base) = self.lookup(m, name);
        chain
            .iter()
            .copied()
            .find(|o| *o != m && self.names(*o).is_private(space, base))
    }

    /// `unbound name x`, or `x is private to M` when a private
    /// definition of another module has the name (syntax §5).
    pub fn unknown(&self, m: ModuleId, space: Space, name: &str, what: &str) -> String {
        match self.private_owner(m, space, name) {
            Some(o) => format!(
                "{name} is private to {}; it is not exported",
                self.module_name(o)
            ),
            None => format!("{what} {name}"),
        }
    }
}
