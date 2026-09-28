//! Global initialisers (spec/lir.md §9).

use super::env::Env;
use super::expr::is_constant;
use crate::ast::{Expr, GlobalDef, Kind};
use crate::diag::{err, Result};
use crate::types::Type;

/// The initialiser is a constant of the global's type.
pub fn check_global(env: &Env, g: &GlobalDef) -> Result<()> {
    if !is_constant(&g.init) {
        return err(
            g.init.pos,
            format!("initializer of @{} must be a constant", g.name),
        );
    }
    let t = const_type(env, &g.init)?;
    if t != g.ty {
        return err(
            g.init.pos,
            format!("initializer of @{} has type {t}, expected {}", g.name, g.ty),
        );
    }
    Ok(())
}

/// The type of a constant expression.
pub fn const_type(env: &Env, e: &Expr) -> Result<Type> {
    match &e.kind {
        Kind::Int(t, _) | Kind::Float(t, _) | Kind::Vector(t, _) => Ok(t.clone()),
        Kind::Null | Kind::Str(_) => Ok(Type::Ptr),
        Kind::Global(n) if env.symbols.contains_key(n) => Ok(Type::Ptr),
        Kind::Global(n) => err(e.pos, format!("undefined global @{n}")),
        Kind::Struct(None, fs) => Ok(Type::Anon(
            fs.iter()
                .map(|f| const_type(env, f))
                .collect::<Result<_>>()?,
        )),
        Kind::Struct(Some(n), fs) => named(env, n, fs, e),
        _ => err(e.pos, "not a constant"),
    }
}

fn named(env: &Env, n: &str, fs: &[Expr], e: &Expr) -> Result<Type> {
    let ty = Type::Named(n.to_string());
    env.valid(&ty, e.pos)?;
    let want = &env.structs[n];
    if want.len() != fs.len() {
        return err(
            e.pos,
            format!(
                "{ty} literal: {} fields expected, found {}",
                want.len(),
                fs.len()
            ),
        );
    }
    for (i, (f, w)) in fs.iter().zip(want).enumerate() {
        let got = const_type(env, f)?;
        if got != *w {
            return err(
                f.pos,
                format!("{ty} literal: field {} has type {got}, expected {w}", i + 1),
            );
        }
    }
    Ok(ty)
}
