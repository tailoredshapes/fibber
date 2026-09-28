//! Lowering one body of the plan to one lIR function (types §8.9,
//! §8.10): the context [`Cx`], the entry points for bodies and closure
//! code, and the dispatch over expression kinds. The plan decides every
//! count operation; this code emits them where the plan lists them
//! ([`ops`]) and lowers the forms themselves ([`ctrl`], [`call`],
//! [`pattern`], [`objects`], [`builtins`], [`arith`], [`cells`]).

mod arith;
mod builtins;
mod call;
mod cells;
mod ctrl;
mod dynamic;
mod objects;
mod ops;
mod pattern;
mod quote;
mod threads;
mod values;

use std::collections::{HashMap, HashSet};

use fibref::own::program::{BodyKey, BodyOwn};
use fibref::types::ast::{BindingId, Expr, ExprId, ExprKind, FnLit, Lit};
use fibref::types::ty::Ty;

use crate::compile::Unsupported;
use crate::ir::{FnBuilder, LirTy, V};
use crate::mono::Inst;
use crate::program::Program;

pub type R<T> = Result<T, Unsupported>;

/// What lowering an expression produced.
#[derive(Clone, Debug)]
pub enum Flow {
    /// A value (or `unit`), in the current block.
    Val(V),
    /// Control left: a tail call, a `recur`, a trap.
    Jump,
}

/// A local's storage: an SSA value, or a slot for a loop variable.
#[derive(Clone, Debug)]
pub enum Local {
    Val(V),
    Slot(String, Option<LirTy>),
}

/// A `loop` being lowered: its variables' slots and its head label.
#[derive(Clone, Debug)]
pub struct LoopCx {
    pub vars: Vec<(BindingId, String, Option<LirTy>)>,
    pub head: String,
}

/// The context of one function.
pub struct Cx<'p, 'a> {
    pub p: &'p mut Program<'a>,
    pub inst: Inst,
    pub own: &'a BodyOwn,
    pub name: String,
    pub b: FnBuilder,
    pub locals: HashMap<BindingId, Local>,
    pub values: HashMap<ExprId, V>,
    pub value_sites: HashSet<ExprId>,
    pub env: Option<V>,
    pub loops: Vec<LoopCx>,
    pub stack_slots: HashMap<ExprId, String>,
    pub lits: HashMap<ExprId, &'a Expr>,
    /// The rest variables of the pattern being matched: bound to
    /// `fib.vec-drop` vectors once the whole pattern has matched (§8.3).
    pub rests: Vec<(BindingId, V, usize, Ty)>,
    /// The entry-block slots of stack closures: their captures are
    /// aliases (§6.5), so their scope end runs no drop.
    pub closure_slots: HashSet<String>,
}

/// Emits the function of `inst` into the program.
pub fn emit_body(p: &mut Program<'_>, inst: Inst) -> R<()> {
    let c = p.c;
    let g = &c.typed.globals;
    let own = c
        .owned
        .bodies
        .get(&inst.key)
        .ok_or_else(|| Unsupported(format!("no plan for {:?}", inst.key)))?;
    let (params, body): (Vec<BindingId>, &Expr) = match inst.key {
        BodyKey::Fun(f) | BodyKey::AllOwned(f) => {
            let d = g.fun(f);
            (d.params.iter().map(|q| q.binding).collect(), &d.body)
        }
        BodyKey::Method(i, m) | BodyKey::MethodOwned(i, m) => {
            let im = &g.instances[i].methods[m];
            (im.params.clone(), &im.body)
        }
        BodyKey::Def(_) => return Err(Unsupported("def initialisers".into())),
    };
    let name = p.queue.request(g, inst.clone());
    let env = matches!(inst.key, BodyKey::AllOwned(_) | BodyKey::MethodOwned(..));
    let mut sig = Vec::new();
    if env {
        sig.push((LirTy::Ptr, "env".to_string()));
    }
    let mut cx = Cx::new(p, inst, own, &name, Vec::new());
    for (i, b) in params.iter().enumerate() {
        let t = cx.binding_ty(*b)?;
        match cx.p.lir(&t)? {
            Some(l) => {
                let pn = format!("p{i}");
                sig.push((l, pn.clone()));
                cx.locals.insert(*b, Local::Val(V::Val(pn, l)));
            }
            None => {
                cx.locals.insert(*b, Local::Val(V::Unit));
            }
        }
    }
    cx.b.params = sig;
    if env {
        cx.env = Some(V::Val("env".into(), LirTy::Ptr));
    }
    cx.finish_with(body)
}

/// Emits the code of closure literal `lit` of `owner` (§8.4): `env`
/// first, then the parameters; captures loaded from `env`.
pub fn emit_closure(p: &mut Program<'_>, owner: Inst, lit: ExprId, name: &str) -> R<()> {
    let c = p.c;
    let own = c
        .owned
        .bodies
        .get(&owner.key)
        .ok_or_else(|| Unsupported(format!("no plan for {:?}", owner.key)))?;
    let lits = literals(c, owner.key);
    let e = lits
        .get(&lit)
        .copied()
        .ok_or_else(|| Unsupported(format!("no literal {lit:?}")))?;
    let f = match &e.kind {
        ExprKind::Fn(f) => f,
        ExprKind::Async(body, _) => return emit_async(p, owner, own, e, body, name),
        _ => return Err(Unsupported("a literal that is neither fn nor async".into())),
    };
    let mut cx = Cx::new(p, owner, own, name, Vec::new());
    cx.lits = lits;
    let mut sig = vec![(LirTy::Ptr, "env".to_string())];
    let env = V::Val("env".into(), LirTy::Ptr);
    cx.env = Some(env.clone());
    if let Some(n) = f.name {
        cx.locals.insert(n, Local::Val(env));
    }
    for (i, (b, _)) in f.params.iter().enumerate() {
        let t = cx.binding_ty(*b)?;
        match cx.p.lir(&t)? {
            Some(l) => {
                let pn = format!("p{i}");
                sig.push((l, pn.clone()));
                cx.locals.insert(*b, Local::Val(V::Val(pn, l)));
            }
            None => {
                cx.locals.insert(*b, Local::Val(V::Unit));
            }
        }
    }
    cx.b.params = sig;
    cx.load_captures(e, f)?;
    cx.finish_with(&f.body)
}

/// The code of an `async` body (§8.8, compiler.md §8 question 3):
/// `env` is the task object, whose captures follow its fixed fields.
fn emit_async<'a>(
    p: &mut Program<'a>,
    owner: Inst,
    own: &'a BodyOwn,
    e: &'a Expr,
    body: &'a Expr,
    name: &str,
) -> R<()> {
    let mut cx = Cx::new(p, owner, own, name, vec![(LirTy::Ptr, "env".to_string())]);
    cx.env = Some(V::Val("env".into(), LirTy::Ptr));
    let own_c = cx
        .own
        .closures
        .get(&e.id)
        .ok_or_else(|| Unsupported("no plan for an async literal".into()))?;
    let caps = cx.capture_tys(own_c)?;
    let (_, sname) = cx.p.task_object(&cx.name.clone(), caps.clone());
    for (i, (c, t)) in own_c.captures.iter().zip(&caps).enumerate() {
        let slot = cx.gep(&sname, "env", crate::objects::TASK_CAPTURE0 + i);
        let v = cx.load(*t, &slot);
        cx.locals.insert(c.binding, Local::Val(v));
    }
    cx.finish_with(body)
}

/// Every `fn` and `async` literal of a body, by expression id.
pub fn literals(c: &fibref::own::Checked, key: BodyKey) -> HashMap<ExprId, &Expr> {
    let g = &c.typed.globals;
    let root: &Expr = match key {
        BodyKey::Fun(f) | BodyKey::AllOwned(f) => &g.fun(f).body,
        BodyKey::Method(i, m) | BodyKey::MethodOwned(i, m) => &g.instances[i].methods[m].body,
        BodyKey::Def(d) => &g.def(d).init,
    };
    let mut out = HashMap::new();
    let mut stack = vec![root];
    while let Some(e) = stack.pop() {
        if matches!(e.kind, ExprKind::Fn(_) | ExprKind::Async(..)) {
            out.insert(e.id, e);
        }
        stack.extend(fibref::eval::ast::children(e));
    }
    out
}

impl<'p, 'a> Cx<'p, 'a> {
    pub fn new(
        p: &'p mut Program<'a>,
        inst: Inst,
        own: &'a BodyOwn,
        name: &str,
        params: Vec<(LirTy, String)>,
    ) -> Self {
        let value_sites = ops::value_sites(own);
        let lits = literals(p.c, inst.key);
        Cx {
            p,
            inst,
            own,
            name: name.to_string(),
            b: FnBuilder::new(name, None, params, true),
            locals: HashMap::new(),
            values: HashMap::new(),
            value_sites,
            env: None,
            loops: Vec::new(),
            stack_slots: HashMap::new(),
            lits,
            rests: Vec::new(),
            closure_slots: HashSet::new(),
        }
    }

    /// Lowers the body, returns its value, and records the function.
    fn finish_with(mut self, body: &'a Expr) -> R<()> {
        let rt = self.ty(body)?;
        self.b.ret = self.p.lir(&rt)?;
        if let Flow::Val(v) = self.expr(body)? {
            match &v {
                V::Unit => self.b.term("(ret)"),
                V::Val(s, _) => self.b.term(&format!("(ret {s})")),
            }
        }
        let text = self.b.render();
        self.p.funcs.push(text);
        Ok(())
    }

    /// The concrete type of an expression.
    pub fn ty(&self, e: &Expr) -> R<Ty> {
        self.p.ty_of(&self.inst, e.id)
    }

    /// The concrete type of a binding.
    pub fn binding_ty(&self, b: BindingId) -> R<Ty> {
        self.p
            .c
            .typed
            .binding_types
            .get(&b)
            .map(|t| self.inst.subst(t))
            .ok_or_else(|| Unsupported(format!("binding {b:?} has no type")))
    }

    /// The value of a local.
    pub fn local(&mut self, b: BindingId) -> R<V> {
        match self.locals.get(&b).cloned() {
            Some(Local::Val(v)) => Ok(v),
            Some(Local::Slot(slot, Some(t))) => {
                Ok(self.b.val(&format!("(load {} {slot})", t.text()), t))
            }
            Some(Local::Slot(_, None)) => Ok(V::Unit),
            None => Err(Unsupported(format!(
                "unbound local {}",
                self.p.g().binding(b).name
            ))),
        }
    }

    /// Lowers an expression: its value, then the plan's operations.
    pub fn expr(&mut self, e: &'a Expr) -> R<Flow> {
        let flow = self.form(e)?;
        if let Flow::Val(v) = &flow {
            if self.value_sites.contains(&e.id) {
                self.values.insert(e.id, v.clone());
            }
            self.after(e.id)?;
        }
        Ok(flow)
    }

    /// Lowers an expression that must produce a value.
    pub fn value(&mut self, e: &'a Expr) -> R<V> {
        match self.expr(e)? {
            Flow::Val(v) => Ok(v),
            Flow::Jump => Err(Unsupported("a jump where a value is needed".into())),
        }
    }

    fn form(&mut self, e: &'a Expr) -> R<Flow> {
        let v = match &e.kind {
            ExprKind::Lit(l) => self.literal(e, l)?,
            ExprKind::Local(b) => self.local(*b)?,
            ExprKind::Global(g) => self.global(e, *g)?,
            ExprKind::Call(h, args) => return self.call(e, h, args),
            ExprKind::Fn(_) => self.make_fn(e)?,
            ExprKind::Let(bs, body) => return self.let_form(bs, body),
            ExprKind::If(c, t, f) => return self.if_form(e, c, t, f),
            ExprKind::Do(es) => return self.do_form(es),
            ExprKind::Match(s, cls) => return self.match_form(e, s, cls),
            ExprKind::Loop(vs, body) => return self.loop_form(e, vs, body),
            ExprKind::Recur(args) => return self.recur(e, args),
            ExprKind::Field(x, name, _) => self.field_of(x, name)?,
            ExprKind::Deref(pl, _) => self.deref_place(pl)?,
            ExprKind::Set(pl, v) => self.set_place(pl, v)?,
            ExprKind::SetField(b, f, v) => self.set_field(*b, f, v)?,
            ExprKind::Unsafe(b) => return self.expr(b),
            ExprKind::Dyn(pr, _, _, b) => self.dyn_of(*pr, b)?,
            ExprKind::Convert(op, t, x) => {
                let v = self.value(x)?;
                self.convert(*op, *t, &v)?
            }
            ExprKind::Async(..) => self.make_async(e)?,
            ExprKind::Await(x) => self.await_task(x)?,
            ExprKind::Quote(f) => self.quote(e, f)?,
            ExprKind::Concat(es) => self.concat(es)?,
        };
        Ok(Flow::Val(v))
    }

    fn literal(&mut self, e: &Expr, l: &Lit) -> R<V> {
        Ok(match l {
            Lit::Int(n, _) => {
                let t = self.ty(e)?;
                let l = self.p.lir(&t)?.unwrap_or(LirTy::I64);
                V::int(l, *n)
            }
            Lit::Float(x, w) => {
                let t = match w {
                    fibref::syntax::FltWidth::F32 => LirTy::Float,
                    fibref::syntax::FltWidth::F64 => LirTy::Double,
                };
                V::Val(format!("({} {})", t.text(), float_text(*x)), t)
            }
            Lit::Str(s) => V::Val(self.p.statics.string(s, 0), LirTy::Ptr),
            Lit::Char(c) => V::int(LirTy::I32, i64::from(u32::from(*c))),
            Lit::Bool(b) => V::int(LirTy::I1, i64::from(*b)),
            Lit::Keyword(k) => {
                let id = self.p.statics.keyword(k);
                V::int(LirTy::I64, id)
            }
            Lit::Unit => V::Unit,
        })
    }

    /// The object word of a value: the pointer itself, or a `dyn`'s
    /// first word; `None` for a scalar.
    pub fn obj_word(&mut self, v: &V) -> Option<String> {
        match v {
            V::Val(s, LirTy::Ptr) => Some(s.clone()),
            V::Val(s, LirTy::Dyn) => Some(
                self.b
                    .val(&format!("(extractvalue {s} 0)"), LirTy::Ptr)
                    .text()
                    .to_string(),
            ),
            _ => None,
        }
    }

    /// A trap with a C-string message: the block ends here.
    pub fn trap_c(&mut self, msg: &str) {
        self.b
            .stmt(&format!("(call @fib.trap-c (string \"{msg}\"))"));
        self.b.term("(unreachable)");
    }

    /// `(getelementptr %struct.S p (i32 0) (i32 i))`.
    pub fn gep(&mut self, sname: &str, p: &str, i: usize) -> String {
        self.b
            .val(
                &format!("(getelementptr %struct.{sname} {p} (i32 0) (i32 {i}))"),
                LirTy::Ptr,
            )
            .text()
            .to_string()
    }

    pub fn load(&mut self, t: LirTy, ptr: &str) -> V {
        self.b.val(&format!("(load {} {ptr})", t.text()), t)
    }

    pub fn store(&mut self, v: &V, ptr: &str) {
        if let V::Val(s, _) = v {
            self.b.stmt(&format!("(store {s} {ptr})"));
        }
    }

    /// The captures of a closure body, loaded from `env` in capture
    /// order into their bindings.
    fn load_captures(&mut self, e: &Expr, f: &FnLit) -> R<()> {
        let own = self
            .own
            .closures
            .get(&e.id)
            .ok_or_else(|| Unsupported("no plan for a fn literal".into()))?;
        let caps = self.capture_tys(own)?;
        let (_, sname) = self.p.closure_object(&self.name.clone(), caps.clone());
        for (i, (c, t)) in own.captures.iter().zip(&caps).enumerate() {
            let slot = self.gep(&sname, "env", crate::objects::CAPTURE0 + i);
            let v = self.load(*t, &slot);
            self.locals.insert(c.binding, Local::Val(v));
        }
        let _ = f;
        Ok(())
    }
}

/// A float literal as lIR writes it.
pub fn float_text(x: f64) -> String {
    if x.is_nan() {
        "nan".into()
    } else if x.is_infinite() {
        if x > 0.0 { "inf" } else { "-inf" }.into()
    } else {
        format!("{x:?}")
    }
}
