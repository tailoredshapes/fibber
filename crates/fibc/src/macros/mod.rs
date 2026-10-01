//! Macros at compile time through the JIT (spec/compiler.md §6, syntax
//! §3.16): each `defmacro` is compiled, with the prelude, into a
//! macro-time module the `Jit` holds; an application converts the
//! argument forms into the module's `Form` objects, calls the entry
//! through `lair`'s C trampoline, and reads the result back; a node of
//! the result that is one of the arguments (or part of one) keeps the
//! position it was read at, every other node takes the call's (syntax
//! §1.3; `module.rs`).

pub mod abi;
#[cfg(feature = "llvm")]
pub mod bridge;
#[cfg(feature = "llvm")]
pub mod module;

#[cfg(feature = "llvm")]
pub use runner::JitRunner;

#[cfg(feature = "llvm")]
mod runner {
    use std::collections::HashMap;

    use super::macro_forms;
    use fibref::expand::MacroRunner;
    use fibref::expand::{ExpandCtx, ExpandError, ExpandErrorKind, MacroDef};
    use fibref::own::check_forms;
    use fibref::syntax::Form;
    use lair::{Jit, JitOptions};

    use super::bridge::Current;
    use super::module::{Fns, Inputs};
    use crate::compile::{compile_macro, Unsupported};

    /// The runner: one `Jit` for the whole expansion, one module per
    /// macro, compiled on its first application.
    pub struct JitRunner {
        prelude: Vec<Form>,
        jit: Jit,
        modules: HashMap<String, Result<Fns, ExpandErrorKind>>,
        /// The lIR text of every module, for `fibc emit --macros`.
        pub texts: Vec<(String, String)>,
    }

    impl JitRunner {
        pub fn new(prelude: Vec<Form>) -> Result<JitRunner, Unsupported> {
            let jit = Jit::new(JitOptions::default())
                .map_err(|e| Unsupported(format!("cannot start the JIT: {e}")))?;
            Ok(JitRunner {
                prelude,
                jit,
                modules: HashMap::new(),
                texts: Vec::new(),
            })
        }

        /// The module of macro `m`, compiled on first use.
        fn module(&mut self, m: &MacroDef) -> Result<Fns, ExpandErrorKind> {
            if let Some(r) = self.modules.get(&m.key) {
                return r.clone();
            }
            let r = self.build(m);
            self.modules.insert(m.key.clone(), r.clone());
            r
        }

        fn build(&mut self, m: &MacroDef) -> Result<Fns, ExpandErrorKind> {
            let failed = |message: String| ExpandErrorKind::MacroFailed {
                name: m.name.clone(),
                message,
            };
            let forms = macro_forms(m);
            let checked = check_forms(&forms, &self.prelude).map_err(|e| failed(e.to_string()))?;
            let n = m.params.len() + usize::from(m.rest.is_some());
            let k = self.texts.len();
            let compiled = compile_macro(&checked, &m.name, n, k).map_err(|u| failed(u.0))?;
            let name = format!("macro.{}.{k}", m.name);
            self.jit
                .add_source(&name, &compiled.text)
                .map_err(|e| failed(e.render(&name)))?;
            self.texts.push((m.name.clone(), compiled.text));
            let fns = Fns::lookup(&mut self.jit, k).map_err(|u| failed(u.0))?;
            // SAFETY: (fn void ()) as abi.rs defines fibm.init.
            let init: extern "C" fn() = unsafe {
                self.jit
                    .function(&format!("fibm.init.{k}"))
                    .map_err(|e| failed(e.to_string()))?
            };
            init();
            Ok(fns)
        }
    }

    impl MacroRunner for JitRunner {
        fn run(
            &mut self,
            m: &MacroDef,
            args: Vec<Form>,
            ctx: &ExpandCtx,
        ) -> Result<Form, ExpandError> {
            let pos = ctx.call_pos().clone();
            let fns = self.module(m).map_err(|k| ExpandError::new(k, &pos))?;
            let fixed = m.params.len().min(args.len());
            // The forms the macro is given, by the address of their
            // objects: what it returns of them keeps their positions.
            let mut inputs = Inputs::default();
            let mut objs: Vec<*const u8> = args[..fixed]
                .iter()
                .map(|a| fns.input_object(a, &mut inputs))
                .collect();
            if m.rest.is_some() {
                objs.push(fns.input_items(&args[fixed..], &mut inputs));
            }
            let mut cur = Current {
                ctx,
                fns: &fns,
                pos: pos.clone(),
                inputs: &inputs,
                error: None,
            };
            let cur_ptr: *mut Current<'_> = &mut cur;
            (fns.set_hooks)(
                super::bridge::gensym_hook as *const () as usize,
                super::bridge::reflect_hook as *const () as usize,
                cur_ptr as usize,
            );
            let result = call_entry(fns.entry, &objs);
            if let Some(e) = cur.error.take() {
                return Err(e);
            }
            fns.to_form(result, &pos, &inputs).map_err(|message| {
                ExpandError::new(
                    ExpandErrorKind::MacroFailed {
                        name: m.name.clone(),
                        message,
                    },
                    &pos,
                )
            })
        }
    }

    /// Calls the macro entry with `n` object arguments (the entry's type
    /// is `(fn ptr (ptr ..))` with as many parameters, abi.rs).
    fn call_entry(entry: usize, objs: &[*const u8]) -> *const u8 {
        type P = *const u8;
        // SAFETY: `entry` is the ccc trampoline of `fibm.entry`, whose
        // parameter count is `objs.len()` by construction (compile_macro).
        unsafe {
            match objs.len() {
                0 => std::mem::transmute::<usize, extern "C" fn() -> P>(entry)(),
                1 => std::mem::transmute::<usize, extern "C" fn(P) -> P>(entry)(objs[0]),
                2 => {
                    std::mem::transmute::<usize, extern "C" fn(P, P) -> P>(entry)(objs[0], objs[1])
                }
                3 => std::mem::transmute::<usize, extern "C" fn(P, P, P) -> P>(entry)(
                    objs[0], objs[1], objs[2],
                ),
                4 => std::mem::transmute::<usize, extern "C" fn(P, P, P, P) -> P>(entry)(
                    objs[0], objs[1], objs[2], objs[3],
                ),
                5 => std::mem::transmute::<usize, extern "C" fn(P, P, P, P, P) -> P>(entry)(
                    objs[0], objs[1], objs[2], objs[3], objs[4],
                ),
                _ => std::mem::transmute::<usize, extern "C" fn(P, P, P, P, P, P) -> P>(entry)(
                    objs[0], objs[1], objs[2], objs[3], objs[4], objs[5],
                ),
            }
        }
    }
}

/// The macro-time module's forms: the `defmacro` and a `main`, as the
/// interpreter's runner builds them (eval/macros.rs `macro_forms`).
pub fn macro_forms(m: &fibref::expand::MacroDef) -> Vec<fibref::syntax::Form> {
    use fibref::syntax::{Form, FormKind, IntWidth};
    let p = &m.pos;
    let sym = |s: &str| Form::new(FormKind::Sym(s.to_string()), p.clone());
    let list = |items: Vec<Form>| Form::new(FormKind::List(items), p.clone());
    let mut params: Vec<Form> = m.params.iter().map(|s| sym(s)).collect();
    if let Some(r) = &m.rest {
        params.push(sym("..."));
        params.push(sym(r));
    }
    let mut def = vec![sym("defmacro"), sym(&m.name), list(params)];
    def.extend(m.body.iter().cloned());
    let zero = Form::new(
        FormKind::Int {
            v: 0,
            width: IntWidth::I64,
        },
        p.clone(),
    );
    let main = list(vec![
        sym("defun"),
        sym("main"),
        list(Vec::new()),
        sym("->"),
        sym("i64"),
        zero,
    ]);
    vec![list(def), main]
}
