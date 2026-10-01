//! A tiny evaluator for expanded macro bodies, implementing
//! `MacroRunner` for the subset of fibber the adversarial tests use:
//! parameters, `quote`, `let`, `if`, `do`, the `Form` constructors
//! `List`/`Vec`/`Sym`, `concat`, the §1.4 prelude calls, `gensym`, `=` on
//! forms and the reflection calls.

use fibref::expand::{
    ExpandCtx, ExpandError, ExpandErrorKind as K, MacroDef, MacroRunner, REFLECTION_CALLS,
};
use fibref::syntax::{Form, FormKind, Pos};

/// A value of the mini evaluator.
#[derive(Clone, Debug)]
pub(crate) enum Val {
    F(Form),
    V(Vec<Form>),
}

pub(crate) struct Mini;

type Env = Vec<(String, Val)>;

fn fail(reason: &'static str, pos: &Pos) -> ExpandError {
    let kind = K::Malformed {
        head: "mini evaluator".into(),
        reason,
    };
    ExpandError::new(kind, pos)
}

impl MacroRunner for Mini {
    fn run(
        &mut self,
        def: &MacroDef,
        args: Vec<Form>,
        ctx: &ExpandCtx,
    ) -> Result<Form, ExpandError> {
        let mut env: Env = Vec::new();
        let mut args = args.into_iter();
        for p in &def.params {
            let a = args
                .next()
                .ok_or_else(|| fail("too few args", ctx.call_pos()))?;
            env.push((p.clone(), Val::F(a)));
        }
        if let Some(r) = &def.rest {
            env.push((r.clone(), Val::V(args.collect())));
        }
        let mut last = Val::V(Vec::new());
        for b in &def.body {
            last = eval(b, &mut env, ctx)?;
        }
        match last {
            Val::F(f) => Ok(f),
            Val::V(_) => Err(fail("macro returned a vector", ctx.call_pos())),
        }
    }
}

/// `f` with every position set to `pos` (a form the macro built, §1.3).
fn at(f: &Form, pos: &Pos) -> Form {
    let kind = match &f.kind {
        FormKind::List(i) => FormKind::List(i.iter().map(|x| at(x, pos)).collect()),
        FormKind::Vec(i) => FormKind::Vec(i.iter().map(|x| at(x, pos)).collect()),
        k => k.clone(),
    };
    Form::new(kind, pos.clone())
}

fn eval(f: &Form, env: &mut Env, ctx: &ExpandCtx) -> Result<Val, ExpandError> {
    let items = match &f.kind {
        FormKind::Sym(s) => {
            let found = env
                .iter()
                .rev()
                .find(|(n, _)| n == s)
                .map(|(_, v)| v.clone());
            return found.ok_or_else(|| fail("unbound symbol", &f.pos));
        }
        FormKind::List(items) if !items.is_empty() => items,
        _ => return Ok(Val::F(at(f, ctx.call_pos()))),
    };
    let head = items[0].as_sym().unwrap_or("");
    match head {
        "quote" => Ok(Val::F(at(&items[1], ctx.call_pos()))),
        "let" => eval_let(items, env, ctx),
        "if" => {
            let c = eval(&items[1], env, ctx)?;
            let yes = matches!(
                c,
                Val::F(Form {
                    kind: FormKind::Bool(true),
                    ..
                })
            );
            eval(&items[if yes { 2 } else { 3 }], env, ctx)
        }
        "do" => {
            let mut last = Val::V(Vec::new());
            for i in &items[1..] {
                last = eval(i, env, ctx)?;
            }
            Ok(last)
        }
        _ => eval_call(head, items, env, ctx),
    }
}

fn eval_let(items: &[Form], env: &mut Env, ctx: &ExpandCtx) -> Result<Val, ExpandError> {
    let depth = env.len();
    for pair in items[1].as_list().unwrap_or(&[]) {
        let p = pair.as_list().unwrap_or(&[]);
        let v = eval(&p[1], env, ctx)?;
        env.push((p[0].as_sym().unwrap_or("").to_string(), v));
    }
    let mut last = Val::V(Vec::new());
    for b in &items[2..] {
        last = eval(b, env, ctx)?;
    }
    env.truncate(depth);
    Ok(last)
}

fn form(v: Val, pos: &Pos) -> Result<Form, ExpandError> {
    match v {
        Val::F(f) => Ok(f),
        Val::V(_) => Err(fail("expected a form", pos)),
    }
}

fn vector(v: Val, pos: &Pos) -> Result<Vec<Form>, ExpandError> {
    match v {
        Val::V(v) => Ok(v),
        Val::F(Form {
            kind: FormKind::Vec(v),
            ..
        }) => Ok(v),
        Val::F(_) => Err(fail("expected a vector", pos)),
    }
}

fn eval_call(
    head: &str,
    items: &[Form],
    env: &mut Env,
    ctx: &ExpandCtx,
) -> Result<Val, ExpandError> {
    let pos = ctx.call_pos().clone();
    let mut args = Vec::new();
    for a in &items[1..] {
        args.push(eval(a, env, ctx)?);
    }
    // The quasiquote rewrite writes its heads as the prelude's
    // (`fib.prelude/concat`, `fib.prelude/List`, stdlib design §7 B2).
    if head == "concat" || head == "fib.prelude/concat" {
        let parts = args.into_iter().map(|a| vector(a, &pos));
        return Ok(Val::V(parts.collect::<Result<Vec<_>, _>>()?.concat()));
    }
    let mut args = args.into_iter();
    let mut next = || args.next().ok_or_else(|| fail("missing argument", &pos));
    let built = |kind: FormKind| Val::F(Form::new(kind, pos.clone()));
    Ok(match head {
        "fib.prelude/vec-empty" => Val::V(Vec::new()),
        "fib.prelude/conj" => {
            let mut v = vector(next()?, &pos)?;
            v.push(form(next()?, &pos)?);
            Val::V(v)
        }
        "List" | "fib.prelude/List" => built(FormKind::List(vector(next()?, &pos)?)),
        "Vec" | "fib.prelude/Vec" => built(FormKind::Vec(vector(next()?, &pos)?)),
        "Sym" => built(FormKind::Sym(string(next()?, &pos)?)),
        "gensym" => Val::F(ctx.gensym(&string(next()?, &pos)?, &pos)),
        "=" => {
            let (a, b) = (form(next()?, &pos)?, form(next()?, &pos)?);
            built(FormKind::Bool(a == b))
        }
        op if REFLECTION_CALLS.contains(&op) => {
            Val::F(ctx.reflect(op, &form(next()?, &pos)?, &pos)?)
        }
        _ => return Err(fail("unknown function", &items[0].pos)),
    })
}

fn string(v: Val, pos: &Pos) -> Result<String, ExpandError> {
    match form(v, pos)?.kind {
        FormKind::Str(s) => Ok(s),
        _ => Err(fail("expected a string", pos)),
    }
}
