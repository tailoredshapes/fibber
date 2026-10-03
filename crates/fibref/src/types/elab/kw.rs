//! Keywords as functions (stdlib §7 L14), after inference: `(:k x)` is
//! the field read `(. x k)` when `x` is a struct and `(map-get x :k)` when
//! it is a `(Map keyword v)`; `(:k x d)` is a `match` on that, `d` for
//! `nil`; a keyword where a function was expected is the `fn` of the
//! read. The checker decided which (`Tables::kw_sites`).

use super::*;
use crate::types::ast::FnLit;
use crate::types::infer::KwSite;

/// What a keyword read is made of: the operand, the keyword, the
/// default, and where it is.
struct Read {
    x: Expr,
    k: String,
    dflt: Option<Expr>,
    pos: Pos,
}

impl Elab<'_> {
    /// A call whose head is a keyword the checker took as a function:
    /// elaborated whole, its head never a node of its own. `false` when
    /// `e` is not one.
    pub(super) fn keyword_call(&mut self, e: &mut Expr) -> bool {
        let ExprKind::Call(head, _) = &e.kind else {
            return false;
        };
        let ExprKind::Lit(Lit::Keyword(k)) = &head.kind else {
            return false;
        };
        let (k, site) = (k.clone(), head.id);
        let Some(&kind) = self.t.kw_sites.get(&site) else {
            return false;
        };
        let ExprKind::Call(_, args) = std::mem::replace(&mut e.kind, ExprKind::Lit(Lit::Unit))
        else {
            return false;
        };
        let mut es: Vec<Expr> = args
            .into_iter()
            .filter_map(|a| match a {
                Arg::Expr(x) => Some(x),
                Arg::Amp(..) => None,
            })
            .collect();
        es.iter_mut().for_each(|x| self.expr(x));
        let dflt = (es.len() == 2).then(|| es.remove(1));
        let x = es.remove(0);
        let read = Read {
            x,
            k,
            dflt,
            pos: e.pos.clone(),
        };
        let ty = self.ty(e);
        e.kind = self.read(kind, read, site, &ty);
        true
    }

    /// A keyword literal that is a function: `(fn (x) <the read of x>)`.
    pub(super) fn keyword_fn(&mut self, e: &Expr, k: &str, kind: KwSite) -> ExprKind {
        let ty = self.ty(e);
        let (arg, ret) = match &ty {
            Ty::Fn(_, ps, r) if ps.len() == 1 => (ps[0].clone(), (**r).clone()),
            _ => (Ty::unit(), Ty::unit()),
        };
        let b = BindingId(self.g.bindings.len() as u32);
        self.g.bindings.push(BindingInfo {
            name: "keyword-arg".to_string(),
            kind: BindingKind::Param,
            pos: e.pos.clone(),
            ann: None,
        });
        self.t.binding_types.insert(b, arg.clone());
        let x = self.node(&e.pos, ExprKind::Local(b), &arg);
        let read = Read {
            x,
            k: k.to_string(),
            dflt: None,
            pos: e.pos.clone(),
        };
        let kind = self.read(kind, read, e.id, &ret);
        let body = self.node(&e.pos, kind, &ret);
        ExprKind::Fn(Box::new(FnLit {
            name: None,
            params: vec![(b, None)],
            ret: None,
            body,
            captures: Vec::new(),
        }))
    }

    /// The read itself, of the type `ty`; `site` is the keyword's node,
    /// whose instantiation of `map-get` moves to the head made here.
    fn read(&mut self, kind: KwSite, r: Read, site: ExprId, ty: &Ty) -> ExprKind {
        let Read { x, k, dflt, pos } = r;
        if kind == KwSite::Field {
            return ExprKind::Field(Box::new(x), k, "the argument".to_string());
        }
        let opt = match dflt {
            Some(_) => Ty::nominal(self.g.option, vec![ty.clone()]),
            None => ty.clone(),
        };
        let kw_ty = Ty::scalar(Scalar::Keyword);
        let fty = Ty::Fn(
            Colour::Send,
            vec![self.ty(&x), kw_ty.clone()],
            Box::new(opt.clone()),
        );
        let f = self.g.value(ModuleId::PRELUDE, "map-get");
        let kind = f.map_or(ExprKind::Lit(Lit::Unit), ExprKind::Global);
        let head = self.node(&pos, kind, &fty);
        if let Some(i) = self.t.instantiations.remove(&site) {
            self.t.instantiations.insert(head.id, i);
        }
        let kw = self.node(&pos, ExprKind::Lit(Lit::Keyword(k)), &kw_ty);
        let call = ExprKind::Call(Box::new(head), vec![Arg::Expr(x), Arg::Expr(kw)]);
        let Some(d) = dflt else {
            return call;
        };
        let call = self.node(&pos, call, &opt);
        let b = self.binding(ty, &pos);
        let some = Pattern {
            pos: pos.clone(),
            kind: PatKind::Ctor(
                self.g.option,
                Some(1),
                vec![Pattern {
                    pos: pos.clone(),
                    kind: PatKind::Bind(b),
                }],
            ),
        };
        let local = self.node(&pos, ExprKind::Local(b), ty);
        let clauses = vec![clause(some, local), clause(Self::wild(&pos), d)];
        ExprKind::Match(Box::new(call), clauses)
    }
}
