//! Completion candidates from a checked program: what each module
//! exports, with the type checker's scheme as the detail.

use std::collections::BTreeSet;

use crate::json::Json;
use crate::types::ast::{BindingId, GlobalRef};
use crate::types::decls::{Globals, ModuleId, Shape};
use crate::types::display::Printer;
use crate::types::program::show_scheme_in;
use crate::types::ty::{Con, Ty, TypeId};
use crate::types::TypedProgram;

/// One completion candidate (the protocol's item).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub label: String,
    pub kind: &'static str,
    pub detail: String,
    pub doc: String,
}

impl Item {
    pub fn new(label: &str, kind: &'static str, detail: String, doc: &str) -> Item {
        Item {
            label: label.to_string(),
            kind,
            detail,
            doc: doc.to_string(),
        }
    }

    /// The item as the protocol's JSON object.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("label", Json::str(&self.label)),
            ("kind", Json::str(self.kind)),
            ("detail", Json::str(&self.detail)),
            ("doc", Json::str(&self.doc)),
        ])
    }
}

/// The scalar type names (syntax §1.5).
const SCALARS: [&str; 12] = [
    "i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64", "f32", "f64", "bool", "str",
];

/// An item for the value `name` as module `m` sees it.
pub fn value_item(t: &TypedProgram, m: ModuleId, name: &str) -> Option<Item> {
    let g = &t.globals;
    let r = g.value(m, name)?;
    let (kind, detail, owner) = match r {
        GlobalRef::Fun(f) => {
            let def = g.fun(f);
            let s = t.fun_schemes.get(f.0 as usize).and_then(Option::as_ref);
            let kind = if def.is_macro { "macro" } else { "function" };
            (kind, s.map(|s| show_scheme_in(g, s)), def.module)
        }
        GlobalRef::Ctor(id, v) => ctor_parts(g, id, v),
        GlobalRef::Method(p, i) => {
            let proto = g.proto(p);
            let s = proto.methods.get(i).map(|m| show_scheme_in(g, &m.scheme));
            ("method", s, proto.module)
        }
        GlobalRef::Builtin(b) => {
            let s = g.builtin_schemes.get(b.0 as usize);
            (
                "function",
                s.map(|s| show_scheme_in(g, s)),
                ModuleId::BUILTIN,
            )
        }
        GlobalRef::Def(d) => {
            let ty = t.def_types.get(d.0 as usize).and_then(Option::as_ref);
            let owner = g.def(d).module;
            ("variable", ty.map(|ty| Printer::new(g).ty(ty)), owner)
        }
        GlobalRef::Extern(x) => ("function", Some(Printer::new(g).ty(&g.ext(x).ty)), m),
    };
    Some(Item::new(
        name,
        kind,
        detail.unwrap_or_default(),
        g.module_name(owner),
    ))
}

fn ctor_parts(
    g: &Globals,
    id: TypeId,
    variant: Option<usize>,
) -> (&'static str, Option<String>, ModuleId) {
    let td = g.ty(id);
    match variant {
        None => (
            "struct",
            Some(format!("constructor of {}", td.name)),
            td.module,
        ),
        Some(_) => (
            "variant",
            Some(format!("variant of {}", td.name)),
            td.module,
        ),
    }
}

/// An item for the type `name` as `m` sees it.
fn type_item(g: &Globals, m: ModuleId, name: &str) -> Option<Item> {
    let id = g.type_name(m, name)?;
    let td = g.ty(id);
    let kind = if matches!(td.shape, Shape::Struct(_)) {
        "struct"
    } else {
        "enum"
    };
    Some(Item::new(
        name,
        kind,
        td.params.join(" "),
        g.module_name(td.module),
    ))
}

fn proto_item(g: &Globals, m: ModuleId, name: &str) -> Option<Item> {
    let id = g.proto_name(m, name)?;
    Some(Item::new(
        name,
        "protocol",
        String::new(),
        g.module_name(g.proto(id).module),
    ))
}

/// The names `m` sees by itself and through its chain, private ones of
/// its own included.
fn visible(g: &Globals, m: ModuleId) -> BTreeSet<String> {
    let mut all: BTreeSet<String> = g.exports(m);
    let own = g.names(m);
    all.extend(
        own.values
            .keys()
            .chain(own.types.keys())
            .chain(own.protos.keys())
            .cloned(),
    );
    for dep in g.module(m).chain.iter().filter(|d| **d != m) {
        all.extend(g.exports(*dep));
    }
    all
}

/// Every value, type and protocol visible in the main module, and the
/// aliases (as `alias/`).
pub fn names(t: &TypedProgram) -> Vec<Item> {
    let g = &t.globals;
    let m = g.main;
    let mut out = Vec::new();
    for name in visible(g, m) {
        out.extend(value_item(t, m, &name));
        out.extend(type_item(g, m, &name));
        out.extend(proto_item(g, m, &name));
    }
    let mut aliases: Vec<_> = g.module(m).aliases.iter().collect();
    aliases.sort_by(|a, b| a.0.cmp(b.0));
    for (alias, id) in aliases {
        out.push(Item::new(
            &format!("{alias}/"),
            "module",
            String::new(),
            g.module_name(*id),
        ));
    }
    out
}

/// Every type and protocol visible in the main module, and the scalars.
pub fn types(t: &TypedProgram) -> Vec<Item> {
    let g = &t.globals;
    let mut out: Vec<Item> = SCALARS
        .iter()
        .map(|s| Item::new(s, "type", String::new(), "builtin"))
        .collect();
    for name in visible(g, g.main) {
        out.extend(type_item(g, g.main, &name));
        out.extend(proto_item(g, g.main, &name));
    }
    out
}

/// What the module aliased `alias` in the main module exports.
pub fn exports(t: &TypedProgram, alias: &str) -> Vec<Item> {
    let g = &t.globals;
    let Some(m) = g.module(g.main).aliases.get(alias).copied() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for name in g.exports(m) {
        out.extend(value_item(t, m, &name));
        out.extend(type_item(g, m, &name));
        out.extend(proto_item(g, m, &name));
    }
    out
}

/// The fields of the struct `id`.
pub fn fields(g: &Globals, id: TypeId) -> Vec<Item> {
    let td = g.ty(id);
    let Shape::Struct(fields) = &td.shape else {
        return Vec::new();
    };
    let p = Printer::with_names(g, &td.params, &[]);
    fields
        .iter()
        .map(|f| Item::new(&f.name, "field", p.ty(&f.ty), &td.name))
        .collect()
}

/// The checked type of the nearest binding named `name` at or before
/// byte `off` of the file `path`: its nominal type and its text.
pub fn local_type(
    t: &TypedProgram,
    name: &str,
    path: &str,
    off: usize,
) -> Option<(Option<TypeId>, String)> {
    let g = &t.globals;
    let (id, _) = g
        .bindings
        .iter()
        .enumerate()
        .filter(|(_, b)| b.name == name && &*b.pos.file == path && b.pos.start <= off)
        .max_by_key(|(_, b)| b.pos.start)?;
    let ty = t.binding_types.get(&BindingId(id as u32))?;
    let nominal = match ty {
        Ty::Con(Con::Nominal(n), _) => Some(*n),
        _ => None,
    };
    Some((nominal, Printer::new(g).ty(ty)))
}
