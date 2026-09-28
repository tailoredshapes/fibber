//! Calls (types §8.9): the arguments handed over as the plan's passes
//! say, the copy-in of `&` arguments, the callee's specialisation, the
//! call or tail call, and the write-backs.

use fibref::own::program::{Callee, Pass, Tail};
use fibref::types::ast::{Arg, BindingId, Expr, ExprKind, GlobalRef};
use fibref::types::infer::Resolution;
use fibref::types::ty::{Con, Pred, ProtoId, Ty};

use super::{Cx, Flow, R};
use crate::compile::Unsupported;
use crate::ir::{LirTy, V};
use crate::mono::fun_key;
use crate::objects::{CELL_VALUE, CLOSURE_CODE};

/// What a call reaches.
#[derive(Clone, Debug)]
pub enum Target {
    /// A defined function by name, with `env` first when `env` holds.
    Direct { name: String, env: bool },
    /// A closure value: its object.
    Value(V),
    /// A builtin by name.
    Builtin(&'static str),
    /// A method of a native instance: protocol and method names.
    Native(String, String),
    /// A constructor.
    Ctor(fibref::types::ty::TypeId, Option<usize>),
    /// A method reached through a vtable: its code, with `this` as the
    /// receiver (§8.5).
    Code { code: String, this: String },
    /// An extern by name.
    Extern(String),
}

impl<'a> Cx<'_, 'a> {
    pub fn call(&mut self, e: &'a Expr, head: &'a Expr, args: &'a [Arg]) -> R<Flow> {
        let own = self
            .own
            .calls
            .get(&e.id)
            .cloned()
            .ok_or_else(|| Unsupported("no plan for a call".into()))?;
        let head_val = match own.callee {
            Callee::Value => {
                let v = self.value(head)?;
                if own.head == Pass::Retain {
                    self.retain(&v);
                }
                Some(v)
            }
            _ => None,
        };
        let mut vals = Vec::with_capacity(args.len());
        let mut arg_tys = Vec::with_capacity(args.len());
        for (i, a) in args.iter().enumerate() {
            let pass = own
                .args
                .get(i)
                .copied()
                .ok_or_else(|| Unsupported("no pass for an argument".into()))?;
            let (v, t) = self.argument(a, pass)?;
            vals.push(v);
            arg_tys.push(t);
        }
        self.copy_in(args, &own.args, &mut vals)?;
        let target = self.target(head, own.callee, head_val, &vals, &arg_tys)?;
        let rt = self.ty(e)?;
        if own.tail == Tail::TailCall {
            self.run_ops(&own.jump)?;
            if let Target::Direct { .. } | Target::Value(_) | Target::Code { .. } = target {
                self.emit_call(&target, &vals, &rt, true)?;
                return Ok(Flow::Jump);
            }
        }
        let result = match &target {
            Target::Direct { .. } | Target::Value(_) | Target::Extern(_) | Target::Code { .. } => {
                self.emit_call(&target, &vals, &rt, false)?
            }
            Target::Builtin(name) => match self.builtin(name, e, &vals, &arg_tys)? {
                Some(v) => v,
                None => return Ok(Flow::Jump),
            },
            Target::Native(proto, method) => {
                self.native_method(proto, method, &vals, &arg_tys, &rt)?
            }
            Target::Ctor(t, variant) => self.construct(e, *t, *variant, &vals)?,
        };
        for (i, var) in &own.write_backs {
            self.write_back(*var, &vals[*i], &arg_tys[*i])?;
        }
        if own.tail == Tail::TailCall {
            match &result {
                V::Unit => self.b.term("(ret)"),
                V::Val(s, _) => self.b.term(&format!("(ret {s})")),
            }
            return Ok(Flow::Jump);
        }
        Ok(Flow::Val(result))
    }

    fn argument(&mut self, a: &'a Arg, pass: Pass) -> R<(V, Ty)> {
        match a {
            Arg::Expr(x) => {
                let v = self.value(x)?;
                if pass == Pass::Retain {
                    self.retain(&v);
                }
                Ok((v, self.ty(x)?))
            }
            Arg::Amp(b, _) => {
                let t = self.binding_ty(*b)?;
                match pass {
                    Pass::Acquire => Ok((V::Unit, t)),
                    Pass::Forward | Pass::OwnCell => Ok((self.local(*b)?, t)),
                    other => Err(Unsupported(format!("an & argument passed as {other:?}"))),
                }
            }
        }
    }

    /// The copy-in of every `&` argument at call entry (§8.6): a
    /// private `STACK` cell per site holding the variable's retained
    /// content.
    fn copy_in(&mut self, args: &[Arg], passes: &[Pass], vals: &mut [V]) -> R<()> {
        for (i, (a, pass)) in args.iter().zip(passes).enumerate() {
            let (Arg::Amp(b, _), Pass::Acquire) = (a, pass) else {
                continue;
            };
            let cell_ty = self.binding_ty(*b)?;
            let content_ty = match &cell_ty {
                Ty::Con(Con::Cell, args) => args[0].clone(),
                _ => return Err(Unsupported("an & parameter that is not a cell".into())),
            };
            let (tid, sname) = self.p.object(&cell_ty)?;
            let slot = self.b.entry_alloca(&format!("%struct.{sname}"));
            self.b
                .stmt(&format!("(call @fib.stack-init {slot} (i32 {tid}))"));
            let var = self.local(*b)?;
            let content = self.cell_load(&sname, var.text(), &content_ty)?;
            self.retain(&content);
            let field = self.gep(&sname, &slot, CELL_VALUE);
            self.store(&content, &field);
            vals[i] = V::Val(slot, LirTy::Ptr);
        }
        Ok(())
    }

    /// The write-back after an ordinary call (§8.6): the private cell's
    /// content into the variable, the variable's old content released.
    fn write_back(&mut self, var: BindingId, private: &V, cell_ty: &Ty) -> R<()> {
        let content_ty = match cell_ty {
            Ty::Con(Con::Cell, args) => args[0].clone(),
            _ => return Err(Unsupported("a write-back to a non-cell".into())),
        };
        let (_, sname) = self.p.object(cell_ty)?;
        let content = self.cell_load(&sname, private.text(), &content_ty)?;
        let target = self.local(var)?;
        let old = self.cell_load(&sname, target.text(), &content_ty)?;
        let field = self.gep(&sname, target.text(), CELL_VALUE);
        self.store(&content, &field);
        self.release(&old);
        self.b
            .stmt(&format!("(call @fib.stack-end {})", private.text()));
        Ok(())
    }

    /// The content of a cell object.
    pub fn cell_load(&mut self, sname: &str, cell: &str, content_ty: &Ty) -> R<V> {
        let field = self.gep(sname, cell, CELL_VALUE);
        Ok(match self.p.lir(content_ty)? {
            Some(t) => self.load(t, &field),
            None => V::Unit,
        })
    }

    fn target(
        &mut self,
        head: &'a Expr,
        c: Callee,
        hv: Option<V>,
        vals: &[V],
        arg_tys: &[Ty],
    ) -> R<Target> {
        use fibref::own::program::BodyKey;
        Ok(match (c, &head.kind) {
            (Callee::Fun(f), _) => {
                let tys = self.fun_instance(head, f)?;
                Target::Direct {
                    name: self.p.request(BodyKey::Fun(f), tys),
                    env: false,
                }
            }
            (Callee::AllOwned(f), _) => {
                let tys = self.fun_instance(head, f)?;
                Target::Direct {
                    name: self.p.request(BodyKey::AllOwned(f), tys),
                    env: true,
                }
            }
            (Callee::Method(p, i), _) => self.method_target(head, p, i, vals, arg_tys)?,
            (Callee::Builtin(b), _) => Target::Builtin(self.p.c.typed.builtins[b.0 as usize].name),
            (Callee::Ctor, ExprKind::Global(GlobalRef::Ctor(t, v))) => Target::Ctor(*t, *v),
            (Callee::Extern, ExprKind::Global(GlobalRef::Extern(x))) => {
                let d = self.p.g().ext(*x);
                self.p.declare_extern(d)?;
                Target::Extern(d.name.clone())
            }
            (Callee::Value, _) => {
                Target::Value(hv.ok_or_else(|| Unsupported("no head value".into()))?)
            }
            (c, _) => return Err(Unsupported(format!("callee {c:?} with this head"))),
        })
    }

    /// The specialisation key of a call to `f` at `head`.
    fn fun_instance(&self, head: &Expr, f: fibref::types::ast::FunId) -> R<Vec<Ty>> {
        let g = self.p.g();
        let scheme = self
            .p
            .c
            .typed
            .fun_schemes
            .get(f.0 as usize)
            .and_then(|s| s.as_ref())
            .ok_or_else(|| Unsupported("a function without a scheme".into()))?;
        let tys: Vec<Ty> = match self.p.c.typed.instantiations.get(&head.id) {
            Some(i) => i.tys.iter().map(|t| self.inst.subst(t)).collect(),
            // A call inside the callee's own SCC has no instantiation:
            // the members share their quantified variables (§3.6), so
            // the callee's are the caller's, by index.
            None => (0..scheme.n_vars)
                .map(|i| self.inst.subst(&Ty::Gen(i)))
                .collect(),
        };
        if tys.len() != scheme.n_vars as usize {
            return Err(Unsupported(format!(
                "call of {} instantiates {} of {} variables",
                g.fun(f).name,
                tys.len(),
                scheme.n_vars
            )));
        }
        fun_key(g, scheme, &tys)
    }

    /// The instance a method call reaches, as a target.
    fn method_target(
        &mut self,
        head: &Expr,
        p: ProtoId,
        i: usize,
        vals: &[V],
        arg_tys: &[Ty],
    ) -> R<Target> {
        use fibref::own::program::BodyKey;
        let g = self.p.g();
        let (index, args) = match self.p.c.typed.resolutions.get(&head.id) {
            Some(Resolution::Instance { index, args }) => (
                *index,
                args.iter().map(|t| self.inst.subst(t)).collect::<Vec<_>>(),
            ),
            Some(Resolution::Bound(pred)) => {
                let pred = pred.map_tys(&mut |t| self.inst.subst(t));
                self.instance_of_pred(&pred)?
            }
            Some(Resolution::Dyn) => {
                let (recv, t) = vals
                    .first()
                    .zip(arg_tys.first())
                    .ok_or_else(|| Unsupported("a dyn call without a receiver".into()))?;
                return self.dyn_target(&recv.clone(), &t.clone(), p, i);
            }
            None => return Err(Unsupported("a method call without a resolution".into())),
        };
        let inst = &g.instances[index];
        if inst.methods.is_empty() {
            let proto = g.proto(inst.proto);
            return Ok(Target::Native(
                proto.name.clone(),
                proto.methods[i].name.clone(),
            ));
        }
        let m = inst
            .methods
            .iter()
            .position(|im| im.index == i)
            .ok_or_else(|| Unsupported("an instance without the method".into()))?;
        Ok(Target::Direct {
            name: self.p.request(BodyKey::Method(index, m), args),
            env: false,
        })
    }

    /// The instance that discharges a concrete protocol constraint,
    /// with its variables' values.
    pub fn instance_of_pred(&self, pred: &Pred) -> R<(usize, Vec<Ty>)> {
        let g = self.p.g();
        let Pred::Proto(p, tys) = pred else {
            return Err(Unsupported("a non-protocol bound at a method call".into()));
        };
        let head_ty = tys
            .first()
            .ok_or_else(|| Unsupported("a protocol without a head".into()))?;
        let con = match head_ty {
            Ty::Con(c, _) => *c,
            Ty::Fn(..) => return Err(Unsupported("a protocol instance on a function type".into())),
            _ => return Err(Unsupported("a type variable at a method call".into())),
        };
        let index = g.instance_index.get(&(*p, con)).copied().ok_or_else(|| {
            Unsupported(format!("no instance of {} for {con:?}", g.proto(*p).name))
        })?;
        let inst = &g.instances[index];
        let mut vars: Vec<Option<Ty>> = vec![None; inst.var_names.len()];
        match_head(&inst.head, head_ty, &mut vars);
        let args = vars
            .into_iter()
            .map(|v| {
                v.ok_or_else(|| Unsupported("an instance variable the head does not fix".into()))
            })
            .collect::<R<Vec<_>>>()?;
        Ok((index, args))
    }

    /// The call itself: `call` or `tailcall`, direct or through a
    /// closure's code pointer (§8.4).
    pub fn emit_call(&mut self, target: &Target, vals: &[V], rt: &Ty, tail: bool) -> R<V> {
        let ret = self.p.lir(rt)?;
        let args: Vec<String> = vals
            .iter()
            .filter(|v| !matches!(v, V::Unit))
            .map(|v| v.text().to_string())
            .collect();
        let instr = match target {
            Target::Direct { name, env } => {
                let op = if tail { "tailcall" } else { "call" };
                let envarg = if *env { " (ptr null)" } else { "" };
                format!("({op} @{name}{envarg} {})", args.join(" "))
            }
            Target::Extern(name) => format!("(call @{name} {})", args.join(" ")),
            Target::Code { code, this } => {
                let tys: Vec<&str> = std::iter::once("ptr")
                    .chain(vals.iter().skip(1).filter_map(|v| v.ty()).map(LirTy::text))
                    .collect();
                let op = if tail {
                    "indirect-tailcall"
                } else {
                    "indirect-call"
                };
                format!(
                    "({op} {code} (fn tailcc {} ({})) {this} {})",
                    ret.map_or("void", LirTy::text),
                    tys.join(" "),
                    args[1..].join(" ")
                )
            }
            Target::Value(clo) => {
                let sname = "fib.closure";
                let codep = self.gep(sname, clo.text(), CLOSURE_CODE);
                let code = self.load(LirTy::Ptr, &codep);
                let tys: Vec<&str> = std::iter::once("ptr")
                    .chain(vals.iter().filter_map(|v| v.ty()).map(LirTy::text))
                    .collect();
                let op = if tail {
                    "indirect-tailcall"
                } else {
                    "indirect-call"
                };
                format!(
                    "({op} {} (fn tailcc {} ({})) {} {})",
                    code.text(),
                    ret.map_or("void", LirTy::text),
                    tys.join(" "),
                    clo.text(),
                    args.join(" ")
                )
            }
            _ => return Err(Unsupported("not a callable target".into())),
        };
        if tail {
            self.b.term(&instr);
            return Ok(V::Unit);
        }
        Ok(match ret {
            Some(t) => self.b.val(&instr, t),
            None => {
                self.b.stmt(&instr);
                V::Unit
            }
        })
    }
}

/// Matches an instance head (over `Gen` variables) against a concrete
/// type, recording each variable's value.
fn match_head(head: &Ty, actual: &Ty, vars: &mut [Option<Ty>]) {
    match (head, actual) {
        (Ty::Gen(i), t) => {
            if let Some(slot) = vars.get_mut(*i as usize) {
                *slot = Some(t.clone());
            }
        }
        (Ty::Con(_, hs), Ty::Con(_, ts)) => {
            for (h, t) in hs.iter().zip(ts) {
                match_head(h, t, vars);
            }
        }
        (Ty::Fn(_, hp, hr), Ty::Fn(_, tp, tr)) => {
            for (h, t) in hp.iter().zip(tp) {
                match_head(h, t, vars);
            }
            match_head(hr, tr, vars);
        }
        _ => {}
    }
}
