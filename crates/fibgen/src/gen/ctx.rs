//! What is in scope where an expression is generated, and what the
//! position allows.

use crate::ty::Ty;

/// How a variable was bound; decides whether it may be an `&` argument
/// (syntax §3.13: a `let` binding or an `&` parameter) and whether it is
/// a value at all (an `&` parameter is not, D2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VarKind {
    /// A `let` binding.
    Let,
    /// A plain parameter of a `defun` or `fn`.
    Param,
    /// A pattern variable of a `match` or a destructuring `let`.
    Pattern,
    /// A `loop` variable or a `plet` binding.
    Loop,
    /// An `&` parameter; `ty` is its content type.
    InOut,
    /// A `def` constant: a global, never a capture (syntax §3.19).
    Global,
}

/// A variable in scope.
#[derive(Clone, Debug)]
pub struct Var {
    /// Its name.
    pub name: String,
    /// Its type (the content type for an `&` parameter).
    pub ty: Ty,
    /// How it was bound.
    pub kind: VarKind,
    /// For a function-typed variable: its value is a named function or a
    /// literal whose captures are all sendable, so its colour is `send`
    /// (types §5.4) and a sendable closure may capture it.
    pub send_fn: bool,
}

impl Var {
    /// A variable whose function value (if any) is not known sendable.
    pub fn new(name: impl Into<String>, ty: Ty, kind: VarKind) -> Self {
        Var {
            name: name.into(),
            ty,
            kind,
            send_fn: false,
        }
    }
}

/// Which thread-related restrictions hold at a position.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Region {
    /// Ordinary code: everything is allowed.
    Main,
    /// A `plet` initialiser or a `pmap` function: runs concurrently with
    /// its siblings, all joined before the result is used. Atoms may be
    /// updated with a commutative `swap!` whose result is discarded, and
    /// never read, so the result does not depend on the schedule.
    Pool,
    /// An `async` body or a `spawn`ed closure, which may run at any later
    /// point: atoms are not reachable at all.
    Task,
}

/// The generation context.
#[derive(Clone, Debug)]
pub struct Ctx {
    /// Variables in scope, oldest first.
    pub vars: Vec<Var>,
    /// Inside an `async` body (and not inside a `fn` in it): `await` is allowed.
    pub in_async: bool,
    /// The thread region.
    pub region: Region,
    /// Values that may reach a cell are not usable (the value will be
    /// stored in a cell that could then be on a cycle, ownership.md §6).
    pub cell_free: bool,
}

impl Ctx {
    /// The context of a function body with these parameters.
    pub fn top(vars: Vec<Var>, in_async: bool) -> Self {
        Ctx {
            vars,
            in_async,
            region: Region::Main,
            cell_free: false,
        }
    }

    /// This context with `v` in scope (shadowing any variable of its name).
    pub fn with(&self, v: Var) -> Self {
        let mut c = self.clone();
        c.vars.retain(|x| x.name != v.name);
        c.vars.push(v);
        c
    }

    /// This context with several variables added in order.
    pub fn with_all(&self, vs: Vec<Var>) -> Self {
        vs.into_iter().fold(self.clone(), |c, v| c.with(v))
    }

    /// Whether `v` may be used as a value here.
    fn usable(&self, v: &Var) -> bool {
        v.kind != VarKind::InOut && !(self.cell_free && v.ty.may_reach_cell())
    }

    /// The variables of type `ty` usable as values.
    pub fn vars_of(&self, ty: &Ty) -> Vec<&Var> {
        self.vars
            .iter()
            .filter(|v| &v.ty == ty && self.usable(v))
            .collect()
    }

    /// The variables whose type satisfies `pred`, usable as values.
    pub fn vars_where(&self, pred: impl Fn(&Ty) -> bool) -> Vec<&Var> {
        self.vars
            .iter()
            .filter(|v| pred(&v.ty) && self.usable(v))
            .collect()
    }

    /// The `&` parameters with content type `ty`.
    pub fn inouts_of(&self, ty: &Ty) -> Vec<&Var> {
        self.vars
            .iter()
            .filter(|v| v.kind == VarKind::InOut && &v.ty == ty)
            .collect()
    }

    /// The variables that may stand as `&x` for content type `ty`: `let`
    /// bindings of type `(Cell ty)` and `&` parameters of content `ty`.
    pub fn cell_args(&self, ty: &Ty) -> Vec<&Var> {
        let cell = Ty::cell(ty.clone());
        self.vars
            .iter()
            .filter(|v| {
                (v.kind == VarKind::Let && v.ty == cell)
                    || (v.kind == VarKind::InOut && &v.ty == ty)
            })
            .collect()
    }

    /// Whether any `&` parameter is in scope.
    pub fn has_inout(&self) -> bool {
        self.vars.iter().any(|v| v.kind == VarKind::InOut)
    }

    /// The context of a closure body: `await` is not allowed (syntax
    /// §3.14) and, for an escaping closure, `&` parameters are not in
    /// scope (§3.13 rule 2).
    pub fn for_closure(&self, escaping: bool) -> Self {
        let mut c = self.clone();
        c.in_async = false;
        if escaping {
            c.vars.retain(|v| v.kind != VarKind::InOut);
        }
        c
    }

    /// The context of code that runs on another thread or as a task:
    /// only sendable captures (types §5), no `&` parameters (§3.13 rule
    /// 3), and no atoms unless `region` is [`Region::Pool`].
    pub fn for_region(&self, region: Region, in_async: bool) -> Self {
        let mut c = self.clone();
        c.region = region;
        c.in_async = in_async;
        c.vars.retain(|v| {
            let sendable = v.ty.is_send() || v.send_fn || v.kind == VarKind::Global;
            let atom_ok = region == Region::Pool || !v.ty.is_atom();
            v.kind != VarKind::InOut && sendable && atom_ok
        });
        c
    }

    /// Whether atoms may be read here.
    pub fn atoms_readable(&self) -> bool {
        self.region == Region::Main
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn var(name: &str, ty: Ty, kind: VarKind) -> Var {
        Var {
            name: name.into(),
            ty,
            kind,
            send_fn: false,
        }
    }

    #[test]
    fn inout_is_not_a_value_but_is_a_cell_argument() {
        let c = Ctx::top(vec![var("v", Ty::Int, VarKind::InOut)], false);
        assert!(c.vars_of(&Ty::Int).is_empty());
        assert_eq!(c.cell_args(&Ty::Int).len(), 1);
        assert!(c.for_closure(true).cell_args(&Ty::Int).is_empty());
    }

    #[test]
    fn task_region_drops_cells_and_atoms() {
        let c = Ctx::top(
            vec![
                var("c", Ty::cell(Ty::Int), VarKind::Let),
                var("a", Ty::atom(Ty::Int), VarKind::Let),
                var("s", Ty::Str, VarKind::Let),
            ],
            false,
        );
        let t = c.for_region(Region::Task, true);
        assert_eq!(t.vars.len(), 1);
        assert_eq!(c.for_region(Region::Pool, false).vars.len(), 2);
    }

    #[test]
    fn shadowing_replaces_the_older_binding() {
        let c = Ctx::top(vec![var("x", Ty::Int, VarKind::Let)], false);
        let c = c.with(var("x", Ty::Str, VarKind::Let));
        assert_eq!(c.vars.len(), 1);
        assert!(c.vars_of(&Ty::Int).is_empty());
    }
}
