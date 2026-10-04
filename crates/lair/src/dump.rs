//! The canonical text of a parsed lIR module, for comparing readers.
//!
//! `lair dump-ast FILE` prints it. The grammar is written down in
//! docs/design/lair-ast-dump.md; a fibber printer must reproduce it
//! byte for byte, so nothing here depends on Rust's own formatting
//! except `{}` of an integer (`i128` as decimal) and `{:x}` of a `u64`.

use lir::ast::{Binding, Block, Callee, Expr, Function, Item, Kind, Modifiers, Module};
use lir::{FnType, Pos, Type};
use std::fmt::{Debug, Write};

/// The dump of `m`: one line per node, children two spaces deeper.
pub fn dump(m: &Module) -> String {
    let mut w = Writer::default();
    w.line(0, "module", None, "");
    for item in &m.items {
        w.item(1, item);
    }
    w.out
}

#[derive(Default)]
struct Writer {
    out: String,
}

/// `s` as a double-quoted string: `\\`, `\"`, `\n`, `\r`, `\t` and
/// `\xHH` (lower-case hex) for every other byte outside 0x20..0x7e.
pub fn quote(s: &[u8]) -> String {
    let mut o = String::from("\"");
    for &b in s {
        match b {
            b'\\' => o.push_str("\\\\"),
            b'"' => o.push_str("\\\""),
            b'\n' => o.push_str("\\n"),
            b'\r' => o.push_str("\\r"),
            b'\t' => o.push_str("\\t"),
            0x20..=0x7e => o.push(b as char),
            _ => {
                let _ = write!(o, "\\x{b:02x}");
            }
        }
    }
    o.push('"');
    o
}

fn name(s: &str) -> String {
    quote(s.as_bytes())
}

fn ty(t: &Type) -> String {
    quote(t.to_string().as_bytes())
}

fn fnty(t: &FnType) -> String {
    quote(t.to_string().as_bytes())
}

/// An enum's variant name in lower case: `SDiv` is `sdiv`.
fn lower<T: Debug>(x: &T) -> String {
    format!("{x:?}").to_lowercase()
}

fn pos(p: Pos) -> String {
    format!("{}:{}", p.line, p.col)
}

fn opt_align(a: Option<u32>) -> String {
    a.map_or("none".into(), |n| n.to_string())
}

fn mods(m: &Modifiers) -> String {
    format!("linkage={} hidden={}", lower(&m.linkage), m.hidden)
}

fn list(items: impl Iterator<Item = String>) -> String {
    format!("({})", items.collect::<Vec<_>>().join(" "))
}

impl Writer {
    fn line(&mut self, depth: usize, tag: &str, at: Option<Pos>, attrs: &str) {
        for _ in 0..depth {
            self.out.push_str("  ");
        }
        self.out.push_str(tag);
        if let Some(p) = at {
            self.out.push(' ');
            self.out.push_str(&pos(p));
        }
        if !attrs.is_empty() {
            self.out.push(' ');
            self.out.push_str(attrs);
        }
        self.out.push('\n');
    }

    fn item(&mut self, d: usize, item: &Item) {
        match item {
            Item::Struct(s) => {
                let fields = list(s.fields.iter().map(ty));
                self.line(
                    d,
                    "struct",
                    Some(s.pos),
                    &format!("name={} fields={fields}", name(&s.name)),
                );
            }
            Item::Global(g) => {
                let a = format!(
                    "name={} ty={} const={} {}",
                    name(&g.name),
                    ty(&g.ty),
                    g.constant,
                    mods(&g.mods)
                );
                self.line(d, "global", Some(g.pos), &a);
                self.expr(d + 1, &g.init);
            }
            Item::DeclareGlobal(g) => {
                let a = format!(
                    "name={} ty={} hidden={}",
                    name(&g.name),
                    ty(&g.ty),
                    g.hidden
                );
                self.line(d, "declare-global", Some(g.pos), &a);
            }
            Item::Declare(f) => {
                let a = format!(
                    "name={} fn={} hidden={}",
                    name(&f.name),
                    fnty(&f.ty),
                    f.hidden
                );
                self.line(d, "declare", Some(f.pos), &a);
            }
            Item::Define(f) => self.function(d, f),
        }
    }

    fn function(&mut self, d: usize, f: &Function) {
        let params = list(
            f.params
                .iter()
                .map(|(n, p)| format!("{}@{}", name(n), pos(*p))),
        );
        let a = format!(
            "name={} fn={} {} params={params}",
            name(&f.name),
            fnty(&f.ty),
            mods(&f.mods)
        );
        self.line(d, "define", Some(f.pos), &a);
        for b in &f.blocks {
            self.block(d + 1, b);
        }
    }

    fn block(&mut self, d: usize, b: &Block) {
        self.line(
            d,
            "block",
            Some(b.pos),
            &format!("label={}", name(&b.label)),
        );
        for e in &b.body {
            self.expr(d + 1, e);
        }
    }

    fn bind(&mut self, d: usize, b: &Binding) {
        self.line(d, "bind", Some(b.pos), &format!("name={}", name(&b.name)));
        self.expr(d + 1, &b.value);
    }

    fn expr(&mut self, d: usize, e: &Expr) {
        let (tag, attrs) = head(&e.kind);
        self.line(d, tag, Some(e.pos), &attrs);
        self.children(d + 1, &e.kind);
    }

    fn exprs(&mut self, d: usize, es: &[Expr]) {
        for e in es {
            self.expr(d, e);
        }
    }

    fn refs(&mut self, d: usize, es: &[&Expr]) {
        for e in es {
            self.expr(d, e);
        }
    }

    fn children(&mut self, d: usize, k: &Kind) {
        match k {
            Kind::Vector(_, es) | Kind::Struct(_, es) | Kind::Array(_, es) => self.exprs(d, es),
            Kind::Bin(_, a, b)
            | Kind::Overflow(_, a, b)
            | Kind::ICmp(_, a, b)
            | Kind::FCmp(_, a, b)
            | Kind::ExtractElement(a, b)
            | Kind::InsertValue(a, b, _)
            | Kind::AtomicStore(_, _, a, b)
            | Kind::AtomicRmw(_, _, _, a, b)
            | Kind::Store {
                value: a, ptr: b, ..
            } => self.refs(d, &[a, b]),
            Kind::Un(_, a)
            | Kind::Cast(_, _, a)
            | Kind::ExtractValue(a, _)
            | Kind::AtomicLoad(_, _, _, a)
            | Kind::Load { ptr: a, .. }
            | Kind::CondBr(a, _, _) => self.expr(d, a),
            Kind::Select(a, b, c) | Kind::InsertElement(a, b, c) | Kind::Shuffle(a, b, c) => {
                self.refs(d, &[a, b, c])
            }
            _ => self.more_children(d, k),
        }
    }

    fn more_children(&mut self, d: usize, k: &Kind) {
        match k {
            Kind::Alloca { count: Some(c), .. } => self.expr(d, c),
            Kind::Gep { ptr, indices, .. } => {
                self.expr(d, ptr);
                self.exprs(d, indices);
            }
            Kind::CmpXchg {
                ptr, expected, new, ..
            } => self.refs(d, &[ptr, expected, new]),
            Kind::Call { callee, args, .. } => {
                if let Callee::Indirect(c, _) = callee {
                    self.expr(d, c);
                }
                self.exprs(d, args);
            }
            Kind::Ret(Some(v)) => self.expr(d, v),
            Kind::Switch(v, _, cases) => {
                self.expr(d, v);
                for (c, label) in cases {
                    self.line(d, "case", None, &format!("label={}", name(label)));
                    self.expr(d + 1, c);
                }
            }
            Kind::Phi(_, bs) => bs.iter().for_each(|b| self.bind(d, b)),
            Kind::Let(bs, body) => {
                bs.iter().for_each(|b| self.bind(d, b));
                self.exprs(d, body);
            }
            _ => {}
        }
    }
}

/// The tag and the attributes of one expression.
fn head(k: &Kind) -> (&'static str, String) {
    head_value(k)
        .or_else(|| head_op(k))
        .or_else(|| head_memory(k))
        .unwrap_or_else(|| head_control(k))
}

fn head_value(k: &Kind) -> Option<(&'static str, String)> {
    Some(match k {
        Kind::Local(n) => ("local", format!("name={}", name(n))),
        Kind::Global(n) => ("global-ref", format!("name={}", name(n))),
        Kind::Int(t, v) => ("int", format!("ty={} val={v}", ty(t))),
        Kind::Float(t, v) => ("float", format!("ty={} bits=0x{:016x}", ty(t), v.to_bits())),
        Kind::Null => ("null", String::new()),
        Kind::Vector(t, _) => ("vector", format!("ty={}", ty(t))),
        Kind::Str(b) => ("str", format!("bytes={}", quote(b))),
        Kind::Struct(n, _) => (
            "struct-lit",
            format!("name={}", n.as_deref().map_or("none".into(), name)),
        ),
        Kind::Array(t, _) => ("array", format!("ty={}", ty(t))),
        Kind::Zero(t) => ("zero", format!("ty={}", ty(t))),
        _ => return None,
    })
}

fn head_op(k: &Kind) -> Option<(&'static str, String)> {
    Some(match k {
        Kind::Bin(o, ..) => ("bin", format!("op={}", lower(o))),
        Kind::Un(o, _) => ("un", format!("op={}", lower(o))),
        Kind::Overflow(o, ..) => ("overflow", format!("op={}", lower(o))),
        Kind::ICmp(p, ..) => ("icmp", format!("pred={}", lower(p))),
        Kind::FCmp(p, ..) => ("fcmp", format!("pred={}", lower(p))),
        Kind::Cast(o, t, _) => ("cast", format!("op={} ty={}", lower(o), ty(t))),
        Kind::Select(..) => ("select", String::new()),
        Kind::ExtractElement(..) => ("extract-element", String::new()),
        Kind::InsertElement(..) => ("insert-element", String::new()),
        Kind::Shuffle(..) => ("shuffle", String::new()),
        Kind::ExtractValue(_, ix) => (
            "extract-value",
            format!("idx={}", list(ix.iter().map(|i| i.to_string()))),
        ),
        Kind::InsertValue(_, _, ix) => (
            "insert-value",
            format!("idx={}", list(ix.iter().map(|i| i.to_string()))),
        ),
        _ => return None,
    })
}

fn head_memory(k: &Kind) -> Option<(&'static str, String)> {
    Some(match k {
        Kind::Alloca {
            ty: t,
            count,
            align,
        } => (
            "alloca",
            format!(
                "ty={} count={} align={}",
                ty(t),
                count.is_some(),
                opt_align(*align)
            ),
        ),
        Kind::Load {
            ty: t,
            volatile,
            align,
            ..
        } => (
            "load",
            format!(
                "ty={} volatile={volatile} align={}",
                ty(t),
                opt_align(*align)
            ),
        ),
        Kind::Store {
            volatile, align, ..
        } => (
            "store",
            format!("volatile={volatile} align={}", opt_align(*align)),
        ),
        Kind::Gep {
            inbounds, ty: t, ..
        } => ("gep", format!("inbounds={inbounds} ty={}", ty(t))),
        Kind::AtomicLoad(s, o, t, _) => (
            "atomic-load",
            format!("scope={} ord={} ty={}", lower(s), lower(o), ty(t)),
        ),
        Kind::AtomicStore(s, o, ..) => (
            "atomic-store",
            format!("scope={} ord={}", lower(s), lower(o)),
        ),
        Kind::AtomicRmw(op, s, o, ..) => (
            "atomic-rmw",
            format!("op={} scope={} ord={}", lower(op), lower(s), lower(o)),
        ),
        Kind::CmpXchg {
            weak,
            scope,
            success,
            failure,
            ..
        } => (
            "cmpxchg",
            format!(
                "weak={weak} scope={} success={} failure={}",
                lower(scope),
                lower(success),
                lower(failure)
            ),
        ),
        Kind::Fence(s, o) => ("fence", format!("scope={} ord={}", lower(s), lower(o))),
        _ => return None,
    })
}

fn head_control(k: &Kind) -> (&'static str, String) {
    match k {
        Kind::Trap => ("trap", String::new()),
        Kind::Call {
            callee: Callee::Direct(n),
            args,
            tail,
        } => (
            "call",
            format!(
                "tail={tail} callee=direct name={} args={}",
                name(n),
                args.len()
            ),
        ),
        Kind::Call {
            callee: Callee::Indirect(_, f),
            args,
            tail,
        } => (
            "call",
            format!(
                "tail={tail} callee=indirect fn={} args={}",
                fnty(f),
                args.len()
            ),
        ),
        Kind::Ret(v) => ("ret", format!("value={}", v.is_some())),
        Kind::Br(l) => ("br", format!("label={}", name(l))),
        Kind::CondBr(_, t, f) => ("cond-br", format!("then={} else={}", name(t), name(f))),
        Kind::Switch(_, d, cases) => (
            "switch",
            format!("default={} cases={}", name(d), cases.len()),
        ),
        Kind::Unreachable => ("unreachable", String::new()),
        Kind::Phi(t, _) => ("phi", format!("ty={}", ty(t))),
        Kind::Let(bs, _) => ("let", format!("bindings={}", bs.len())),
        // head_value, head_op and head_memory take every other kind.
        _ => ("unknown", String::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dumped(src: &str) -> String {
        dump(&lir::parse(src).expect("parses"))
    }

    #[test]
    fn quotes_by_byte() {
        assert_eq!(
            quote(b"a\"b\\c\n\t\r\x00\x7f\xc3\xa9"),
            r#""a\"b\\c\n\t\r\x00\x7f\xc3\xa9""#
        );
    }

    #[test]
    fn dumps_a_small_module_line_by_line() {
        let d = dumped(
            "(define (main i32) () (block entry (let ((x (add (i32 1) (i32 -2)))) (ret x))))",
        );
        let want = "module\n  define 1:1 name=\"main\" fn=\"(fn i32 ())\" linkage=external hidden=false params=()\n    \
block 1:23 label=\"entry\"\n      let 1:36 bindings=1\n        bind 1:42 name=\"x\"\n          \
bin 1:45 op=add\n            int 1:50 ty=\"i32\" val=1\n            int 1:58 ty=\"i32\" val=-2\n        \
ret 1:70 value=true\n          local 1:75 name=\"x\"\n";
        assert_eq!(d, want);
    }

    #[test]
    fn literals_keep_their_exact_value() {
        let d = dumped(
            "(define (f i64) () (block e (let ((a (i64 18446744073709551615)) (b (i64 -9223372036854775808)) (c (double 0.1)) (s (string \"h\\n\\xc3\"))) (ret a))))",
        );
        assert!(d.contains("val=18446744073709551615\n"), "{d}");
        assert!(d.contains("val=-9223372036854775808\n"), "{d}");
        assert!(d.contains("bits=0x3fb999999999999a\n"), "{d}");
        assert!(d.contains("bytes=\"h\\n\\xc3\"\n"), "{d}");
    }

    #[test]
    fn dump_distinguishes_modules() {
        let a = dumped("(define (f void) () (block e (ret)))");
        let b = dumped("(define (f void) () (block e (unreachable)))");
        assert_ne!(a, b);
    }
}
