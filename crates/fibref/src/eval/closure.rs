//! Closures and function values (types §6.5, §8.4), and protocol
//! dispatch (§4.5: on the type of the receiver at run time).

use crate::heap::{Kind, ObjId};
use crate::own::program::{BodyKey, Pass};
use crate::types::ast::{Expr, ExprId, ExprKind, GlobalRef};
use crate::types::ty::{Con, ProtoId, TypeId};

use super::alloc::Placement;
use super::call::Target;
use super::error::{RunError, R};
use super::expr::Flow;
use super::interp::{Frame, Interp};
use super::object::{Clo, Obj};
use super::value::Val;

impl<'p> Interp<'p> {
    /// A `fn` literal (§6.5): a heap closure retains its object captures
    /// (E3, as fields of the object); a stack closure's captures are
    /// aliases and stay out of the heap's fields.
    pub fn make_fn(&mut self, e: &Expr) -> R<Val> {
        let plan = self.plan()?;
        let own = plan
            .closures
            .get(&e.id)
            .ok_or_else(|| RunError::gap("no plan for a fn literal"))?;
        let at = Placement::of(plan.allocs.get(&e.id).copied());
        let (caps, fields) = self.captures(&own.captures)?;
        let body = self.frame()?.key;
        let clo = Clo::Lambda {
            lit: e.id,
            body,
            caps,
        };
        let id = self.alloc(Kind::Immutable, fields, Obj::Closure(clo), at)?;
        Ok(Val::Obj(id))
    }

    /// The values of the captures, and the heap fields of the retained
    /// ones.
    pub fn captures(
        &mut self,
        caps: &[crate::own::program::CaptureOwn],
    ) -> R<(super::object::Captures, Vec<crate::heap::Value>)> {
        let mut vals = Vec::with_capacity(caps.len());
        let mut fields = Vec::new();
        for c in caps {
            let v = self.local(c.binding)?;
            if c.pass == Pass::Retain {
                fields.push(v.project());
            }
            vals.push((c.binding, v));
        }
        Ok((vals, fields))
    }

    /// A call through a `fn` closure: its body in a frame of its own,
    /// with the closure as `env` (§8.4).
    pub fn run_lambda(&mut self, clo: ObjId, args: Vec<Val>) -> R<Flow> {
        let (lit, key, caps) = match self.objs.get(&self.heap, clo)? {
            Obj::Closure(Clo::Lambda { lit, body, caps }) => (*lit, *body, caps.clone()),
            o => return Err(RunError::internal(format!("not a fn closure: {o:?}"))),
        };
        let plan = self.body(key)?;
        let e = self.literal_expr(lit)?;
        let ExprKind::Fn(f) = &e.kind else {
            return Err(RunError::internal("a closure whose literal is not a fn"));
        };
        let mut frame = Frame::new(plan, key);
        frame.env = Some(Val::Obj(clo));
        frame.locals.extend(caps);
        if let Some(name) = f.name {
            frame.locals.insert(name, Val::Obj(clo));
        }
        for ((b, _), v) in f.params.iter().zip(args) {
            frame.locals.insert(*b, v);
        }
        self.run_frame(frame, &f.body)
    }

    /// The literal expression `lit`.
    pub fn literal_expr(&self, lit: ExprId) -> R<&'p Expr> {
        self.lits
            .get(&lit)
            .copied()
            .ok_or_else(|| RunError::internal(format!("no literal {lit:?}")))
    }

    /// A named function, builtin or constructor as a value: its
    /// immortal closure (§8.2, §8.4), made once.
    pub fn function_value(&mut self, g: GlobalRef) -> R<Val> {
        let clo = match g {
            GlobalRef::Fun(f) => Clo::Fun(f),
            GlobalRef::Builtin(b) => Clo::Builtin(b),
            GlobalRef::Method(p, i) => Clo::Method(p, i),
            GlobalRef::Ctor(t, v) => Clo::Ctor(t, v),
            GlobalRef::Extern(_) | GlobalRef::Def(_) => {
                return Err(RunError::internal(format!("{g:?} as a function value")))
            }
        };
        self.immortal_closure(clo)
    }

    /// The method value `e`, method `i` of `p` (§8.4): the resolved
    /// instance's implementation when the checker resolved the use to
    /// one (§4.2), else a value that dispatches on its receiver's type
    /// at each call (§4.5).
    pub fn method_value(&mut self, e: ExprId, p: ProtoId, i: usize) -> R<Val> {
        match self.instances.get(&e) {
            Some(inst) => self.immortal_closure(Clo::Impl(*inst, i)),
            None => self.function_value(GlobalRef::Method(p, i)),
        }
    }

    /// The immortal closure `clo`, made once per run.
    fn immortal_closure(&mut self, clo: Clo) -> R<Val> {
        let key = format!("{clo:?}");
        if let Some(id) = self.statics.closures.get(&key) {
            return Ok(Val::Obj(*id));
        }
        let id = self.alloc(
            Kind::Immutable,
            Vec::new(),
            Obj::Closure(clo),
            Placement::Immortal,
        )?;
        self.statics.closures.insert(key, id);
        Ok(Val::Obj(id))
    }

    /// A constructor named as a value: `nil`, a field-less variant (a
    /// scalar, or a new object of an enum with fields), or a
    /// constructor as a function value.
    pub fn ctor_value(&mut self, e: &Expr, t: TypeId, v: Option<usize>) -> R<Val> {
        let def = self.p.globals.ty(t);
        let fieldless = match (&def.shape, v) {
            (crate::types::decls::Shape::Enum(vs), Some(i)) => {
                vs.get(i).is_some_and(|x| x.fields.is_empty())
            }
            _ => false,
        };
        match v {
            Some(i) if fieldless && def.is_fieldless_enum() => Ok(Val::Tag(t, i as u32)),
            Some(_) if fieldless => {
                let at = self.placement_of(e.id)?;
                self.new_data(t, v, Vec::new(), at)
            }
            _ => self.function_value(GlobalRef::Ctor(t, v)),
        }
    }

    /// The target of method `i` of protocol `p` for the receiver `recv`:
    /// the resolution the checker recorded when it is an instance, else
    /// the instance of the receiver's type at run time (§4.5).
    pub fn dispatch(&self, site: Option<ExprId>, p: ProtoId, i: usize, recv: &Val) -> R<Target> {
        let inst = match site.and_then(|s| self.instances.get(&s)) {
            Some(index) => *index,
            None => self.instance_of(p, recv)?,
        };
        let def = &self.p.globals.instances[inst];
        if def.methods.is_empty() {
            return Ok(Target::Native(inst, i, false));
        }
        Ok(Target::Body(BodyKey::Method(
            inst,
            self.method_in(inst, i)?,
        )))
    }

    /// The target of a call through a method value (§8.4): the
    /// all-owned body of instance `inst`'s implementation of method `i`,
    /// or its built-in implementation under the closure convention.
    pub fn method_value_target(&self, inst: usize, i: usize) -> R<Target> {
        if self.p.globals.instances[inst].methods.is_empty() {
            return Ok(Target::Native(inst, i, true));
        }
        let m = self.method_in(inst, i)?;
        Ok(Target::Body(BodyKey::MethodOwned(inst, m)))
    }

    /// The instance of protocol `p` for the type of `recv` (§4.5).
    pub fn instance_of(&self, p: ProtoId, recv: &Val) -> R<usize> {
        let g = &self.p.globals;
        let con = self.con_of(recv)?;
        g.instance_index.get(&(p, con)).copied().ok_or_else(|| {
            RunError::internal(format!("no instance of {} for {con:?}", g.proto(p).name))
        })
    }

    fn method_in(&self, inst: usize, i: usize) -> R<usize> {
        self.p.globals.instances[inst]
            .methods
            .iter()
            .position(|m| m.index == i)
            .ok_or_else(|| RunError::internal("an instance without the method"))
    }

    /// The head constructor of a value's type at run time.
    pub fn con_of(&self, v: &Val) -> R<Con> {
        if let Some(c) = v.scalar_con() {
            return Ok(c);
        }
        let id = match v {
            Val::None | Val::Some(_) => return Ok(Con::Nominal(self.p.globals.option)),
            Val::Obj(id) => *id,
            _ => return Err(RunError::internal(format!("no type for {v:?}"))),
        };
        Ok(match self.objs.get(&self.heap, id)? {
            Obj::Str(_) => Con::Str,
            Obj::Struct { ty, .. } | Obj::Variant { ty, .. } => Con::Nominal(*ty),
            Obj::Array(_) => Con::Array,
            Obj::Cell(_) => Con::Cell,
            Obj::Atom(_) => Con::Atom,
            Obj::Weak(_) => Con::Weak,
            Obj::Task(_) => Con::Task,
            Obj::Closure(_) => return Err(RunError::internal("dispatch on a closure")),
        })
    }
}
