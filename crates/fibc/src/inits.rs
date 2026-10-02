//! The `def`s made at run time (syntax §3.19, types §2.16, L15).
//!
//! A `def` whose initialiser is a constant expression (`DefDef::constant`)
//! is evaluated while compiling and emitted as static data (`defs/`). Any
//! other initialiser is evaluated **before `main`, once**, by an init
//! function: the program's `def`s are taken in the order the checker typed
//! them (a `def` after the `def`s its initialiser names, directly or through
//! the functions it calls; modules in dependency order), and each maximal
//! run of `def`s of one module is one init function, `fib.init.mN`. The
//! program's entry calls `fib.defs-init`, which calls the init functions in
//! that order, after `fib.init` and `fib.set-args` (so that a `def` may read
//! the arguments) and before `main`.
//!
//! Each such `def` has a **slot**, a global of the lIR module
//! (`def.slot.N`, [`Program::def_slots`]); reading the `def` is a count-free
//! load of it (types §6.1: a global is borrowed). Its init function does,
//! per `def`, in this order:
//!
//! 1. `fib.initdef := "def NAME: "`, so that a trap in the initialiser
//!    reads `trap: def NAME: MESSAGE` (the interpreter's text, `eval_def`);
//! 2. calls the initialiser, the body `BodyKey::Def`, which returns the
//!    value owned;
//! 3. `fib.immortalise` on a value that is an object: the value and
//!    everything reachable from it become IMMORTAL and SHARED, so that
//!    no count operation touches them, no write is in place, and any task
//!    may read them (a `def` has no `Cell` or `Weak`, an `Atom` locks
//!    because it is SHARED);
//! 4. stores the value into the slot.
//!
//! The allocations of the initialisers are not in the free trace (compiler.md
//! §4): `fib.defs-init` ends with `fib.init-done`, which prints the marker
//! `I k` that `trace::Trace::parse` cuts at, as the interpreter's trace
//! cuts at the last `Immortalised` event.

use std::fmt::Write;

use fibref::own::program::BodyKey;
use fibref::types::ast::DefId;
use fibref::types::decls::ModuleId;

use crate::compile::Unsupported;
use crate::defs::all_def_ids;
use crate::ir::LirTy;
use crate::program::Program;

/// One `def` made at run time.
#[derive(Clone, Debug)]
pub struct Init {
    pub def: DefId,
    /// The module that defines it.
    pub module: ModuleId,
    /// The code name of its initialiser's body.
    pub body: String,
    /// Whether its value is an object to make immortal.
    pub object: bool,
}

/// Chooses the `def`s made at run time, gives each its slot and requests
/// its body. Runs before the constants are made, because a function a
/// constant names may read one of these `def`s.
pub fn plan(p: &mut Program<'_>) -> Result<(), Unsupported> {
    for d in all_def_ids(p) {
        if p.g().def(d).constant {
            continue;
        }
        let t = p.c.typed.def_types[d.0 as usize]
            .clone()
            .ok_or_else(|| Unsupported("a def without a type".into()))?;
        let lt = p.lir(&t)?;
        if lt == Some(LirTy::Dyn) {
            return Err(Unsupported(format!(
                "def {}: a def made at run time holds a dyn value",
                p.g().def(d).name
            )));
        }
        let object = lt == Some(LirTy::Ptr);
        let body = p.request(BodyKey::Def(d), Vec::new());
        p.def_slots.insert(d, (format!("def.slot.{}", d.0), lt));
        p.inits.push(Init {
            def: d,
            module: p.g().def(d).module,
            body,
            object,
        });
    }
    Ok(())
}

/// The globals of the slots: `(global internal def.slot.N T zero)`.
pub fn slot_text(p: &Program<'_>) -> String {
    let mut out = String::new();
    for init in &p.inits {
        if let Some((slot, Some(l))) = p.def_slots.get(&init.def) {
            let _ = writeln!(out, "(global internal {slot} {} {})", l.text(), zero(*l));
        }
    }
    out
}

fn zero(l: LirTy) -> String {
    match l {
        LirTy::Ptr | LirTy::Raw => "(ptr null)".to_string(),
        LirTy::Float | LirTy::Double => format!("({} 0.0)", l.text()),
        _ => format!("({} 0)", l.text()),
    }
}

/// The init functions and `fib.defs-init`, or nothing if every `def`
/// is a constant.
pub fn render(p: &Program<'_>) -> String {
    if p.inits.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    let mut names = Vec::new();
    let mut start = 0;
    while start < p.inits.len() {
        let module = p.inits[start].module;
        let run = p.inits[start..]
            .iter()
            .take_while(|i| i.module == module)
            .count();
        let name = format!("fib.init.m{}.{}", module.0, names.len());
        out.push_str(&init_function(p, &name, &p.inits[start..start + run]));
        names.push(name);
        start += run;
    }
    let calls: String = names.iter().map(|n| format!("(call @{n}) ")).collect();
    let _ = writeln!(
        out,
        "(define internal (fib.defs-init void) ()\n  (block entry {calls}(call @fib.init-done) (ret)))"
    );
    out
}

/// The init function of one run of `def`s of a module.
fn init_function(p: &Program<'_>, name: &str, run: &[Init]) -> String {
    let mut body = String::new();
    let mut close = String::new();
    for (k, init) in run.iter().enumerate() {
        let def = &p.g().def(init.def).name;
        let _ = write!(body, "(store (string \"def {def}: \") @fib.initdef) ");
        let (slot, lt) = &p.def_slots[&init.def];
        match lt {
            None => {
                let _ = write!(body, "(call @{}) ", init.body);
            }
            Some(_) => {
                let _ = write!(body, "(let ((v{k} (call @{}))) ", init.body);
                close.push(')');
                if init.object {
                    let _ = write!(body, "(call @fib.immortalise v{k}) ");
                }
                let _ = write!(body, "(store v{k} @{slot}) ");
            }
        }
    }
    format!("(define internal ({name} void) ()\n  (block entry {body}(ret){close}))\n")
}
