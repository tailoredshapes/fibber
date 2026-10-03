//! Globals and calls (spec/types.md §2.1, §2.2): instantiating schemes,
//! `&` positions, constants that are not functions, annotations.

use crate::syntax::{IntWidth, Pos};

use crate::types::annot::{ann_to_ty, RigidEnv};
use crate::types::ast::{Arg, Expr, ExprId, ExprKind, GlobalRef, Lit, TypeAnn};
use crate::types::decls::{ModuleId, Shape};
use crate::types::error::{ErrorKind, TResult, TypeError};
use crate::types::scheme::{ColourBound, Scheme};
use crate::types::ty::{Colour, Pred, Ty};

use super::cx::{CapsCon, ColourCon, Cx, DKind, Instantiation, MonoSig};
use super::expr::amp_not_cell;
use super::polyrec::PolyCall;

/// A function's signature at a call: parameter types, `&` flags, names.
struct Sig {
    params: Vec<Ty>,
    amps: Vec<bool>,
    names: Vec<String>,
    ret: Ty,
    name: String,
    /// Whether the function is the library's (§7 D1 says what its arity
    /// errors may add).
    library: bool,
}

/// The stand-in a library function has for the arity a call wrote, until
/// arity overloading lands (stdlib §7 L1, diagnostic D1): the function's
/// name, the number of arguments the call has, and the function that takes
/// them. `(get m k 0)` is `get takes 2 argument(s), got 3; use get-or`.
const STAND_INS: &[(&str, usize, &str)] = &[
    ("get", 3, "get-or"),
    ("nth", 3, "nth-or"),
    ("reduce", 2, "reduce1"),
    ("sort", 2, "sort-with"),
    ("range", 3, "range-by"),
];

/// The words `; use X` a library function's arity error ends with when
/// the call has the arguments of its stand-in, else nothing.
fn stand_in_hint(name: &str, found: usize) -> String {
    STAND_INS
        .iter()
        .find(|(n, k, _)| *n == name && *k == found)
        .map(|(_, _, stand_in)| format!("; use {stand_in}"))
        .unwrap_or_default()
}

impl Cx<'_> {
    /// Converts an annotation in the current definition: named variables
    /// are its rigid variables, omitted colours fresh colour variables.
    pub fn ann(&mut self, a: &TypeAnn, pos: &Pos) -> TResult<Ty> {
        let st = &mut *self.st;
        let mut colour = || st.fresh_colour();
        let mut env = RigidEnv {
            map: &mut self.u.rigid_map,
            names: &mut self.u.rigid_names,
            colour: &mut colour,
            colour_names: &mut self.u.rigid_colours,
        };
        ann_to_ty(a, &mut env, pos)
    }

    /// Instantiates `s` for the use `site` of `who`, emitting its
    /// context and colour constraints.
    pub fn instantiate(
        &mut self,
        s: &Scheme,
        site: ExprId,
        who: &str,
        pos: &Pos,
        method: Option<usize>,
    ) -> Ty {
        let tys: Vec<Ty> = (0..s.n_vars).map(|_| self.st.fresh()).collect();
        let colours: Vec<Colour> = (0..s.n_colours).map(|_| self.st.fresh_colour()).collect();
        let inst = s.instantiate_with(&tys, &colours);
        for (i, p) in inst.preds.into_iter().enumerate() {
            let dispatch = method.filter(|_| i == 0);
            let kind = match p {
                Pred::Proto(q, args) => DKind::Proto(q, args, dispatch),
                Pred::Send(t) => DKind::Send(t, vec![send_root(s, &p_var(&s.preds[i]), who)]),
                Pred::Object(t) => DKind::Object(t, who.to_string()),
                Pred::Weakable(t) => DKind::Weakable(t),
            };
            let site = dispatch.map(|_| site);
            self.defer(kind, pos, site);
        }
        for b in inst.colour_bounds {
            match b {
                ColourBound::Flow(a, c) => self.u.colours.push(ColourCon {
                    from: a,
                    to: c,
                    label: None,
                    origin: None,
                    pos: pos.clone(),
                }),
                ColourBound::Caps(k, t) => {
                    let label = format!("a capture of a closure made by {who}");
                    self.u.caps.push(CapsCon {
                        colour: k,
                        ty: t,
                        label,
                        pos: pos.clone(),
                    });
                }
            }
        }
        self.t
            .instantiations
            .insert(site, Instantiation { tys, colours });
        self.u.insts.push(site);
        inst.ty
    }

    /// The type of a global name used as a value (§2.1).
    pub fn global_value(&mut self, id: ExprId, r: GlobalRef, pos: &Pos) -> TResult<Ty> {
        if let Some(sig) = self.signature(id, r, pos)? {
            if sig.amps.iter().any(|a| *a) {
                let msg = "function with & parameters is not a value";
                return Err(TypeError::new(ErrorKind::AmpFunctionValue, pos, msg));
            }
            return Ok(Ty::Fn(Colour::Send, sig.params, Box::new(sig.ret)));
        }
        match r {
            GlobalRef::Def(d) => match self.env.defs.get(d.0 as usize).cloned().flatten() {
                Some(t) => Ok(t),
                None => Err(TypeError::other(
                    pos,
                    format!("def {} is not typed yet", self.g.def(d).name),
                )),
            },
            GlobalRef::Ctor(t, v) => {
                let s = ctor_scheme(self.g, t, v);
                Ok(self.instantiate(&s, id, &self.g.ty(t).name.clone(), pos, None))
            }
            _ => Err(TypeError::other(pos, "internal: global without a type")),
        }
    }

    /// Where a global's type comes from: its scheme (with its name and,
    /// for a method, its index), or a monomorphic SCC member's signature.
    fn source(&mut self, r: GlobalRef, pos: &Pos) -> TResult<Source> {
        let g = self.g;
        Ok(match r {
            GlobalRef::Fun(f) => {
                let name = g.fun(f).name.clone();
                if let Some(s) = self.u.poly.get(&f).cloned() {
                    self.u.poly_used.insert(f);
                    Source::Scheme(s, name, None)
                } else if let Some(m) = self.u.mono.get(&f).cloned() {
                    Source::Mono(mono_sig(m, name))
                } else {
                    match self.env.funs.get(f.0 as usize).cloned().flatten() {
                        Some(s) => Source::Scheme(s, name, None),
                        None => {
                            return Err(TypeError::other(
                                pos,
                                format!("internal: {name} has no scheme"),
                            ))
                        }
                    }
                }
            }
            GlobalRef::Method(p, m) => {
                let md = &g.proto(p).methods[m];
                Source::Scheme(md.scheme.clone(), md.name.clone(), Some(m))
            }
            GlobalRef::Builtin(b) => {
                let name = crate::types::builtins::BUILTINS[b.0 as usize]
                    .name
                    .to_string();
                Source::Scheme(g.builtin_schemes[b.0 as usize].clone(), name, None)
            }
            GlobalRef::Extern(x) => Source::Scheme(
                Scheme::mono(g.ext(x).ty.clone()),
                g.ext(x).name.clone(),
                None,
            ),
            GlobalRef::Ctor(t, v) if ctor_is_fn(g, t, v) => {
                Source::Scheme(ctor_scheme(g, t, v), variant_name(g, t, v), None)
            }
            _ => Source::Value,
        })
    }

    /// Whether the function or method `r` is the library's: defined in
    /// the prelude or in a module named `fib.*` (the loader's rule for the
    /// modules that are the library's own, `sees_implicit`). A program's
    /// own `get` is not, whatever its arity.
    fn is_library(&self, r: GlobalRef) -> bool {
        let module = match r {
            GlobalRef::Fun(f) => self.g.fun(f).module,
            GlobalRef::Method(p, _) => self.g.proto(p).module,
            _ => return false,
        };
        module == ModuleId::PRELUDE || self.g.modules[module.0 as usize].ns.starts_with("fib.")
    }

    /// The signature of a global function, instantiated; `None` for a
    /// `def` or a constructor that is a value.
    fn signature(&mut self, id: ExprId, r: GlobalRef, pos: &Pos) -> TResult<Option<Sig>> {
        let (scheme, name, method) = match self.source(r, pos)? {
            Source::Scheme(s, name, method) => (s, name, method),
            Source::Mono(sig) => {
                let library = self.is_library(r);
                return Ok(Some(Sig { library, ..sig }));
            }
            Source::Value => return Ok(None),
        };
        let ty = self.instantiate(&scheme, id, &name, pos, method);
        if let (GlobalRef::Fun(callee), Some(caller)) = (r, self.u.cur) {
            if self.u.poly.contains_key(&callee) {
                let call = PolyCall {
                    caller,
                    callee,
                    site: id,
                    pos: pos.clone(),
                };
                self.u.poly_calls.push(call);
            }
        }
        let Ty::Fn(_, params, ret) = ty else {
            return Err(TypeError::other(
                pos,
                format!("internal: {name} is not a function"),
            ));
        };
        let amps = if scheme.amps.is_empty() {
            vec![false; params.len()]
        } else {
            scheme.amps.clone()
        };
        Ok(Some(Sig {
            params,
            amps,
            names: scheme.params.clone(),
            ret: *ret,
            name,
            library: self.is_library(r),
        }))
    }

    /// A call `(head args..)` (§2.2).
    pub fn call(&mut self, e: &Expr, head: &Expr, args: &[Arg]) -> TResult<Ty> {
        if let ExprKind::Global(r) = &head.kind {
            if let GlobalRef::Ctor(t, v) = r {
                if !ctor_is_fn(self.g, *t, *v) {
                    let name = variant_name(self.g, *t, *v);
                    let msg = format!("{name} is a constant, not a function; write {name}");
                    return Err(TypeError::new(ErrorKind::ConstantCalled, &head.pos, msg));
                }
            }
            if let Some(sig) = self.signature(head.id, *r, &head.pos)? {
                let fty = Ty::Fn(Colour::Send, sig.params.clone(), Box::new(sig.ret.clone()));
                self.record(head.id, &fty);
                return self.apply(e, sig, args);
            }
        }
        let ht = self.infer(head)?;
        let params: Vec<Ty> = args.iter().map(|_| self.st.fresh()).collect();
        let ret = self.fresh();
        let k = self.st.fresh_colour();
        let want = Ty::Fn(k, params.clone(), Box::new(ret.clone()));
        self.unify(&ht, &want, &head.pos)?;
        let names = (1..=args.len()).map(|i| i.to_string()).collect();
        let sig = Sig {
            params,
            amps: vec![false; args.len()],
            names,
            ret,
            name: "the function".into(),
            library: false,
        };
        self.apply(e, sig, args)
    }

    fn apply(&mut self, e: &Expr, sig: Sig, args: &[Arg]) -> TResult<Ty> {
        if sig.params.len() != args.len() {
            let hint = if sig.library {
                stand_in_hint(&sig.name, args.len())
            } else {
                String::new()
            };
            let msg = format!(
                "{} takes {} argument(s), got {}{hint}",
                sig.name,
                sig.params.len(),
                args.len()
            );
            return Err(TypeError::other(&e.pos, msg));
        }
        let mut lits: Vec<Ty> = Vec::new();
        for (i, a) in args.iter().enumerate() {
            let pname = sig
                .names
                .get(i)
                .cloned()
                .unwrap_or_else(|| (i + 1).to_string());
            match (a, sig.amps[i]) {
                (Arg::Expr(x), false) => {
                    let t = match &x.kind {
                        // An integer literal in an argument position
                        // adopts a float type it unifies with (L19);
                        // else it is `i64`, below.
                        ExprKind::Lit(Lit::Int(n, IntWidth::I64)) => {
                            let v = self.st.fresh_lit(*n);
                            self.record(x.id, &v);
                            lits.push(v.clone());
                            v
                        }
                        _ => self.infer(x)?,
                    };
                    self.flow(&t, &sig.params[i], &x.pos)?;
                }
                (Arg::Amp(b, pos), true) => {
                    let t = self.binding(*b, pos)?;
                    if self
                        .unify(&t, &Ty::cell(sig.params[i].clone()), pos)
                        .is_err()
                    {
                        return Err(amp_not_cell(pos));
                    }
                }
                (Arg::Expr(x), true) => {
                    let msg = format!("parameter {pname} of {} is &; pass &x", sig.name);
                    return Err(TypeError::new(ErrorKind::AmpPosition, &x.pos, msg));
                }
                (Arg::Amp(_, pos), false) => {
                    let msg = format!("parameter {pname} of {} is not &; pass x", sig.name);
                    return Err(TypeError::new(ErrorKind::AmpPosition, pos, msg));
                }
            }
        }
        if !lits.is_empty() && !self.u.deferred.is_empty() {
            // What the arguments determine (`Reducible c e`) comes first.
            self.solve_all()?;
        }
        for v in lits {
            if matches!(self.st.resolve(&v), Ty::Var(_)) {
                self.unify(&v, &Ty::i64(), &e.pos)?;
            }
        }
        Ok(sig.ret)
    }
}

/// See [`Cx::source`].
enum Source {
    Scheme(Scheme, String, Option<usize>),
    Mono(Sig),
    Value,
}

fn mono_sig(m: MonoSig, name: String) -> Sig {
    Sig {
        params: m.params,
        amps: m.amps,
        names: m.names,
        ret: m.ret,
        name,
        library: false,
    }
}

/// The variable a `Send` bound is on, for its path.
fn p_var(p: &Pred) -> Ty {
    match p {
        Pred::Send(t) => t.clone(),
        _ => Ty::unit(),
    }
}

/// `type argument a of f`.
fn send_root(s: &Scheme, t: &Ty, who: &str) -> String {
    match t {
        Ty::Gen(i) => {
            let name = s
                .var_names
                .get(*i as usize)
                .cloned()
                .unwrap_or_else(|| format!("t{i}"));
            format!("type argument {name} of {who}")
        }
        _ => format!("a type argument of {who}"),
    }
}

/// Whether a constructor is called (a struct, a variant with fields).
pub fn ctor_is_fn(
    g: &crate::types::decls::Globals,
    t: crate::types::ty::TypeId,
    v: Option<usize>,
) -> bool {
    match (&g.ty(t).shape, v) {
        (Shape::Struct(_), None) => true,
        (Shape::Enum(vs), Some(i)) => vs.get(i).is_some_and(|x| !x.fields.is_empty()),
        _ => false,
    }
}

fn variant_name(
    g: &crate::types::decls::Globals,
    t: crate::types::ty::TypeId,
    v: Option<usize>,
) -> String {
    match (&g.ty(t).shape, v) {
        (Shape::Enum(vs), Some(i)) => vs.get(i).map(|x| x.name.clone()).unwrap_or_default(),
        _ => g.ty(t).name.clone(),
    }
}

/// `N : ∀ā. (fn :send (T₁ ..) (N ā))`, or `∀ā. (N ā)` for a field-less
/// variant (§2.7).
pub fn ctor_scheme(
    g: &crate::types::decls::Globals,
    t: crate::types::ty::TypeId,
    v: Option<usize>,
) -> Scheme {
    let def = g.ty(t);
    let result = Ty::nominal(t, def.head_args());
    let fields = match (&def.shape, v) {
        (Shape::Struct(fs), None) => fs.clone(),
        (Shape::Enum(vs), Some(i)) => vs.get(i).map(|x| x.fields.clone()).unwrap_or_default(),
        _ => Vec::new(),
    };
    let mut s = Scheme::mono(result.clone());
    s.n_vars = def.params.len() as u32;
    s.var_names = def.params.clone();
    if !def.colours.is_empty() {
        // Colour parameter `i` is `Colour::Gen(i)` in the field types
        // and the result (§1.3).
        s.n_colours = def.params.len() as u32;
    }
    if ctor_is_fn(g, t, v) {
        s.ty = Ty::Fn(
            Colour::Send,
            fields.iter().map(|f| f.ty.clone()).collect(),
            Box::new(result),
        );
        s.amps = vec![false; fields.len()];
        s.params = fields.iter().map(|f| f.name.clone()).collect();
    }
    s
}
