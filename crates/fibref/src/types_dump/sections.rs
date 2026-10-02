//! The sections of one module (spec/bootstrap.md §6.3): `type`,
//! `protocol`, `instance`, `fun`, `def`, `extern` and `unit`, each over
//! the `Vec` tables of the global tables in index order, filtered to the
//! module's own definitions; `ast` and `tables` are in [`super::tables`].

use crate::dump::span_in;
use crate::types::ast::{ExternId, FunId, GlobalRef};
use crate::types::decls::{FunDef, Globals, MethodDef, ModuleId, ParamDecl, PredAnn, Shape};
use crate::types::display::Printer;
use crate::types::infer::UnitRef;
use crate::types::program::show_scheme_in;
use crate::types::ty::{Pred, Ty};

use super::tables;
use super::texts::ann_text;
use super::{push, Options, Section, Shown, View};

/// Appends the sections `opts` asks for of the module `s`.
pub(super) fn module(v: &View, s: &Shown, opts: &Options, out: &mut String) {
    if opts.wants(Section::Type) {
        types(v.g, s, out);
    }
    if opts.wants(Section::Protocol) {
        protocols(v.g, s, out);
    }
    if opts.wants(Section::Instance) {
        instances(v.g, s, out);
    }
    if opts.wants(Section::Fun) {
        funs(v, s, out);
    }
    if opts.wants(Section::Def) {
        defs(v, s, out);
    }
    if opts.wants(Section::Extern) {
        externs(v.g, s, out);
    }
    let tables = opts.wants(Section::Tables);
    if v.typed.is_some() && (opts.wants(Section::Unit) || tables) {
        units(v, s, tables, out);
    }
    if opts.wants(Section::Ast) {
        tables::ast(v, s, out);
    }
}

fn types(g: &Globals, s: &Shown, out: &mut String) {
    for def in g.types.iter().filter(|d| d.module == s.id) {
        let params: Vec<String> = (0..def.params.len())
            .map(|i| match def.is_colour(i) {
                true => format!("{}:colour", def.params[i]),
                false => def.params[i].clone(),
            })
            .collect();
        let shape = match def.shape {
            Shape::Struct(_) => "struct",
            Shape::Enum(_) => "enum",
        };
        let at = span_in(&def.pos, &s.file);
        push(
            out,
            &format!("type {} ({}) {shape} {at}", def.name, params.join(" ")),
        );
        let p = Printer::with_names(g, &def.params, &[]);
        match &def.shape {
            Shape::Struct(fields) => {
                for f in fields {
                    push(out, &format!("  field {} : {}", f.name, p.ty(&f.ty)));
                }
            }
            Shape::Enum(variants) => {
                for v in variants {
                    let tys: Vec<String> = v.fields.iter().map(|f| p.ty(&f.ty)).collect();
                    push(out, &format!("  variant {} ({})", v.name, tys.join(" ")));
                }
            }
        }
    }
}

/// `  method M : SCHEME params P[:borrow|:owned]..` (the protocol's own
/// declaration of the method).
fn method_line(g: &Globals, m: &MethodDef) -> String {
    let params: Vec<String> = m
        .params
        .iter()
        .map(|p| {
            let borrow = if p.borrow { ":borrow" } else { "" };
            let owned = if p.owned { ":owned" } else { "" };
            format!(" {}{borrow}{owned}", p.name)
        })
        .collect();
    format!(
        "  method {} : {} params{}",
        m.name,
        show_scheme_in(g, &m.scheme),
        params.concat()
    )
}

fn protocols(g: &Globals, s: &Shown, out: &mut String) {
    for def in g.protos.iter().filter(|d| d.module == s.id) {
        let p = Printer::with_names(g, &def.params, &[]);
        let supers: Vec<String> = def
            .supers
            .iter()
            .map(|q| format!(" {}", p.pred(q)))
            .collect();
        let at = span_in(&def.pos, &s.file);
        let head = format!("protocol {} ({}) {at}", def.name, def.params.join(" "));
        push(out, &format!("{head} supers{}", supers.concat()));
        for m in &def.methods {
            push(out, &method_line(g, m));
        }
    }
}

fn instances(g: &Globals, s: &Shown, out: &mut String) {
    for (n, inst) in g
        .instances
        .iter()
        .enumerate()
        .filter(|(_, i)| i.module == s.id)
    {
        let p = Printer::with_names(g, &inst.var_names, &[]);
        let mut args = vec![inst.head.clone()];
        args.extend(inst.dets.iter().cloned());
        let head = p.pred(&Pred::Proto(inst.proto, args));
        let context: Vec<String> = inst.context.iter().map(|q| p.pred(q)).collect();
        let ctx = match context.is_empty() {
            true => String::new(),
            false => format!(" where {}", context.join(" ")),
        };
        let at = span_in(&inst.pos, &s.file);
        push(
            out,
            &format!(
                "instance {n} {head} vars {}{ctx} {at}",
                inst.var_names.len()
            ),
        );
        for m in &inst.methods {
            push(out, &method_line(g, &g.proto(inst.proto).methods[m.index]));
        }
    }
}

/// `(fn (A..) R) where B..` of a `defun` as it was written: an annotation
/// that is not there prints `_`, an `&` parameter `(& A)`.
fn declared(g: &Globals, f: &FunDef) -> String {
    let ann = |a: &Option<crate::types::ast::TypeAnn>| {
        a.as_ref().map_or("_".to_string(), |a| ann_text(g, a))
    };
    let params: Vec<String> = f
        .params
        .iter()
        .map(|p| match p.amp {
            true => format!("(& {})", ann(&p.ann)),
            false => ann(&p.ann),
        })
        .collect();
    let bounds: Vec<String> = f
        .bounds
        .iter()
        .map(|b| match b {
            PredAnn::Proto(id, args) => {
                let args: Vec<String> = args
                    .iter()
                    .map(|a| format!(" {}", ann_text(g, a)))
                    .collect();
                format!("({}{})", g.proto(*id).name, args.concat())
            }
            PredAnn::Send(a) => format!("(Send {})", ann_text(g, a)),
        })
        .collect();
    let mut text = format!("(fn ({}) {})", params.join(" "), ann(&f.ret));
    if !bounds.is_empty() {
        text.push_str(&format!(" where {}", bounds.join(" ")));
    }
    text
}

fn param_words(params: &[ParamDecl]) -> String {
    let words: Vec<String> = params
        .iter()
        .map(|p| {
            let amp = if p.amp { "&" } else { "" };
            let borrow = if p.borrow { ":borrow" } else { "" };
            format!(" {amp}{}{borrow}", p.name)
        })
        .collect();
    words.concat()
}

fn funs(v: &View, s: &Shown, out: &mut String) {
    for (i, f) in
        v.g.funs
            .iter()
            .enumerate()
            .filter(|(_, f)| f.module == s.id)
    {
        let scheme = match v.typed {
            None => declared(v.g, f),
            Some(t) => t.fun_schemes[i]
                .as_ref()
                .map_or("_".to_string(), |sc| show_scheme_in(v.g, sc)),
        };
        let head = if f.is_macro { "macro" } else { "fun" };
        let at = span_in(&f.pos, &s.file);
        let params = param_words(&f.params);
        push(
            out,
            &format!("{head} {} : {scheme} params{params} {at}", f.name),
        );
    }
}

fn defs(v: &View, s: &Shown, out: &mut String) {
    for (i, d) in
        v.g.defs
            .iter()
            .enumerate()
            .filter(|(_, d)| d.module == s.id)
    {
        let ty = match v.typed {
            None => d.ann.as_ref().map_or("_".to_string(), |a| ann_text(v.g, a)),
            Some(t) => t.def_types[i]
                .as_ref()
                .map_or("_".to_string(), |ty| Printer::new(v.g).ty(ty)),
        };
        let at = span_in(&d.pos, &s.file);
        push(out, &format!("def {} : {ty} {at}", d.name));
    }
}

/// The module that binds the extern `id`: looked up by its name in each
/// module in turn (the tables of names are maps and are not iterated).
fn extern_owner(g: &Globals, id: ExternId) -> Option<ModuleId> {
    let name = &g.ext(id).name;
    (0..g.modules.len() as u32).map(ModuleId).find(|m| {
        let bound = g.names(*m).values.get(name);
        bound == Some(&GlobalRef::Extern(id))
    })
}

fn externs(g: &Globals, s: &Shown, out: &mut String) {
    for (i, x) in g.externs.iter().enumerate() {
        if extern_owner(g, ExternId(i as u32)) != Some(s.id) {
            continue;
        }
        let ty: &Ty = &x.ty;
        let varargs = if x.varargs { " varargs" } else { "" };
        let at = span_in(&x.pos, &s.file);
        push(
            out,
            &format!(
                "extern {} : {}{varargs} {at}",
                x.name,
                Printer::new(g).ty(ty)
            ),
        );
    }
}

/// The module a unit belongs to.
fn unit_module(g: &Globals, u: &UnitRef) -> ModuleId {
    match u {
        UnitRef::Scc(fs) => g.fun(fs[0]).module,
        UnitRef::Def(d) => g.def(*d).module,
        UnitRef::ImplMethod(i, _) => g.instances[*i].module,
        UnitRef::Macro(f) => g.fun(*f).module,
    }
}

/// The name of method `k` of instance `i`.
pub(crate) fn impl_method_name(g: &Globals, i: usize, k: usize) -> &str {
    let inst = &g.instances[i];
    &g.proto(inst.proto).methods[inst.methods[k].index].name
}

fn unit_line(g: &Globals, u: &UnitRef) -> String {
    let names =
        |fs: &[FunId]| -> Vec<String> { fs.iter().map(|f| g.fun(*f).name.clone()).collect() };
    match u {
        UnitRef::Scc(fs) => format!("unit scc {}", names(fs).join(" ")),
        UnitRef::Def(d) => format!("unit def {}", g.def(*d).name),
        UnitRef::ImplMethod(i, k) => format!("unit impl {i} {}", impl_method_name(g, *i, *k)),
        UnitRef::Macro(f) => format!("unit macro {}", g.fun(*f).name),
    }
}

/// The `unit` lines of the module, in the order the units were checked,
/// each followed by its tables when `tables` is set.
fn units(v: &View, s: &Shown, tables: bool, out: &mut String) {
    let Some(t) = v.typed else { return };
    for u in t.units.iter().filter(|u| unit_module(v.g, u) == s.id) {
        push(out, &unit_line(v.g, u));
        if tables {
            tables::unit_tables(v, t, (u, &s.file), out);
        }
    }
}
