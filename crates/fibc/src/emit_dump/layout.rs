//! `--layout` (spec/bootstrap.md §8.5): what the emitter decides about every
//! ground type of the typed program before it emits a body: the mangled
//! name, the lIR type, the class and its representative, and for an object
//! its struct, type id, size and field offsets; then the structs and type
//! table the objects make. No body is lowered and no ownership plan is read,
//! so the names, the layout rules, the monomorphisation classes and the
//! table can be judged alone.

use std::collections::HashSet;

use fibref::own::Checked;
use fibref::types::display::Printer;
use fibref::types::program::TypedProgram;
use fibref::types::ty::{Colour, Leaf, Ty};

use crate::layout::lir_ty;
use crate::mono::{class_of, representative, Class};
use crate::names::mangle;
use crate::objects::{ObjInfo, ObjKind};
use crate::program::{tyname, Program};

/// The layout table and the type table of `checked`'s ground types: the
/// line `;; == layout`, one line per type, then `;; == section types` and
/// the text of the `types` section the objects make.
pub(crate) fn text(checked: &Checked) -> String {
    let mut p = Program::new(checked);
    let mut out = String::from(";; == layout\n");
    for t in ground_types(&checked.typed) {
        out.push_str(&line(&mut p, &t));
        out.push('\n');
    }
    out.push_str(";; == section types\n");
    out.push_str(&p.objects.render());
    out
}

/// The types of the expressions (in the order of their ids), of the
/// bindings (in the order of theirs) and of the instantiations (in the
/// order of the expressions that use them, each one's types in order), with
/// colours erased as the mangled name erases them, without the types that
/// still hold a variable, and without repeats.
fn ground_types(typed: &TypedProgram) -> Vec<Ty> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    let mut add = |t: &Ty| {
        let erased = t.map_colours(&mut |_| Colour::Local);
        let mut ground = true;
        erased.visit(&mut |l| ground &= matches!(l, Leaf::Colour(_)));
        if ground && seen.insert(erased.clone()) {
            out.push(erased);
        }
    };
    let mut exprs: Vec<_> = typed.expr_types.iter().collect();
    exprs.sort_by_key(|(id, _)| **id);
    exprs.iter().for_each(|(_, t)| add(t));
    let mut bindings: Vec<_> = typed.binding_types.iter().collect();
    bindings.sort_by_key(|(id, _)| **id);
    bindings.iter().for_each(|(_, t)| add(t));
    let mut uses: Vec<_> = typed.instantiations.iter().collect();
    uses.sort_by_key(|(id, _)| **id);
    uses.iter().flat_map(|(_, i)| &i.tys).for_each(&mut add);
    out
}

/// `TYPE mangle M lir L class C repr R object O`, where `O` is `-` for a
/// type that is no object, else `SNAME tid N size S offsets OFFSETS`; a
/// type the compiler cannot lay out ends `unsupported` after the columns
/// that were found.
fn line(p: &mut Program<'_>, t: &Ty) -> String {
    let g = p.g();
    let head = format!(
        "{} mangle {}",
        Printer::with_names(g, &[], &[]).ty(t),
        mangle(g, t)
    );
    let (Ok(lir), Ok(class)) = (lir_ty(g, t), class_of(g, t)) else {
        return format!("{head} unsupported");
    };
    let lir = lir.map_or("void", tyname);
    let repr = mangle(g, &representative(g, class));
    let object = object_columns(p, t, class);
    format!(
        "{head} lir {lir} class {} repr {repr} {object}",
        class_word(class)
    )
}

fn class_word(c: Class) -> &'static str {
    match c {
        Class::Scalar(_) => "scalar",
        Class::Ptr => "ptr",
        Class::Opt => "opt",
        Class::Boxed => "boxed",
        Class::Dyn => "dyn",
        Class::Unit => "unit",
    }
}

/// The columns of the object `t` is, registering it in the type table: only
/// a pointer class that is not a closure (closures are objects per code,
/// registered when their literal is lowered) has one.
fn object_columns(p: &mut Program<'_>, t: &Ty, class: Class) -> String {
    if !matches!(class, Class::Ptr | Class::Boxed) || matches!(t, Ty::Fn(..)) {
        return "object -".to_string();
    }
    match p.object(t) {
        Err(_) => "object unsupported".to_string(),
        Ok((tid, sname)) => {
            let o = p.objects.get(tid);
            format!(
                "object {sname} tid {tid} size {} offsets {}",
                o.size(),
                offsets_word(o)
            )
        }
    }
}

/// The byte offsets of an object's user slots after its header, separated by
/// commas (`-` for none); an enum's are listed by variant, `v0:- v1:16,24`.
fn offsets_word(o: &ObjInfo) -> String {
    let list = |v: Vec<u64>| {
        if v.is_empty() {
            "-".to_string()
        } else {
            v.iter().map(u64::to_string).collect::<Vec<_>>().join(",")
        }
    };
    match &o.kind {
        ObjKind::Enum(vs) => (0..vs.len())
            .map(|i| format!("v{i}:{}", list(o.offsets(Some(i)))))
            .collect::<Vec<_>>()
            .join(" "),
        _ => list(o.offsets(None)),
    }
}
