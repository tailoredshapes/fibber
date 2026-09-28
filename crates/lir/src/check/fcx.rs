//! The per-function checking context: names, dominance, statements.

use std::collections::HashMap;

use super::cfg::Cfg;
use super::env::Env;
use super::phi::PendingPhi;
use super::walk::Bindings;
use crate::ast::{Binding, Expr, Function, Kind};
use crate::diag::{err, Pos, Result};
use crate::types::Type;

pub struct Fcx<'a> {
    pub env: &'a Env,
    pub f: &'a Function,
    pub cfg: &'a Cfg,
    pub labels: &'a HashMap<String, usize>,
    pub defs: HashMap<String, Option<usize>>,
    pub allocas: HashMap<String, Type>,
    /// Types of the names checked so far.
    pub types: HashMap<String, Type>,
    /// The block being checked.
    pub cur: usize,
    /// Whether an instruction other than a phi has been evaluated in
    /// the current block (spec/lir.md §5.4).
    pub emitted: bool,
    pub phis: Vec<PendingPhi<'a>>,
}

impl<'a> Fcx<'a> {
    pub fn new(
        env: &'a Env,
        f: &'a Function,
        cfg: &'a Cfg,
        labels: &'a HashMap<String, usize>,
        b: Bindings,
    ) -> Self {
        let types = f
            .params
            .iter()
            .zip(&f.ty.params)
            .map(|((n, _), t)| (n.clone(), t.clone()))
            .collect();
        Fcx {
            env,
            f,
            cfg,
            labels,
            defs: b.defs,
            allocas: b.allocas,
            types,
            cur: 0,
            emitted: false,
            phis: Vec::new(),
        }
    }

    pub fn label(&self, b: usize) -> &'a str {
        &self.f.blocks[b].label
    }

    /// Check one block's statements.
    pub fn block(&mut self, b: usize) -> Result<()> {
        self.cur = b;
        self.emitted = false;
        let body = &self.f.blocks[b].body;
        body.iter().try_for_each(|e| self.stmt(e))
    }

    /// A form in statement position: its value, if any, is dropped.
    pub fn stmt(&mut self, e: &'a Expr) -> Result<()> {
        match &e.kind {
            Kind::Let(binds, body) => self.let_form(binds, body, false).map(|_| ()),
            k if k.is_terminator() => self.terminator(e),
            _ => self.ty(e).map(|_| ()),
        }
    }

    /// A form evaluated for its effect where no terminator may appear
    /// (inside a `let` used as a value).
    pub fn effect(&mut self, e: &'a Expr) -> Result<()> {
        match &e.kind {
            k if k.is_terminator() => err(e.pos, "terminator used as a value"),
            Kind::Let(binds, body) => self.let_form(binds, body, true).map(|_| ()),
            _ => self.ty(e).map(|_| ()),
        }
    }

    /// A form whose value is used: never void, never a terminator.
    pub fn val(&mut self, e: &'a Expr) -> Result<Type> {
        if e.kind.is_terminator() {
            return err(e.pos, "terminator used as a value");
        }
        match self.ty(e)? {
            Some(t) => Ok(t),
            None => err(e.pos, "void value used as an operand"),
        }
    }

    /// `(let ((x v)..) body..)`: its value is the last body form's.
    pub fn let_form(
        &mut self,
        binds: &'a [Binding],
        body: &'a [Expr],
        as_value: bool,
    ) -> Result<Option<Type>> {
        for b in binds {
            if b.value.kind.is_terminator() {
                return err(b.value.pos, "terminator used as a value");
            }
            let Some(t) = self.ty(&b.value)? else {
                return err(b.pos, format!("void value bound to {}", b.name));
            };
            self.types.insert(b.name.clone(), t);
        }
        let (last, init) = body
            .split_last()
            .ok_or_else(|| crate::diag::Diagnostic::new(Pos::default(), "let without a body"))?;
        for e in init {
            if as_value {
                self.effect(e)?;
            } else {
                self.stmt(e)?;
            }
        }
        if !as_value {
            self.stmt(last)?;
            return Ok(None);
        }
        if last.kind.is_terminator() {
            return err(last.pos, "terminator used as a value");
        }
        self.ty(last)
    }

    /// The type of a use of `name` in the current block.
    pub fn lookup(&self, name: &str, pos: Pos) -> Result<Type> {
        let here = self.label(self.cur);
        match self.defs.get(name) {
            None => err(pos, format!("undefined name {name}")),
            Some(Some(d)) if *d == self.cur => match self.types.get(name) {
                Some(t) => Ok(t.clone()),
                None => err(
                    pos,
                    format!("{name} is used before its binding in block {here}"),
                ),
            },
            Some(Some(d)) if !self.cfg.available(*d, self.cur) => err(
                pos,
                format!("{name} does not dominate its use in block {here}"),
            ),
            Some(_) => self.typed(name, pos),
        }
    }

    /// The type of a use of `name` at the end of block `m` (a phi's
    /// incoming value, spec/lir.md §5.4).
    pub fn lookup_at_end(&self, name: &str, m: usize, pos: Pos) -> Result<Type> {
        match self.defs.get(name) {
            None => err(pos, format!("undefined name {name}")),
            Some(Some(d)) if *d != m && !self.cfg.available(*d, m) => err(
                pos,
                format!(
                    "{name} does not dominate the end of block {}",
                    self.label(m)
                ),
            ),
            Some(_) => self.typed(name, pos),
        }
    }

    fn typed(&self, name: &str, pos: Pos) -> Result<Type> {
        match self.types.get(name) {
            Some(t) => Ok(t.clone()),
            None => err(pos, format!("{name} does not dominate its use")),
        }
    }

    /// `t` must be a valid type of this module.
    pub fn valid(&self, t: &Type, pos: Pos) -> Result<()> {
        self.env.valid(t, pos)
    }
}
