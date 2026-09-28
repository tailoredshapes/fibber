//! The lowering context: lexical scopes, binding sites, closure frames
//! (for capture sets, §3.7), enclosing loops (for `recur`, §2.4) and
//! `unsafe` depth.

use std::collections::HashMap;

use crate::syntax::Pos;

use crate::types::ast::{BindingId, BindingInfo, BindingKind, Expr, ExprKind};
use crate::types::decls::{Globals, ModuleId};

/// A `fn` or `async` literal being lowered.
struct Frame {
    is_async: bool,
    captures: Vec<BindingId>,
}

/// Lowers the expressions of one definition.
pub struct Lowerer<'g> {
    /// The global tables (bindings are added to them).
    pub g: &'g mut Globals,
    /// The module being lowered.
    pub m: ModuleId,
    /// The definition, for messages (`& parameter v used as a value in f`).
    pub owner: String,
    scope: Vec<(String, BindingId)>,
    depth: HashMap<BindingId, usize>,
    frames: Vec<Frame>,
    loops: Vec<usize>,
    /// How many `unsafe` forms enclose the current form.
    pub unsafe_depth: u32,
}

impl<'g> Lowerer<'g> {
    /// A lowerer for a definition named `owner` in module `m`.
    pub fn new(g: &'g mut Globals, m: ModuleId, owner: &str) -> Self {
        Lowerer {
            g,
            m,
            owner: owner.to_string(),
            scope: Vec::new(),
            depth: HashMap::new(),
            frames: Vec::new(),
            loops: Vec::new(),
            unsafe_depth: 0,
        }
    }

    /// A new binding site, in scope from now on.
    pub fn bind(&mut self, name: &str, kind: BindingKind, pos: &Pos) -> BindingId {
        let id = BindingId(self.g.bindings.len() as u32);
        self.g.bindings.push(BindingInfo {
            name: name.to_string(),
            kind,
            pos: pos.clone(),
            ann: None,
        });
        self.depth.insert(id, self.frames.len());
        self.scope.push((name.to_string(), id));
        id
    }

    /// The current scope, to restore with [`Lowerer::reset`].
    pub fn mark(&self) -> usize {
        self.scope.len()
    }

    /// Ends the bindings made since `mark`.
    pub fn reset(&mut self, mark: usize) {
        self.scope.truncate(mark);
    }

    /// The binding `name` refers to, recording it as a capture of every
    /// closure frame between its binding and here.
    pub fn lookup(&mut self, name: &str) -> Option<BindingId> {
        let id = self.scope.iter().rev().find(|(n, _)| n == name)?.1;
        let bound_at = self.depth.get(&id).copied().unwrap_or(0);
        for frame in self.frames.iter_mut().skip(bound_at) {
            if !frame.captures.contains(&id) {
                frame.captures.push(id);
            }
        }
        Some(id)
    }

    /// Whether `name` is bound locally, without recording a capture.
    pub fn is_local(&self, name: &str) -> bool {
        self.scope.iter().any(|(n, _)| n == name)
    }

    /// The kind of a binding.
    pub fn kind(&self, id: BindingId) -> BindingKind {
        self.g.binding(id).kind
    }

    /// Enters a `fn` (or `async`) literal: loops outside it are not
    /// visible to `recur` inside it.
    pub fn enter_frame(&mut self, is_async: bool) {
        self.frames.push(Frame {
            is_async,
            captures: Vec::new(),
        });
    }

    /// Leaves the literal; returns its capture set.
    pub fn leave_frame(&mut self) -> Vec<BindingId> {
        self.frames.pop().map(|f| f.captures).unwrap_or_default()
    }

    /// Whether the innermost closure frame is an `async` (for `await`).
    pub fn in_async(&self) -> bool {
        self.frames.last().is_some_and(|f| f.is_async)
    }

    /// Enters a loop body.
    pub fn enter_loop(&mut self) {
        self.loops.push(self.frames.len());
    }

    /// Leaves the loop body.
    pub fn leave_loop(&mut self) {
        self.loops.pop();
    }

    /// Whether a loop encloses the current form in the same closure
    /// frame.
    pub fn in_loop(&self) -> bool {
        self.loops.last() == Some(&self.frames.len())
    }

    /// An expression with a fresh id.
    pub fn mk(&mut self, pos: &Pos, kind: ExprKind) -> Expr {
        Expr {
            id: self.g.next_expr(),
            pos: pos.clone(),
            kind,
        }
    }
}
