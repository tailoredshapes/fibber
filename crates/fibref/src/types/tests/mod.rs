//! Unit tests for the checker, split by what they cover: the prelude and
//! the 20 ownership cases (`cases`), and one test per type error of the
//! catalogue of types §6.14 (`errors`, `errors2`), and the rules of the
//! remaining forms (`forms`) and annotated bindings (`bindings`).

mod bindings;
mod cases;
mod colours;
mod diagnostics;
mod dyn_send;
mod errors;
mod errors2;
mod forms;
mod impl_colours;
mod patterns;
mod polyrec;
mod private;
mod supers;

use super::{check_source, ErrorKind, SourceError, TypeError, TypedProgram};

/// Checks `src` as the user module.
fn check(src: &str) -> Result<TypedProgram, Vec<TypeError>> {
    match check_source(src, "t.fib") {
        Ok(p) => Ok(p),
        Err(SourceError::Type(es)) => Err(es),
        Err(e) => panic!("{src}\ndoes not read or expand: {e:?}"),
    }
}

/// Checks `src` as a program with the implicit library (`fib.core`,
/// `fib.seq`, `fib.coll`, `fib.print`, which every program sees after the
/// flip), through the module loader: the type errors, or the typed program.
fn check_lib(src: &str) -> Result<TypedProgram, Vec<TypeError>> {
    match crate::own::check_source(src, "t.fib") {
        Ok(c) => Ok(c.typed),
        Err(crate::own::CheckError::Type(es)) => Err(es),
        Err(e) => panic!("{src}\ndoes not check: {e}"),
    }
}

/// [`ok`] with the implicit library.
fn ok_lib(src: &str) -> TypedProgram {
    check_lib(src).unwrap_or_else(|es| {
        let text: Vec<String> = es.iter().map(|e| e.to_string()).collect();
        panic!("{src}\nfailed:\n{}", text.join("\n"))
    })
}

/// Checks `src`, which must type-check.
fn ok(src: &str) -> TypedProgram {
    check(src).unwrap_or_else(|es| {
        let text: Vec<String> = es.iter().map(|e| e.to_string()).collect();
        panic!("{src}\nfailed:\n{}", text.join("\n"))
    })
}

/// Checks `src`, which must fail; its first error must be of `kind`
/// and its message must start with `text`.
fn fails(src: &str, kind: ErrorKind, text: &str) -> TypeError {
    fails_with(check(src), src, kind, text)
}

/// [`fails`] with the implicit library.
fn fails_lib(src: &str, kind: ErrorKind, text: &str) -> TypeError {
    fails_with(check_lib(src), src, kind, text)
}

fn fails_with(
    checked: Result<TypedProgram, Vec<TypeError>>,
    src: &str,
    kind: ErrorKind,
    text: &str,
) -> TypeError {
    let es = match checked {
        Ok(_) => panic!("{src}\ntype-checked; expected {kind:?}: {text}"),
        Err(es) => es,
    };
    let e = es[0].clone();
    assert_eq!(e.kind, kind, "{src}\n{e}");
    assert!(
        e.message.starts_with(text),
        "{src}\nexpected `{text}`, got `{}`",
        e.message
    );
    e
}

/// The type of the binding `name` (the first so named in the user
/// module), printed.
fn binding_type(p: &TypedProgram, name: &str) -> String {
    let (id, _) = p
        .globals
        .bindings
        .iter()
        .enumerate()
        .find(|(_, b)| {
            b.name == name && !b.pos.file.starts_with("lib/") && !b.pos.file.starts_with('<')
        })
        .unwrap_or_else(|| panic!("no binding {name}"));
    let t = &p.binding_types[&super::ast::BindingId(id as u32)];
    super::display::Printer::new(&p.globals).ty(t)
}
