//! The text of the nodes of the AST and of the annotations, colours and
//! literals in them (spec/bootstrap.md §6.3): one function for the line of
//! each kind of node, used by the `ast` section and the tables.

use crate::dump::{quote, span_in};
use crate::eval::arith::float_text;
use crate::syntax::FltWidth;
use crate::types::ast::{
    BindingId, ConvOp, Expr, ExprKind, GlobalRef, IntConv, Lit, PatKind, Pattern, Rest, TypeAnn,
};
use crate::types::builtins::BUILTINS;
use crate::types::decls::{Globals, Shape};
use crate::types::ty::{Colour, Con, Scalar};

/// A colour as the tables print it: `send`, `local`, `ς3` (quantified),
/// `κ0` (rigid), `?ς7` (an unsolved variable, renumbered by the dump).
pub(super) fn colour_text(k: Colour) -> String {
    match k {
        Colour::Send => "send".into(),
        Colour::Local => "local".into(),
        Colour::Gen(i) => format!("ς{i}"),
        Colour::Var(v) => format!("?ς{}", v.0),
        Colour::Rigid(i) => format!("κ{i}"),
    }
}

/// A written type annotation as text: names as written, `Self`, the
/// scalars, `str`, `(Array T)`, `(Name A..)`, `(fn [COLOUR] (A..) R)`,
/// `(dyn P A.. [:send])`.
pub(super) fn ann_text(g: &Globals, a: &TypeAnn) -> String {
    let list =
        |xs: &[TypeAnn]| -> String { xs.iter().map(|x| format!(" {}", ann_text(g, x))).collect() };
    match a {
        TypeAnn::Var(n) => n.clone(),
        TypeAnn::SelfTy => "Self".into(),
        TypeAnn::Scalar(s) => s.name().into(),
        TypeAnn::Str => "str".into(),
        TypeAnn::Builtin(con, inner) => format!("({} {})", con_name(*con), ann_text(g, inner)),
        TypeAnn::Nominal(id, args) if args.is_empty() => g.ty(*id).name.clone(),
        TypeAnn::Nominal(id, args) => format!("({}{})", g.ty(*id).name, list(args)),
        TypeAnn::Fn(colour, ps, r) => {
            let k = colour
                .as_ref()
                .map_or(String::new(), |c| format!("{} ", colour_ann(c)));
            let ps: Vec<String> = ps.iter().map(|p| ann_text(g, p)).collect();
            format!("(fn {k}({}) {})", ps.join(" "), ann_text(g, r))
        }
        TypeAnn::ColourArg(c) => colour_ann(c),
        TypeAnn::Dyn(p, args, send) => {
            let send = if *send { " :send" } else { "" };
            format!("(dyn {}{}{send})", g.proto(*p).name, list(args))
        }
    }
}

pub(super) fn colour_ann(c: &crate::types::ast::ColourAnn) -> String {
    use crate::types::ast::ColourAnn;
    match c {
        ColourAnn::Fixed(Colour::Send) => ":send".into(),
        ColourAnn::Fixed(Colour::Local) => ":local".into(),
        ColourAnn::Fixed(k) => colour_text(*k),
        ColourAnn::Named(n) => n.clone(),
    }
}

pub(super) fn con_name(c: Con) -> &'static str {
    match c {
        Con::Scalar(s) => s.name(),
        Con::Str => "str",
        Con::Array => "Array",
        Con::Cell => "Cell",
        Con::Atom => "Atom",
        Con::Weak => "Weak",
        Con::Task => "Task",
        Con::Nominal(_) => "nominal",
        Con::Dyn(..) => "dyn",
    }
}

pub(super) fn variant_name(g: &Globals, t: crate::types::ty::TypeId, v: Option<usize>) -> String {
    let Some(i) = v else { return "-".into() };
    match &g.ty(t).shape {
        Shape::Enum(vs) => vs.get(i).map_or(i.to_string(), |x| x.name.clone()),
        Shape::Struct(_) => i.to_string(),
    }
}

pub(super) fn global_text(g: &Globals, r: GlobalRef) -> String {
    match r {
        GlobalRef::Fun(f) => format!("fun {}", g.fun(f).name),
        GlobalRef::Ctor(t, v) => format!("ctor {} {}", g.ty(t).name, variant_name(g, t, v)),
        GlobalRef::Method(p, i) => {
            format!("method {} {}", g.proto(p).name, g.proto(p).methods[i].name)
        }
        GlobalRef::Builtin(b) => format!("builtin {}", BUILTINS[b.0 as usize].name),
        GlobalRef::Def(d) => format!("def {}", g.def(d).name),
        GlobalRef::Extern(x) => format!("extern {}", g.ext(x).name),
    }
}

pub(super) fn lit_text(l: &Lit) -> String {
    match l {
        Lit::Int(v, w) => format!("int {v} {}", w.suffix()),
        Lit::Float(v, w) => {
            let scalar = if *w == FltWidth::F32 {
                Scalar::F32
            } else {
                Scalar::F64
            };
            format!("flt {} {}", float_text(*v, scalar), w.suffix())
        }
        Lit::Str(s) => format!("str {}", quote(s)),
        Lit::Char(c) => format!("chr U+{:04X}", u32::from(*c)),
        Lit::Bool(b) => format!("bool {b}"),
        Lit::Keyword(k) => format!("kw {}", quote(k)),
        Lit::Unit => "unit".into(),
    }
}

pub(super) fn conv_text(op: ConvOp) -> String {
    let sign = |signed: bool| if signed { "signed" } else { "unsigned" };
    match op {
        ConvOp::IntToInt(IntConv::Trunc) => "int-to-int:trunc".into(),
        ConvOp::IntToInt(IntConv::Zext) => "int-to-int:zext".into(),
        ConvOp::IntToInt(IntConv::Sext) => "int-to-int:sext".into(),
        ConvOp::FloatToFloat => "float-to-float".into(),
        ConvOp::FloatToInt { signed } => format!("float-to-int:{}", sign(signed)),
        ConvOp::IntToFloat { signed } => format!("int-to-float:{}", sign(signed)),
    }
}

pub(super) fn captures(bs: &[BindingId]) -> String {
    let ids: Vec<String> = bs.iter().map(|b| format!("B{}", b.0)).collect();
    format!("captures[{}]", ids.join(","))
}

/// `KIND DETAIL` of an expression node.
pub(super) fn expr_kind(g: &Globals, e: &Expr) -> String {
    let name = |b: &BindingId| g.binding(*b).name.clone();
    match &e.kind {
        ExprKind::Lit(l) => format!("lit {}", lit_text(l)),
        ExprKind::Local(b) => format!("local B{} {}", b.0, name(b)),
        ExprKind::Global(r) => format!("global {}", global_text(g, *r)),
        ExprKind::Call(_, args) => format!("call {}", args.len()),
        ExprKind::Fn(lit) => format!("fn {} {}", lit.params.len(), captures(&lit.captures)),
        ExprKind::Let(bs, _) => format!("let {}", bs.len()),
        ExprKind::If(..) => "if".into(),
        ExprKind::Do(es) => format!("do {}", es.len()),
        ExprKind::Match(_, cs) => format!("match {}", cs.len()),
        ExprKind::Loop(vs, _) => format!("loop {}", vs.len()),
        ExprKind::Recur(es) => format!("recur {}", es.len()),
        ExprKind::Field(_, f, text) => format!("field {f} {}", quote(text)),
        ExprKind::Deref(_, text) => format!("deref {}", quote(text)),
        ExprKind::Set(..) => "set".into(),
        ExprKind::SetField(b, f, _) => format!("set-field B{} {} {f}", b.0, name(b)),
        ExprKind::Async(_, caps) => format!("async {}", captures(caps)),
        ExprKind::Await(_) => "await".into(),
        ExprKind::Unsafe(_) => "unsafe".into(),
        ExprKind::Quote(_) => "quote".into(),
        ExprKind::Dyn(p, args, send, _) => {
            let args: Vec<String> = args
                .iter()
                .map(|a| format!(" {}", ann_text(g, a)))
                .collect();
            let send = if *send { " :send" } else { "" };
            format!("dyn {}{send}{}", g.proto(*p).name, args.concat())
        }
        ExprKind::Convert(op, to, _) => format!("convert {} {}", conv_text(*op), to.name()),
        ExprKind::Concat(es) => format!("concat {}", es.len()),
        ExprKind::Guarded(_) | ExprKind::And(_) | ExprKind::Or(_) | ExprKind::Elided => {
            "unelaborated".into()
        }
    }
}

pub(super) fn binding_head(g: &Globals, b: BindingId) -> String {
    let info = g.binding(b);
    let ann = info
        .ann
        .as_ref()
        .map_or(String::new(), |a| format!(" : {}", ann_text(g, a)));
    format!("B{} {}{ann}", b.0, info.name)
}

pub(super) fn binding_kind(k: crate::types::ast::BindingKind) -> &'static str {
    use crate::types::ast::BindingKind as K;
    match k {
        K::Param => "param",
        K::AmpParam => "ampparam",
        K::Let => "let",
        K::Pattern => "pattern",
        K::Loop => "loop",
        K::FnSelf => "fnself",
    }
}

pub(super) fn binding_line(g: &Globals, b: BindingId, home: &str) -> String {
    let info = g.binding(b);
    let ann = info
        .ann
        .as_ref()
        .map_or(String::new(), |a| format!(" : {}", ann_text(g, a)));
    let at = span_in(&info.pos, home);
    format!(
        "B{} {} {}{ann} {at}",
        b.0,
        binding_kind(info.kind),
        info.name
    )
}

pub(super) fn pat_line(g: &Globals, p: &Pattern, home: &str) -> String {
    let at = span_in(&p.pos, home);
    match &p.kind {
        PatKind::Wild => format!("P wild {at}"),
        PatKind::Bind(b) => format!("P bind {} {at}", binding_head(g, *b)),
        PatKind::Lit(l) => format!("P lit {} {at}", lit_text(l)),
        PatKind::Ctor(t, v, subs) => {
            let (name, variant) = (&g.ty(*t).name, variant_name(g, *t, *v));
            format!("P ctor {name} {variant} {} {at}", subs.len())
        }
        PatKind::As(_, b) => format!("P as {} {at}", binding_head(g, *b)),
        PatKind::Vec(subs, rest) => {
            let rest = match rest {
                Rest::Exact => "exact".to_string(),
                Rest::Ignore => "ignore".to_string(),
                Rest::Bind(b) => format!("B{}", b.0),
            };
            format!("P vec {} {rest} {at}", subs.len())
        }
    }
}
