//! Lowering expressions (syntax §3): atoms, names and the core forms.
//! Calls and the builtins with special operands are in `call`.

use crate::syntax::{Form, FormKind};

use crate::types::ast::{BindingKind, Expr, ExprKind, FnLit, GlobalRef, Lit};
use crate::types::error::{ErrorKind, TResult, TypeError};

use super::decl::annotation_name;
use super::pattern::let_pattern;
use super::scope::Lowerer;
use super::typeform::type_ann;

/// A literal form's value, if it is one.
pub fn literal(form: &Form) -> Option<Lit> {
    Some(match &form.kind {
        FormKind::Int { v, width } => Lit::Int(*v, *width),
        FormKind::Flt { v, width } => Lit::Float(*v, *width),
        FormKind::Str(s) => Lit::Str(s.clone()),
        FormKind::Chr(c) => Lit::Char(*c),
        FormKind::Bool(b) => Lit::Bool(*b),
        FormKind::Kw(k) => Lit::Keyword(k.clone()),
        FormKind::List(items) if items.is_empty() => Lit::Unit,
        _ => return None,
    })
}

impl Lowerer<'_> {
    /// Lowers `form` in expression position; `tail` says whether it is
    /// in tail position of the innermost loop body (for `recur`).
    pub fn expr(&mut self, form: &Form, tail: bool) -> TResult<Expr> {
        if let Some(lit) = literal(form) {
            return Ok(self.mk(&form.pos, ExprKind::Lit(lit)));
        }
        match &form.kind {
            FormKind::Nil => {
                let nil = GlobalRef::Ctor(self.g.option, Some(0));
                Ok(self.mk(&form.pos, ExprKind::Global(nil)))
            }
            FormKind::Sym(name) => self.name(name, form),
            FormKind::List(items) => self.list(items, form, tail),
            _ => Err(TypeError::resolve(
                &form.pos,
                format!("{form} is not an expression after expansion"),
            )),
        }
    }

    /// A body of one or more forms, as one expression (a `do` when
    /// there are several).
    pub fn body(&mut self, forms: &[Form], pos: &crate::syntax::Pos, tail: bool) -> TResult<Expr> {
        if let [one] = forms {
            return self.expr(one, tail);
        }
        let mut steps = Vec::new();
        for (i, f) in forms.iter().enumerate() {
            steps.push(self.expr(f, tail && i + 1 == forms.len())?);
        }
        Ok(self.mk(pos, ExprKind::Do(steps)))
    }

    /// A name in expression position.
    fn name(&mut self, name: &str, form: &Form) -> TResult<Expr> {
        if name == "_" {
            return Err(TypeError::resolve(&form.pos, "_ is not an expression"));
        }
        if let Some(b) = self.lookup(name) {
            if self.kind(b) == BindingKind::AmpParam {
                let msg = format!("& parameter {name} used as a value in {}", self.owner);
                return Err(TypeError::new(ErrorKind::AmpParamValue, &form.pos, msg));
            }
            return Ok(self.mk(&form.pos, ExprKind::Local(b)));
        }
        if self.is_primitive_form(name) {
            return Err(self.primitive_value(name, form));
        }
        match self.g.value(self.m, name) {
            Some(r) => {
                self.check_unsafe(r, name, form)?;
                Ok(self.mk(&form.pos, ExprKind::Global(r)))
            }
            None => Err(TypeError::resolve(
                &form.pos,
                format!("unbound name {name}"),
            )),
        }
    }

    /// A list form: a core form or a call.
    fn list(&mut self, items: &[Form], form: &Form, tail: bool) -> TResult<Expr> {
        let head = items[0].as_sym().unwrap_or("");
        let pos = &form.pos;
        match head {
            "if" => self.if_form(items, form, tail),
            "do" => self.body_or_unit(&items[1..], form, tail),
            "let" => self.let_form(items, form, tail),
            "loop" => self.loop_form(items, form, tail),
            "recur" => self.recur(items, form, tail),
            "match" => self.match_form(items, form, tail),
            "fn" => self.fn_form(items, form),
            "async" => self.async_form(items, form),
            "await" => self.await_form(items, form),
            "unsafe" => {
                self.unsafe_depth += 1;
                let body = self.body_or_unit(&items[1..], form, false);
                self.unsafe_depth -= 1;
                let body = body?;
                Ok(self.mk(pos, ExprKind::Unsafe(Box::new(body))))
            }
            "quote" => match items {
                [_, q] => Ok(self.mk(pos, ExprKind::Quote(q.clone()))),
                _ => Err(TypeError::resolve(pos, "malformed quote")),
            },
            "." => self.field(items, form),
            "&" => Err(TypeError::new(
                ErrorKind::AmpArgument,
                pos,
                "& argument must be a cell variable",
            )),
            _ if crate::expand::is_core(head) => Err(TypeError::resolve(
                pos,
                format!("{head} is only allowed at top level"),
            )),
            _ => self.call(items, form),
        }
    }

    fn body_or_unit(&mut self, forms: &[Form], form: &Form, tail: bool) -> TResult<Expr> {
        if forms.is_empty() {
            return Ok(self.mk(&form.pos, ExprKind::Lit(Lit::Unit)));
        }
        let e = self.body(forms, &form.pos, tail)?;
        // `(do e)` keeps its own node so the step structure is visible.
        if forms.len() == 1 {
            return Ok(self.mk(&form.pos, ExprKind::Do(vec![e])));
        }
        Ok(e)
    }

    fn if_form(&mut self, items: &[Form], form: &Form, tail: bool) -> TResult<Expr> {
        let [_, c, t, e] = items else {
            return Err(TypeError::resolve(
                &form.pos,
                "if takes a test and two branches",
            ));
        };
        let c = self.expr(c, false)?;
        let t = self.expr(t, tail)?;
        let e = self.expr(e, tail)?;
        Ok(self.mk(
            &form.pos,
            ExprKind::If(Box::new(c), Box::new(t), Box::new(e)),
        ))
    }

    fn let_form(&mut self, items: &[Form], form: &Form, tail: bool) -> TResult<Expr> {
        let pairs = bindings_of(items, form)?;
        let mark = self.mark();
        let mut names: Vec<String> = Vec::new();
        let mut out = Vec::new();
        for (pat, init) in pairs {
            let init = self.expr(init, false)?;
            let pat = let_pattern(self, pat, &mut names)?;
            out.push((pat, init));
        }
        let body = self.body(&items[2..], &form.pos, tail);
        self.reset(mark);
        let body = body?;
        Ok(self.mk(&form.pos, ExprKind::Let(out, Box::new(body))))
    }

    fn loop_form(&mut self, items: &[Form], form: &Form, _tail: bool) -> TResult<Expr> {
        let pairs = bindings_of(items, form)?;
        let mark = self.mark();
        let mut vars = Vec::new();
        for (name, init) in pairs {
            let Some(n) = name.as_sym() else {
                return Err(TypeError::resolve(&name.pos, "a loop variable is a symbol"));
            };
            let init = self.expr(init, false)?;
            let b = self.bind(n, BindingKind::Loop, &name.pos);
            vars.push((b, init));
        }
        self.enter_loop();
        let body = self.body(&items[2..], &form.pos, true);
        self.leave_loop();
        self.reset(mark);
        let body = body?;
        Ok(self.mk(&form.pos, ExprKind::Loop(vars, Box::new(body))))
    }

    fn recur(&mut self, items: &[Form], form: &Form, tail: bool) -> TResult<Expr> {
        if !self.in_loop() {
            let e = TypeError::new(ErrorKind::RecurOutsideLoop, &form.pos, "recur outside loop");
            return Err(e);
        }
        if !tail {
            let msg = "recur not in tail position";
            return Err(TypeError::new(ErrorKind::RecurNotTail, &form.pos, msg));
        }
        let args = items[1..]
            .iter()
            .map(|a| self.expr(a, false))
            .collect::<TResult<Vec<_>>>()?;
        Ok(self.mk(&form.pos, ExprKind::Recur(args)))
    }

    fn match_form(&mut self, items: &[Form], form: &Form, tail: bool) -> TResult<Expr> {
        if items.len() < 3 {
            return Err(TypeError::resolve(
                &form.pos,
                "match needs a scrutinee and clauses",
            ));
        }
        let scrut = self.expr(&items[1], false)?;
        let mut clauses = Vec::new();
        for clause in &items[2..] {
            let parts = clause.as_list().unwrap_or(&[]);
            if parts.len() < 2 {
                return Err(TypeError::resolve(
                    &clause.pos,
                    "a clause is (pattern body+)",
                ));
            }
            let mark = self.mark();
            let pat =
                super::pattern::pattern(self, &parts[0], BindingKind::Pattern, &mut Vec::new());
            let body = pat.and_then(|p| Ok((p, self.body(&parts[1..], &clause.pos, tail)?)));
            self.reset(mark);
            clauses.push(body?);
        }
        Ok(self.mk(&form.pos, ExprKind::Match(Box::new(scrut), clauses)))
    }

    fn fn_form(&mut self, items: &[Form], form: &Form) -> TResult<Expr> {
        let named = items.get(1).and_then(Form::as_sym);
        let start = if named.is_some() { 2 } else { 1 };
        let Some(params) = items.get(start).and_then(Form::as_list) else {
            return Err(TypeError::resolve(&form.pos, "fn needs a parameter list"));
        };
        let (ret, body_start) = self.ret_annotation(items, start + 1)?;
        let mark = self.mark();
        self.enter_frame(false);
        let lit = self.fn_parts(named, params, ret, &items[body_start..], form);
        let captures = self.leave_frame();
        self.reset(mark);
        let mut lit = lit?;
        lit.captures = captures;
        Ok(self.mk(&form.pos, ExprKind::Fn(Box::new(lit))))
    }

    fn fn_parts(
        &mut self,
        named: Option<&str>,
        params: &[Form],
        ret: Option<crate::types::ast::TypeAnn>,
        body: &[Form],
        form: &Form,
    ) -> TResult<FnLit> {
        if body.is_empty() {
            return Err(TypeError::resolve(&form.pos, "fn needs a body"));
        }
        let name = named.map(|n| self.bind(n, BindingKind::FnSelf, &form.pos));
        let mut ps = Vec::new();
        let mut i = 0;
        while i < params.len() {
            let (name, ann, next) = self.plain_param(params, i)?;
            if ps.iter().any(|(b, _)| self.g.binding(*b).name == name) {
                return Err(TypeError::resolve(
                    &params[i].pos,
                    format!("parameter {name} is repeated"),
                ));
            }
            ps.push((self.bind(&name, BindingKind::Param, &params[i].pos), ann));
            i = next;
        }
        let body = self.body(body, &form.pos, false)?;
        Ok(FnLit {
            name,
            params: ps,
            ret,
            body,
            captures: Vec::new(),
        })
    }

    /// A `fn` parameter `sym` or `sym: type` at `i`; returns the name,
    /// the annotation and the index after it.
    pub fn plain_param(
        &mut self,
        params: &[Form],
        i: usize,
    ) -> TResult<(String, Option<crate::types::ast::TypeAnn>, usize)> {
        let p = &params[i];
        if let Some(n) = annotation_name(p) {
            let Some(t) = params.get(i + 1) else {
                return Err(TypeError::resolve(&p.pos, "annotation without a type"));
            };
            let ann = type_ann(self.g, self.m, t, false)?;
            return Ok((n.to_string(), Some(ann), i + 2));
        }
        match p.as_sym() {
            Some(n) if n != "_" => Ok((n.to_string(), None, i + 1)),
            _ => Err(TypeError::resolve(
                &p.pos,
                "a fn parameter is sym or sym: type",
            )),
        }
    }

    /// An optional `-> T` at `i`; returns it and where the body starts.
    pub fn ret_annotation(
        &mut self,
        items: &[Form],
        i: usize,
    ) -> TResult<(Option<crate::types::ast::TypeAnn>, usize)> {
        if items.get(i).and_then(Form::as_sym) != Some("->") {
            return Ok((None, i));
        }
        let Some(t) = items.get(i + 1) else {
            return Err(TypeError::resolve(&items[i].pos, "-> without a type"));
        };
        Ok((Some(type_ann(self.g, self.m, t, false)?), i + 2))
    }

    fn async_form(&mut self, items: &[Form], form: &Form) -> TResult<Expr> {
        self.enter_frame(true);
        let body = self.body_or_unit(&items[1..], form, false);
        let captures = self.leave_frame();
        let body = body?;
        Ok(self.mk(&form.pos, ExprKind::Async(Box::new(body), captures)))
    }

    fn await_form(&mut self, items: &[Form], form: &Form) -> TResult<Expr> {
        if !self.in_async() {
            let e = TypeError::new(
                ErrorKind::AwaitOutsideAsync,
                &form.pos,
                "await outside async",
            );
            return Err(e);
        }
        let [_, e] = items else {
            return Err(TypeError::resolve(&form.pos, "await takes one operand"));
        };
        let e = self.expr(e, false)?;
        Ok(self.mk(&form.pos, ExprKind::Await(Box::new(e))))
    }

    fn field(&mut self, items: &[Form], form: &Form) -> TResult<Expr> {
        let [_, e, f] = items else {
            return Err(TypeError::resolve(
                &form.pos,
                "(. e field) takes two operands",
            ));
        };
        let Some(field) = f.as_sym() else {
            return Err(TypeError::resolve(&f.pos, "a field name is a symbol"));
        };
        let text = e.to_string();
        let e = self.expr(e, false)?;
        Ok(self.mk(
            &form.pos,
            ExprKind::Field(Box::new(e), field.to_string(), text),
        ))
    }
}

/// The `((pat expr)..)` of a `let` or `loop`.
fn bindings_of<'f>(items: &'f [Form], form: &Form) -> TResult<Vec<(&'f Form, &'f Form)>> {
    let head = items[0].as_sym().unwrap_or("let");
    let (Some(pairs), true) = (items.get(1).and_then(Form::as_list), items.len() >= 3) else {
        return Err(TypeError::resolve(
            &form.pos,
            format!("{head} needs bindings and a body"),
        ));
    };
    pairs
        .iter()
        .map(|p| match p.as_list() {
            Some([pat, e]) => Ok((pat, e)),
            _ => Err(TypeError::resolve(
                &p.pos,
                "a binding is (pattern expression)",
            )),
        })
        .collect()
}
