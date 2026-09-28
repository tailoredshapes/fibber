//! The module environment: structs, functions and globals, with the
//! module-level rules of spec/lir.md §4.1.

use std::collections::{HashMap, HashSet};

use crate::ast::{Item, Module};
use crate::diag::{err, Diagnostic, Pos, Result};
use crate::types::{FnType, Type};

/// What a global name denotes.
#[derive(Clone, Debug, PartialEq)]
pub enum Symbol {
    Func(FnType),
    Var { ty: Type, constant: bool },
}

#[derive(Clone, Debug, Default)]
pub struct Env {
    pub structs: HashMap<String, Vec<Type>>,
    pub symbols: HashMap<String, Symbol>,
}

impl Env {
    /// Collect every definition, reporting duplicates, reserved names,
    /// undefined and self-containing structs.
    pub fn build(m: &Module) -> std::result::Result<Env, Vec<Diagnostic>> {
        let mut env = Env::default();
        let mut errs = Vec::new();
        let mut order = Vec::new();
        for item in &m.items {
            if let Err(e) = env.add(item, &mut order) {
                errs.push(e);
            }
        }
        for item in &m.items {
            if let Err(e) = env.item_types(item) {
                errs.push(e);
            }
        }
        if errs.is_empty() {
            for (name, pos) in &order {
                if let Err(e) = env.acyclic(name, *pos) {
                    errs.push(e);
                }
            }
        }
        if errs.is_empty() {
            Ok(env)
        } else {
            Err(errs)
        }
    }

    fn add(&mut self, item: &Item, order: &mut Vec<(String, Pos)>) -> Result<()> {
        let (name, pos, sym) = match item {
            Item::Struct(s) => {
                if self
                    .structs
                    .insert(s.name.clone(), s.fields.clone())
                    .is_some()
                {
                    return err(s.pos, format!("duplicate struct %struct.{}", s.name));
                }
                order.push((s.name.clone(), s.pos));
                return Ok(());
            }
            Item::Global(g) => (
                &g.name,
                g.pos,
                Symbol::Var {
                    ty: g.ty.clone(),
                    constant: g.constant,
                },
            ),
            Item::Declare(d) => (&d.name, d.pos, Symbol::Func(d.ty.clone())),
            Item::Define(f) => (&f.name, f.pos, Symbol::Func(f.ty.clone())),
        };
        if name.starts_with("llvm.") {
            return err(pos, format!("name @{name} is reserved"));
        }
        if self.symbols.insert(name.clone(), sym).is_some() {
            return err(pos, format!("duplicate definition of @{name}"));
        }
        Ok(())
    }

    fn item_types(&self, item: &Item) -> Result<()> {
        match item {
            Item::Struct(s) => s.fields.iter().try_for_each(|t| self.valid(t, s.pos)),
            Item::Global(g) => self.valid(&g.ty, g.pos),
            Item::Declare(d) => self.valid_fn(&d.ty, d.pos),
            Item::Define(f) => self.valid_fn(&f.ty, f.pos),
        }
    }

    /// Every struct `t` names is defined.
    pub fn valid(&self, t: &Type, pos: Pos) -> Result<()> {
        match t {
            Type::Named(n) if !self.structs.contains_key(n) => {
                err(pos, format!("undefined struct %struct.{n}"))
            }
            Type::Anon(fs) => fs.iter().try_for_each(|f| self.valid(f, pos)),
            Type::Vector(_, e) => self.valid(e, pos),
            _ => Ok(()),
        }
    }

    pub fn valid_fn(&self, f: &FnType, pos: Pos) -> Result<()> {
        if let Some(r) = &f.ret {
            self.valid(r, pos)?;
        }
        f.params.iter().try_for_each(|p| self.valid(p, pos))
    }

    fn acyclic(&self, name: &str, pos: Pos) -> Result<()> {
        let mut seen = HashSet::new();
        let fields = self.structs.get(name).cloned().unwrap_or_default();
        let mut stack: Vec<Type> = fields;
        while let Some(t) = stack.pop() {
            match t {
                Type::Named(n) if n == name => {
                    return err(
                        pos,
                        format!("struct %struct.{name} contains itself by value"),
                    )
                }
                Type::Named(n) => {
                    if seen.insert(n.clone()) {
                        stack.extend(self.structs.get(&n).cloned().unwrap_or_default());
                    }
                }
                Type::Anon(fs) => stack.extend(fs),
                _ => {}
            }
        }
        Ok(())
    }

    /// The fields of a struct type, named or anonymous.
    pub fn fields(&self, t: &Type) -> Option<Vec<Type>> {
        match t {
            Type::Named(n) => self.structs.get(n).cloned(),
            Type::Anon(fs) => Some(fs.clone()),
            _ => None,
        }
    }

    /// A function's type, or the error for a call of `@name`.
    pub fn function(&self, name: &str, pos: Pos) -> Result<&FnType> {
        match self.symbols.get(name) {
            Some(Symbol::Func(f)) => Ok(f),
            Some(Symbol::Var { .. }) => err(pos, format!("@{name} is not a function")),
            None => err(pos, format!("undefined function @{name}")),
        }
    }
}
