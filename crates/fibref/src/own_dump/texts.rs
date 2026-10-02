//! The words of the ownership dump (spec/bootstrap.md §7.3): how a site,
//! a mode, an operation, a pass, a callee and the other values of
//! [`crate::own::program`] are written on a line. Ids are numbers (a
//! binding, an expression, a function, a definition); a name is written
//! only where the id alone would not tell a reader what it is.

use crate::own::program::{
    Alloc, Because, BindKind, BodyKey, Callee, Mode, Op, OpKind, OwnedWhy, ParamKind, ParamOwn,
    Pass, Site, Tail, Why,
};
use crate::own::OwnErrorKind;
use crate::types::builtins::BUILTINS;
use crate::types::decls::{Globals, ModuleId};
use crate::types_dump::impl_method_name;

/// `b:B`, `v:E`, `cap:E:B`, `env:E` or `def:D`.
pub(super) fn site(s: Site) -> String {
    match s {
        Site::Bind(b) => format!("b:{}", b.0),
        Site::Value(e) => format!("v:{}", e.0),
        Site::Capture(e, b) => format!("cap:{}:{}", e.0, b.0),
        Site::Env(e) => format!("env:{}", e.0),
        Site::Global(d) => format!("def:{}", d.0),
    }
}

/// `scalar`, `owned`, `owned!` (immortal), `borrowed SITE`, `derived SITE`.
pub(super) fn mode(m: Mode) -> String {
    match m {
        Mode::Scalar => "scalar".to_string(),
        Mode::Owned { immortal: false } => "owned".to_string(),
        Mode::Owned { immortal: true } => "owned!".to_string(),
        Mode::Borrowed(s) => format!("borrowed {}", site(s)),
        Mode::Derived(s) => format!("derived {}", site(s)),
    }
}

/// `KIND SITE WHY`: `retain`, `release` or `end-stack`, then the site and
/// the reason.
pub(super) fn op(o: &Op) -> String {
    let kind = match o.kind {
        OpKind::Retain => "retain",
        OpKind::Release => "release",
        OpKind::EndStack => "end-stack",
    };
    let why = match o.why {
        Why::Return => "return",
        Why::Store => "store",
        Why::Join => "join",
        Why::ScopeExit => "scope-exit",
        Why::StepEnd => "step-end",
        Why::DerivedExit => "derived-exit",
        Why::Discard => "discard",
        Why::ParamExit => "param-exit",
        Why::LoopInit => "loop-init",
        Why::Jump => "jump",
        Why::RecurOld => "recur-old",
    };
    format!("{kind} {} {why}", site(o.site))
}

/// The operations in order, separated by `; `.
pub(super) fn ops(list: &[Op]) -> String {
    let words: Vec<String> = list.iter().map(op).collect();
    words.join("; ")
}

pub(super) fn pass(p: Pass) -> &'static str {
    match p {
        Pass::Scalar => "scalar",
        Pass::Borrow => "borrow",
        Pass::Move => "move",
        Pass::Retain => "retain",
        Pass::Acquire => "acquire",
        Pass::Forward => "forward",
        Pass::OwnCell => "own-cell",
        Pass::Alias => "alias",
        Pass::KeepEnv => "keep-env",
    }
}

/// The passes in order, separated by commas.
pub(super) fn passes(list: &[Pass]) -> String {
    let words: Vec<&str> = list.iter().map(|p| pass(*p)).collect();
    words.join(",")
}

/// `none` (not in tail position), `tail`, or `ordinary:BECAUSE`.
pub(super) fn tail(t: Tail) -> String {
    match t {
        Tail::NotInTail => "none".to_string(),
        Tail::TailCall => "tail".to_string(),
        Tail::Ordinary(b) => format!("ordinary:{}", because(b)),
    }
}

fn because(b: Because) -> String {
    match b {
        Because::AmpArgument => "amp-argument".to_string(),
        Because::AmpCaptured { param } => format!("amp-captured:{}", param.0),
        Because::FrameOwned { arg } => format!("frame-owned:{arg}"),
        Because::AsyncBody => "async-body".to_string(),
        Because::Extern => "extern".to_string(),
        Because::Store => "store".to_string(),
    }
}

/// `fun NAME`, `allowned NAME`, `method PROTOCOL.NAME`, `builtin NAME`,
/// `ctor`, `extern` or `value`; a builtin by its name, not its index.
pub(super) fn callee(g: &Globals, c: Callee) -> String {
    match c {
        Callee::Fun(f) => format!("fun {}", g.fun(f).name),
        Callee::AllOwned(f) => format!("allowned {}", g.fun(f).name),
        Callee::Method(p, i) => {
            let proto = g.proto(p);
            format!("method {}.{}", proto.name, proto.methods[i].name)
        }
        Callee::Builtin(b) => format!("builtin {}", BUILTINS[b.0 as usize].name),
        Callee::Ctor => "ctor".to_string(),
        Callee::Extern => "extern".to_string(),
        Callee::Value => "value".to_string(),
    }
}

/// `rule1(TEXT)`, `rule2(TEXT)`, `rule3(TEXT)`, `loop(TEXT)`, `declared`.
fn why(w: &OwnedWhy) -> String {
    match w {
        OwnedWhy::Rule1(t) => format!("rule1({t})"),
        OwnedWhy::Rule2(t) => format!("rule2({t})"),
        OwnedWhy::Rule3(t) => format!("rule3({t})"),
        OwnedWhy::Loop(t) => format!("loop({t})"),
        OwnedWhy::Declared => "declared".to_string(),
    }
}

/// The reasons a parameter is owned, in order, separated by commas.
pub(super) fn whys(list: impl IntoIterator<Item = impl std::borrow::Borrow<OwnedWhy>>) -> String {
    let words: Vec<String> = list.into_iter().map(|w| why(w.borrow())).collect();
    words.join(",")
}

/// `scalar`, `amp`, `borrowed`, `owned` or `owned:WHY,WHY..`.
fn param_kind(k: &ParamKind) -> String {
    match k {
        ParamKind::Scalar => "scalar".to_string(),
        ParamKind::Amp => "amp".to_string(),
        ParamKind::Borrowed => "borrowed".to_string(),
        ParamKind::Owned(ws) if ws.is_empty() => "owned".to_string(),
        ParamKind::Owned(ws) => format!("owned:{}", whys(ws)),
    }
}

/// `B NAME KIND escapes=0|1 declared-borrow=0|1`: a parameter of a body
/// or a closure.
pub(super) fn param(g: &Globals, p: &ParamOwn) -> String {
    format!(
        "B{} {} {} escapes={} declared-borrow={}",
        p.binding.0,
        g.binding(p.binding).name,
        param_kind(&p.kind),
        u8::from(p.escapes),
        u8::from(p.declared_borrow)
    )
}

/// `scalar`, `owns`, `alias-of SITE`, `derived-of SITE`, `borrowed-param`,
/// `owned-param` or `amp-param`.
pub(super) fn bind_kind(k: BindKind) -> String {
    match k {
        BindKind::Scalar => "scalar".to_string(),
        BindKind::Owns => "owns".to_string(),
        BindKind::AliasOf(s) => format!("alias-of {}", site(s)),
        BindKind::DerivedOf(s) => format!("derived-of {}", site(s)),
        BindKind::BorrowedParam => "borrowed-param".to_string(),
        BindKind::OwnedParam => "owned-param".to_string(),
        BindKind::AmpParam => "amp-param".to_string(),
    }
}

pub(super) fn alloc(a: Alloc) -> &'static str {
    match a {
        Alloc::Heap => "heap",
        Alloc::Stack => "stack",
        Alloc::Nothing => "nothing",
    }
}

/// `no`, or `yes(REASON)`.
pub(super) fn reason(r: &Option<String>) -> String {
    match r {
        None => "no".to_string(),
        Some(why) => format!("yes({why})"),
    }
}

/// `fun NAME`, `allowned NAME`, `method I NAME`, `methodowned I NAME` (`I`
/// the instance, as in the `unit impl` lines of §6.3) or `def NAME`.
pub(super) fn body_key(g: &Globals, key: BodyKey) -> String {
    match key {
        BodyKey::Fun(f) => format!("fun {}", g.fun(f).name),
        BodyKey::AllOwned(f) => format!("allowned {}", g.fun(f).name),
        BodyKey::Method(i, m) => format!("method {i} {}", impl_method_name(g, i, m)),
        BodyKey::MethodOwned(i, m) => format!("methodowned {i} {}", impl_method_name(g, i, m)),
        BodyKey::Def(d) => format!("def {}", g.def(d).name),
    }
}

/// The module that defines the body `key`.
pub(super) fn key_module(g: &Globals, key: BodyKey) -> ModuleId {
    match key {
        BodyKey::Fun(f) | BodyKey::AllOwned(f) => g.fun(f).module,
        BodyKey::Method(i, _) | BodyKey::MethodOwned(i, _) => g.instances[i].module,
        BodyKey::Def(d) => g.def(d).module,
    }
}

/// The name of an own error's variant; a new variant is a compile error
/// here until it is named.
pub fn kind_name(k: OwnErrorKind) -> &'static str {
    match k {
        OwnErrorKind::AmpTwice => "AmpTwice",
        OwnErrorKind::AmpInAsync => "AmpInAsync",
        OwnErrorKind::AmpCaptured => "AmpCaptured",
        OwnErrorKind::BorrowEscapes => "BorrowEscapes",
        OwnErrorKind::ImplEscapes => "ImplEscapes",
    }
}
