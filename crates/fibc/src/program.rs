//! The state of one compilation (compiler.md §2): the checked program,
//! the object types, the static data, the queue of bodies, and the
//! functions emitted so far.

use std::collections::VecDeque;
use std::fmt::Write;

use fibref::own::program::BodyKey;
use fibref::own::Checked;
use fibref::types::ast::ExprId;
use fibref::types::decls::Globals;
use fibref::types::ty::{Con, Ty};

use crate::compile::Unsupported;
use crate::ir::LirTy;
use crate::layout::{lir_ty, option_payload, option_rep, variants, OptRep};
use crate::mono::{Inst, Queue};
use crate::names::{closure_name, mangle};
use crate::objects::{ObjKind, Objects};
use crate::statics::Statics;

/// Something to emit.
#[derive(Clone, Debug)]
pub enum Work {
    /// A body at a specialisation.
    Body(Inst),
    /// The code of closure literal `lit` inside the body `owner` at
    /// its specialisation, under the given name.
    Closure {
        owner: Inst,
        lit: ExprId,
        name: String,
    },
}

/// The compilation.
pub struct Program<'a> {
    pub c: &'a Checked,
    pub objects: Objects,
    pub statics: Statics,
    pub queue: Queue,
    closures: VecDeque<Work>,
    pub funcs: Vec<String>,
    externs: Vec<String>,
    helpers: std::collections::HashSet<String>,
    /// Every `def`'s value as lIR text with its type (compile-time
    /// evaluated, `defs.rs`).
    pub def_values: std::collections::HashMap<fibref::types::ast::DefId, (String, Option<LirTy>)>,
    /// The constant of every quoted form, by its expression, and the
    /// constants' text (`lower/quote.rs`).
    pub quotes: std::collections::HashMap<ExprId, String>,
    pub quote_text: String,
    quote_counter: usize,
    /// Whether a body used `show` or `hash` on a keyword, so that the
    /// module needs `kw.show` and `kw.hash` over every keyword interned.
    pub keyword_helpers: bool,
}

impl<'a> Program<'a> {
    pub fn new(c: &'a Checked) -> Self {
        let mut p = Program {
            c,
            objects: Objects::default(),
            statics: Statics::default(),
            queue: Queue::default(),
            closures: VecDeque::new(),
            funcs: Vec::new(),
            externs: Vec::new(),
            helpers: std::collections::HashSet::new(),
            def_values: std::collections::HashMap::new(),
            quotes: std::collections::HashMap::new(),
            quote_text: String::new(),
            quote_counter: 0,
            keyword_helpers: false,
        };
        // `str` is type id 0: the literals need it before any body runs;
        // the runtime's own text names the byte array and the weak box.
        p.objects.intern("fib.str", "str", ObjKind::Str);
        p.objects
            .intern("fib.array.i8", "(Array i8)", ObjKind::Array(LirTy::I8));
        p.objects.intern("fib.weakbox", "weak box", ObjKind::Weak);
        p.objects.intern(
            "fib.task",
            "task",
            ObjKind::Task {
                caps: Vec::new(),
                frame: Vec::new(),
            },
        );
        p
    }

    pub fn g(&self) -> &'a Globals {
        &self.c.typed.globals
    }

    /// The concrete type of expression `e` in `inst`.
    pub fn ty_of(&self, inst: &Inst, e: ExprId) -> Result<Ty, Unsupported> {
        self.c
            .typed
            .expr_types
            .get(&e)
            .map(|t| inst.subst(t))
            .ok_or_else(|| Unsupported(format!("expression {e:?} has no type")))
    }

    /// The lIR type of a concrete type; `None` for `unit`.
    pub fn lir(&self, t: &Ty) -> Result<Option<LirTy>, Unsupported> {
        lir_ty(self.g(), t)
    }

    /// The name of a body's specialisation, queued if new.
    pub fn request(&mut self, key: BodyKey, tys: Vec<Ty>) -> String {
        let g = self.g();
        // Colours have no representation (§8.4): one specialisation
        // serves every colour instantiation.
        let tys = tys
            .iter()
            .map(|t| t.map_colours(&mut |_| fibref::types::ty::Colour::Local))
            .collect();
        self.queue.request(g, Inst { key, tys })
    }

    /// The code name of closure literal `lit` of body `owner`, queued.
    pub fn request_closure(&mut self, owner: &Inst, owner_name: &str, lit: ExprId) -> String {
        let name = closure_name(owner_name, lit.0);
        self.closures.push_back(Work::Closure {
            owner: owner.clone(),
            lit,
            name: name.clone(),
        });
        name
    }

    /// The next thing to emit.
    pub fn next_work(&mut self) -> Option<Work> {
        if let Some(w) = self.closures.pop_front() {
            return Some(w);
        }
        self.queue.pop().map(Work::Body)
    }

    /// The type id and struct name of the object type `t` (a struct,
    /// an enum with fields, `str`, an array, a boxed `Option`).
    pub fn object(&mut self, t: &Ty) -> Result<(u32, String), Unsupported> {
        let g = self.g();
        let (sname, kind) = match t {
            Ty::Con(Con::Str, _) => ("fib.str".to_string(), ObjKind::Str),
            Ty::Con(Con::Array, args) => {
                let e = self.elem_lir(args)?;
                (format!("fib.array.{}", tyname(e)), ObjKind::Array(e))
            }
            Ty::Con(Con::Cell, args) => {
                let e = self.elem_lir(args)?;
                (format!("fib.cell.{}", tyname(e)), ObjKind::Cell(e))
            }
            Ty::Con(Con::Atom, args) => {
                let e = self.elem_lir(args)?;
                (format!("fib.atom.{}", tyname(e)), ObjKind::Atom(e))
            }
            Ty::Con(Con::Weak, _) => ("fib.weakbox".to_string(), ObjKind::Weak),
            Ty::Con(Con::Task, _) => (
                "fib.task".to_string(),
                ObjKind::Task {
                    caps: Vec::new(),
                    frame: Vec::new(),
                },
            ),
            Ty::Con(Con::Nominal(id), args) if *id == g.option => {
                let payload = option_payload(g, t).cloned().unwrap_or(Ty::unit());
                if option_rep(g, &payload)? == OptRep::Null {
                    return Err(Unsupported(
                        "a null-represented Option has no object".into(),
                    ));
                }
                let f = lir_ty(g, &payload)?;
                (
                    format!("o.{}", mangle(g, t)),
                    ObjKind::Enum(vec![vec![], vec![f]]),
                )
            }
            Ty::Con(Con::Nominal(id), args) => {
                let vs = variants(g, *id, args)?;
                let mut lowered = Vec::new();
                for (_, fields) in &vs {
                    let mut fs = Vec::new();
                    for f in fields {
                        fs.push(lir_ty(g, f)?);
                    }
                    lowered.push(fs);
                }
                let kind = match vs[0].0 {
                    None => ObjKind::Struct(lowered.remove(0)),
                    Some(_) => ObjKind::Enum(lowered),
                };
                (format!("o.{}", mangle(g, t)), kind)
            }
            _ => {
                return Err(Unsupported(format!(
                    "no object layout for {}",
                    mangle(g, t)
                )))
            }
        };
        let name = fibref::types::display::Printer::with_names(g, &[], &[]).ty(t);
        let tid = self.objects.intern(&sname, &name, kind);
        Ok((tid, sname))
    }

    /// The element layout of a container; a `unit` content takes an
    /// `i1` slot that nothing reads (an atom of `unit` is a task's
    /// result holder).
    fn elem_lir(&self, args: &[Ty]) -> Result<LirTy, Unsupported> {
        let t = args
            .first()
            .ok_or_else(|| Unsupported("a constructor without its argument".into()))?;
        Ok(self.lir(t)?.unwrap_or(LirTy::I1))
    }

    /// Declares an `extern` (syntax §3.15) for the module, once.
    pub fn declare_extern(
        &mut self,
        d: &fibref::types::decls::ExternDef,
    ) -> Result<(), Unsupported> {
        let Ty::Fn(_, params, ret) = &d.ty else {
            return Err(Unsupported("an extern without a function type".into()));
        };
        let g = self.g();
        let mut ps = Vec::new();
        for t in params {
            ps.push(
                lir_ty(g, t)?
                    .ok_or_else(|| Unsupported("a unit extern parameter".into()))?
                    .text(),
            );
        }
        if d.varargs {
            ps.push("...");
        }
        let r = lir_ty(g, ret)?.map_or("void", LirTy::text);
        let decl = format!("(declare {} {r} ({}))\n", d.name, ps.join(" "));
        if !self.externs.contains(&decl) {
            self.externs.push(decl);
        }
        Ok(())
    }

    /// A fresh name for a quoted form's constant.
    pub fn fresh_quote(&mut self) -> String {
        self.quote_counter += 1;
        format!("@q.{}", self.quote_counter)
    }

    /// Whether a generated runtime helper of this name exists.
    pub fn has_helper(&self, name: &str) -> bool {
        self.helpers.contains(name)
    }

    /// Records a generated helper function's text.
    pub fn add_helper(&mut self, name: &str, text: String) {
        self.helpers.insert(name.to_string());
        self.funcs.push(text);
    }

    /// The extern declarations.
    pub fn render_externs(&self) -> String {
        self.externs.concat()
    }

    /// `kw.show` and `kw.hash`, switching on the keyword id (types
    /// §2.12: `show` is `:` and the name, `hash` the FNV-1a of the
    /// name), when a body needed them; interns the names' strings, so
    /// this runs before the statics are rendered.
    pub fn render_keyword_helpers(&mut self) -> String {
        if !self.keyword_helpers {
            return String::new();
        }
        let (mut show, mut hash) = (String::new(), String::new());
        let mut cases = Vec::new();
        for (i, k) in self.statics.keywords().iter().enumerate() {
            let colon = self.statics.string(&format!(":{k}"), 0);
            let name = self.statics.string(k, 0);
            cases.push(format!("((i64 {i}) k{i})"));
            let _ = writeln!(show, "  (block k{i} (ret (call @fib.str-copy {colon})))");
            let _ = writeln!(hash, "  (block k{i} (ret (call @fib.str-hash {name})))");
        }
        let cases = cases.join(" ");
        format!(
            "(define internal (kw.show ptr) ((i64 k))\n  (block entry (switch k bad {cases}))\n{show}  (block bad (unreachable)))\n\
             (define internal (kw.hash i64) ((i64 k))\n  (block entry (switch k bad {cases}))\n{hash}  (block bad (unreachable)))\n"
        )
    }

    /// The type id and struct name of the task object of the `async`
    /// body `code`, with its captures.
    pub fn task_object(&mut self, code: &str, caps: Vec<LirTy>) -> (u32, String) {
        let sname = format!("task.{code}");
        let tid = self.objects.intern(
            &sname,
            &format!("task {code}"),
            ObjKind::Task {
                caps,
                frame: Vec::new(),
            },
        );
        (tid, sname)
    }

    /// The type id and struct name of the closure object of `code`.
    pub fn closure_object(&mut self, code: &str, caps: Vec<LirTy>) -> (u32, String) {
        let sname = format!("clo.{code}");
        let tid = self
            .objects
            .intern(&sname, &format!("closure {code}"), ObjKind::Closure(caps));
        (tid, sname)
    }
}

/// A short name for an lIR type inside a struct name.
pub fn tyname(t: LirTy) -> &'static str {
    match t {
        LirTy::Dyn => "dyn",
        other => other.text(),
    }
}
