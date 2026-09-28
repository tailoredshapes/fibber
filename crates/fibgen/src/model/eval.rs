//! The model's evaluator over the generated tree.

use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};
use std::rc::Rc;

use crate::ast::{Arg, Expr, FunDef, Kind, Method, Pat, Program};

use super::value::{Closure, TaskState, V};
use super::ModelError;

/// The most expression evaluations one program may take.
const STEP_BUDGET: u64 = 2_000_000;

/// The deepest nesting of calls the model follows (generated recursion
/// is a few levels; a shrunk program may recurse without end).
const DEPTH_BUDGET: u32 = 400;

/// A persistent environment: newest binding first.
#[derive(Clone, Debug, Default)]
pub struct Env(Option<Rc<Node>>);

#[derive(Debug)]
struct Node {
    name: String,
    val: V,
    next: Env,
}

impl Env {
    /// This environment with `name` bound to `val`.
    pub fn bind(&self, name: &str, val: V) -> Env {
        Env(Some(Rc::new(Node {
            name: name.to_string(),
            val,
            next: self.clone(),
        })))
    }

    /// The newest binding of `name`.
    pub fn get(&self, name: &str) -> Option<&V> {
        let mut cur = self;
        while let Some(n) = &cur.0 {
            if n.name == name {
                return Some(&n.val);
            }
            cur = &n.next;
        }
        None
    }
}

/// Why evaluation left an expression other than with a value.
#[derive(Debug)]
pub enum Stop {
    /// The model's own failure.
    Model(ModelError),
    /// A `recur` on its way to its loop.
    Recur(Vec<V>),
}

impl From<ModelError> for Stop {
    fn from(e: ModelError) -> Self {
        Stop::Model(e)
    }
}

/// The evaluation result.
pub type Res = Result<V, Stop>;

/// Evaluator state: the helper functions and the step count.
pub struct Machine<'p> {
    pub(super) funs: HashMap<&'p str, &'p FunDef>,
    /// The values of the `def`s evaluated so far.
    pub(super) globals: HashMap<String, V>,
    /// The `impl` methods, by (method, implementing type's head).
    pub(super) impls: HashMap<(&'p str, &'static str), &'p Method>,
    /// What the run did that a static count of the program cannot see
    /// (a guard that failed, a default method that ran, ...).
    pub(super) trace: BTreeSet<&'static str>,
    steps: u64,
    depth: u32,
}

/// A trap.
pub fn trap(msg: &str) -> Stop {
    Stop::Model(ModelError::Trap(msg.to_string()))
}

/// Something the model does not implement.
pub fn unsupported(msg: String) -> Stop {
    Stop::Model(ModelError::Unsupported(msg))
}

impl<'p> Machine<'p> {
    /// A machine for `p`.
    pub fn new(p: &'p Program) -> Self {
        let funs = p.funs.iter().map(|f| (f.name.as_str(), f)).collect();
        Machine {
            funs,
            globals: HashMap::new(),
            impls: super::protos::impl_table(p),
            trace: BTreeSet::new(),
            steps: 0,
            depth: 0,
        }
    }

    /// Evaluates `e` to a value (a stray `recur` is a model error), one
    /// call level deeper.
    pub fn eval(&mut self, e: &Expr, env: &Env) -> Result<V, ModelError> {
        if self.depth >= DEPTH_BUDGET {
            return Err(ModelError::Budget);
        }
        self.depth += 1;
        let r = self.ev(e, env);
        self.depth -= 1;
        match r {
            Ok(v) => Ok(v),
            Err(Stop::Model(m)) => Err(m),
            Err(Stop::Recur(_)) => Err(ModelError::Unsupported("recur outside loop".into())),
        }
    }

    /// Evaluates `e`.
    pub fn ev(&mut self, e: &Expr, env: &Env) -> Res {
        self.steps += 1;
        if self.steps > STEP_BUDGET {
            return Err(ModelError::Budget.into());
        }
        match &e.kind {
            Kind::Int(n) => Ok(V::Int(*n)),
            Kind::Bool(b) => Ok(V::Bool(*b)),
            Kind::Str(s) => Ok(V::str(s)),
            Kind::Unit => Ok(V::Unit),
            Kind::Nil => Ok(V::Opt(None)),
            Kind::Empty => Ok(V::List(Rc::new(Vec::new()))),
            Kind::VecLit(es) => Ok(V::Vector(Rc::new(self.ev_all(es, env)?))),
            Kind::Var(n) => env
                .get(n)
                .or_else(|| self.globals.get(n))
                .cloned()
                .ok_or_else(|| unsupported(format!("unbound {n}"))),
            Kind::Global(n) => Ok(V::Named(Rc::from(n.as_str()))),
            Kind::Let(bs, body) => self.ev_let(bs, body, env),
            Kind::If(c, t, f) => match self.ev(c, env)? {
                V::Bool(true) => self.ev(t, env),
                V::Bool(false) => self.ev(f, env),
                v => Err(unsupported(format!("if on {v:?}"))),
            },
            Kind::Do(es) => {
                let mut last = V::Unit;
                for s in es {
                    last = self.ev(s, env)?;
                }
                Ok(last)
            }
            Kind::Match(s, cl) => self.ev_match(s, cl, env),
            Kind::Loop(bs, body) => self.ev_loop(bs, body, env),
            Kind::Recur(es) => Err(Stop::Recur(self.ev_all(es, env)?)),
            _ => self.ev_more(e, env),
        }
    }

    fn ev_more(&mut self, e: &Expr, env: &Env) -> Res {
        match &e.kind {
            Kind::Fn(ps, body) => Ok(closure(None, ps, body, env)),
            Kind::FnNamed(n, ps, body) => Ok(closure(Some(n.clone()), ps, body, env)),
            Kind::Call(h, args) => self.ev_call(h, args, env),
            Kind::Apply(f, args) => {
                let f = self.ev(f, env)?;
                let args = self.ev_all(args, env)?;
                self.apply(&f, args)
            }
            Kind::Field(s, f) => field(&self.ev(s, env)?, f),
            Kind::Deref(c) => deref(&self.ev(c, env)?),
            Kind::Set(t, v) => {
                let t = self.ev(t, env)?;
                let v = self.ev(v, env)?;
                match t {
                    V::Cell(c) => {
                        *c.borrow_mut() = v;
                        Ok(V::Unit)
                    }
                    t => Err(unsupported(format!("set! on {t:?}"))),
                }
            }
            Kind::SetField(c, f, v) => self.ev_set_field(c, f, v, env),
            Kind::Async(b) => Ok(V::Task(Rc::new(RefCell::new(TaskState::Pending(
                (**b).clone(),
                env.clone(),
            ))))),
            Kind::Await(t) => {
                let t = self.ev(t, env)?;
                self.force(&t)
            }
            Kind::Plet(bs, body) => {
                let mut env2 = env.clone();
                for (n, init) in bs {
                    let v = self.ev(init, env)?;
                    env2 = env2.bind(n, v);
                }
                self.ev(body, &env2)
            }
            Kind::WeakDead(_, t) => {
                self.ev(t, env)?;
                Ok(V::Weak(None))
            }
            Kind::Dyn(..) | Kind::GMatch(..) | Kind::Macro(..) => self.ev_new(e, env),
            _ => Err(unsupported(format!("node {:?}", e.kind))),
        }
    }

    fn ev_all(&mut self, es: &[Expr], env: &Env) -> Result<Vec<V>, Stop> {
        es.iter().map(|e| self.ev(e, env)).collect()
    }

    fn ev_let(&mut self, bs: &[(Pat, Expr)], body: &Expr, env: &Env) -> Res {
        let mut env2 = env.clone();
        for (p, init) in bs {
            let v = self.ev(init, &env2)?;
            env2 = bind_pat(p, &v, &env2)
                .ok_or_else(|| unsupported(format!("refutable let pattern {p:?}")))?;
        }
        self.ev(body, &env2)
    }

    fn ev_match(&mut self, s: &Expr, cl: &[(Pat, Expr)], env: &Env) -> Res {
        let v = self.ev(s, env)?;
        for (p, body) in cl {
            if let Some(env2) = bind_pat(p, &v, env) {
                return self.ev(body, &env2);
            }
        }
        Err(trap("match: no clause"))
    }

    fn ev_loop(&mut self, bs: &[(String, Expr)], body: &Expr, env: &Env) -> Res {
        let mut env2 = env.clone();
        for (n, init) in bs {
            let v = self.ev(init, &env2)?;
            env2 = env2.bind(n, v);
        }
        loop {
            match self.ev(body, &env2) {
                Err(Stop::Recur(vals)) => {
                    let mut next = env.clone();
                    for ((n, _), v) in bs.iter().zip(vals) {
                        next = next.bind(n, v);
                    }
                    env2 = next;
                }
                other => return other,
            }
        }
    }

    fn ev_set_field(&mut self, c: &str, f: &str, v: &Expr, env: &Env) -> Res {
        let v = self.ev(v, env)?;
        let Some(V::Cell(cell)) = env.get(c).cloned() else {
            return Err(unsupported(format!("set-field! on {c}")));
        };
        let old = cell.borrow().clone();
        let V::Data(name, fields) = old else {
            return Err(unsupported("set-field! on a non-struct".into()));
        };
        let i = field_index(&name, f)?;
        let mut fields = (*fields).clone();
        fields[i] = v;
        *cell.borrow_mut() = V::Data(name, Rc::new(fields));
        Ok(V::Unit)
    }

    /// A call by name: `&` arguments are copied in at their position and
    /// written back, in parameter order, after the call (syntax §3.13).
    fn ev_call(&mut self, h: &str, args: &[Arg], env: &Env) -> Res {
        let mut vals = Vec::new();
        let mut backs = Vec::new();
        for a in args {
            match a {
                Arg::Val(e) => vals.push(self.ev(e, env)?),
                Arg::InOut(n) => {
                    let Some(V::Cell(outer)) = env.get(n).cloned() else {
                        return Err(unsupported(format!("&{n} is not a cell")));
                    };
                    let private = Rc::new(RefCell::new(outer.borrow().clone()));
                    backs.push((outer, private.clone()));
                    vals.push(V::Cell(private));
                }
            }
        }
        let result = self.call_named(h, vals)?;
        for (outer, private) in backs {
            let v = private.borrow().clone();
            *outer.borrow_mut() = v;
        }
        Ok(result)
    }

    /// Calls the helper or builtin `h`.
    pub fn call_named(&mut self, h: &str, vals: Vec<V>) -> Res {
        match self.funs.get(h).copied() {
            Some(f) => {
                let mut env = Env::default();
                for (p, v) in f.params.iter().zip(vals) {
                    env = env.bind(&p.name, v);
                }
                Ok(self.eval(&f.body, &env)?)
            }
            None => self.builtin(h, vals),
        }
    }

    /// Calls a function value.
    pub fn apply(&mut self, f: &V, args: Vec<V>) -> Res {
        match f {
            V::Closure(c) => {
                let mut env = c.env.clone();
                if let Some(n) = &c.self_name {
                    env = env.bind(n, V::Closure(c.clone()));
                }
                for (p, v) in c.params.iter().zip(args) {
                    env = env.bind(p, v);
                }
                Ok(self.eval(&c.body, &env)?)
            }
            V::Named(n) => self.call_named(n, args),
            v => Err(unsupported(format!("apply {v:?}"))),
        }
    }

    /// Runs a task to completion (once) and returns its result.
    pub fn force(&mut self, t: &V) -> Res {
        let V::Task(cell) = t else {
            return Err(unsupported(format!("await {t:?}")));
        };
        let state = std::mem::replace(&mut *cell.borrow_mut(), TaskState::Done(V::Unit));
        let v = match state {
            TaskState::Done(v) => v,
            TaskState::Pending(body, env) => self.eval(&body, &env)?,
            TaskState::Thunk(f) => self.apply(&f, Vec::new())?,
        };
        *cell.borrow_mut() = TaskState::Done(v.clone());
        Ok(v)
    }
}

fn closure(self_name: Option<String>, ps: &[(String, crate::ty::Ty)], body: &Expr, env: &Env) -> V {
    V::Closure(Rc::new(Closure {
        self_name,
        params: ps.iter().map(|(p, _)| p.clone()).collect(),
        body: body.clone(),
        env: env.clone(),
    }))
}

/// Binds `p` against `v`, or `None` if it does not match.
pub fn bind_pat(p: &Pat, v: &V, env: &Env) -> Option<Env> {
    match (p, v) {
        (Pat::Wild, _) => Some(env.clone()),
        (Pat::Bind(n), v) => Some(env.bind(n, v.clone())),
        (Pat::Nil, V::Opt(None)) => Some(env.clone()),
        (Pat::Some(q), V::Opt(Some(x))) => bind_pat(q, x, env),
        (Pat::Lit(n), V::Int(m)) if n == m => Some(env.clone()),
        (Pat::Vector(ps, rest), V::Vector(xs)) => super::patterns::bind_vector(ps, rest, xs, env),
        (Pat::As(q, n), v) => bind_pat(q, v, &env.bind(n, v.clone())),
        (Pat::Ctor(c, ps), V::Data(name, fields)) if **name == **c => {
            let mut env2 = env.clone();
            for (q, x) in ps.iter().zip(fields.iter()) {
                env2 = bind_pat(q, x, &env2)?;
            }
            Some(env2)
        }
        (Pat::Ctor(c, ps), V::List(items)) => match (c.as_str(), items.split_first()) {
            ("empty", None) => Some(env.clone()),
            ("cons", Some((h, t))) if ps.len() == 2 => {
                let env2 = bind_pat(&ps[0], h, env)?;
                bind_pat(&ps[1], &V::List(Rc::new(t.to_vec())), &env2)
            }
            _ => None,
        },
        _ => None,
    }
}

/// The position of field `f` in struct `s`.
pub fn field_index(s: &str, f: &str) -> Result<usize, Stop> {
    match (s, f) {
        ("Pt", "x") | ("Wrap", "s") | ("Holder", "f") | ("Box", "v") | ("Hook", "f") => Ok(0),
        ("Pt", "y") | ("Wrap", "v") | ("Holder", "c") | ("Hook", "tag") => Ok(1),
        _ => Err(unsupported(format!("field {s}.{f}"))),
    }
}

fn field(v: &V, f: &str) -> Res {
    match v {
        V::Data(name, fields) => Ok(fields[field_index(name, f)?].clone()),
        v => Err(unsupported(format!("field {f} of {v:?}"))),
    }
}

/// `@v` on a cell, atom or weak reference.
pub fn deref(v: &V) -> Res {
    match v {
        V::Cell(c) | V::Atom(c) => Ok(c.borrow().clone()),
        V::Weak(w) => Ok(V::Opt(w.clone())),
        v => Err(unsupported(format!("deref {v:?}"))),
    }
}
