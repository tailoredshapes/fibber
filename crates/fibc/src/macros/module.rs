//! One compiled macro-time module in the JIT: its C entry points
//! (`abi.rs`) as function pointers, its table of keywords (read once,
//! through `fibm.kw-count` and `fibm.kw`, as the stage-2 compiler reads
//! it), and the conversions between the expander's forms and the
//! module's `Form` objects.
//!
//! Positions (syntax §1.3): a form a macro produces carries the position
//! of the macro call, unless the macro built it from an input form, which
//! keeps its own. The objects of the arguments are made immortal by
//! `fibm.form` and never freed, so each has an address that is its
//! identity for the whole run: [`Inputs`] maps the address of every
//! node built for an argument to the form it came from, and `to_form`
//! reads a result back through it. A node whose address is in the table
//! is that form, subtree and positions included, as the interpreter's
//! `value_form` answers; any other node is new, takes the call's
//! position, and its children are read by the same rule (a new list of
//! old children: the list at the call, the children at their own).

use std::collections::HashMap;

use fibref::syntax::{FltWidth, Form, FormKind, IntWidth, Pos};
use lair::Jit;

use super::abi::{VARIANTS, WIDTHS};
use crate::compile::Unsupported;

type P = *const u8;

/// The forms a macro run was given, by the address of the `Form` object
/// made for each node of them (the arguments, their descendants, and the
/// elements of a rest argument; not the rest vector, which is no form).
/// Borrowed from the arguments, which outlive the run.
#[derive(Default)]
pub struct Inputs<'a> {
    by_address: HashMap<usize, &'a Form>,
}

impl<'a> Inputs<'a> {
    /// The input form behind `o`, if `o` is the object made for one.
    fn get(&self, o: P) -> Option<&'a Form> {
        self.by_address.get(&(o as usize)).copied()
    }

    fn record(&mut self, o: P, f: &'a Form) {
        self.by_address.insert(o as usize, f);
    }
}

/// The function pointers of a module.
#[derive(Clone)]
pub struct Fns {
    pub entry: usize,
    str_new: extern "C" fn(P, i64) -> P,
    form: extern "C" fn(i32, i64, i64, P) -> P,
    vec: extern "C" fn(P, i64) -> P,
    tag: extern "C" fn(P) -> i32,
    f0_ptr: extern "C" fn(P) -> P,
    f0_i64: extern "C" fn(P) -> i64,
    f0_f64: extern "C" fn(P) -> f64,
    f0_i32: extern "C" fn(P) -> i32,
    f0_i1: extern "C" fn(P) -> i32,
    f1_i64: extern "C" fn(P) -> i64,
    str_len: extern "C" fn(P) -> i64,
    str_ptr: extern "C" fn(P) -> P,
    vec_len: extern "C" fn(P) -> i64,
    vec_elem: extern "C" fn(P, i64) -> P,
    pub set_hooks: extern "C" fn(usize, usize, usize),
    /// Not part of the interface a fibber-written runner uses: it sets
    /// no trap hook, and a macro of its module that traps aborts.
    pub set_trap_hook: extern "C" fn(usize),
    /// The keyword ids of the width suffixes, in the order of `WIDTHS`.
    widths: [i64; 6],
    /// Every keyword the module interned, by id: the module's table.
    keywords: Vec<String>,
}

impl Fns {
    /// Looks the module's functions and its keyword table up in the JIT.
    /// A module whose table lacks a width suffix cannot be used: a form
    /// of that width could not be built for it.
    pub fn lookup(jit: &mut Jit, k: usize) -> Result<Fns, Unsupported> {
        let n = |s: &str| format!("fibm.{s}.{k}");
        // SAFETY: each name is defined by `abi.rs` with exactly the
        // signature transmuted to here; the JIT outlives the runner.
        let mut fns = unsafe {
            let e = |m: lair::Error| Unsupported(format!("macro module: {m}"));
            Fns {
                entry: jit.c_entry(&n("entry")).map_err(e)?,
                str_new: jit.function(&n("str")).map_err(e)?,
                form: jit.function(&n("form")).map_err(e)?,
                vec: jit.function(&n("vec")).map_err(e)?,
                tag: jit.function(&n("tag")).map_err(e)?,
                f0_ptr: jit.function(&n("f0-ptr")).map_err(e)?,
                f0_i64: jit.function(&n("f0-i64")).map_err(e)?,
                f0_f64: jit.function(&n("f0-f64")).map_err(e)?,
                f0_i32: jit.function(&n("f0-i32")).map_err(e)?,
                f0_i1: jit.function(&n("f0-i1")).map_err(e)?,
                f1_i64: jit.function(&n("f1-i64")).map_err(e)?,
                str_len: jit.function(&n("str-len")).map_err(e)?,
                str_ptr: jit.function(&n("str-ptr")).map_err(e)?,
                vec_len: jit.function(&n("vec-len")).map_err(e)?,
                vec_elem: jit.function(&n("vec-elem")).map_err(e)?,
                set_hooks: jit.function(&n("set-hooks")).map_err(e)?,
                set_trap_hook: jit.function(&n("set-trap-hook")).map_err(e)?,
                widths: [0; 6],
                keywords: Vec::new(),
            }
        };
        // SAFETY: as above, `abi.rs` defines (fn i64 ()) and (fn ptr (i64)).
        let (count, name): (extern "C" fn() -> i64, extern "C" fn(i64) -> P) = unsafe {
            let e = |m: lair::Error| Unsupported(format!("macro module: {m}"));
            (
                jit.function(&n("kw-count")).map_err(e)?,
                jit.function(&n("kw")).map_err(e)?,
            )
        };
        fns.keywords = fns.read_table(count(), name)?;
        fns.widths = fns.width_ids()?;
        Ok(fns)
    }

    /// The names of the keywords with ids 0 to `count` - 1.
    fn read_table(
        &self,
        count: i64,
        name: extern "C" fn(i64) -> P,
    ) -> Result<Vec<String>, Unsupported> {
        (0..count)
            .map(|id| {
                let s = name(id);
                if s.is_null() {
                    return Err(Unsupported(format!(
                        "macro module: the keyword table has no name for keyword {id} of {count}"
                    )));
                }
                Ok(self.read_str(s))
            })
            .collect()
    }

    /// The ids of the width suffixes, found in the table by name.
    fn width_ids(&self) -> Result<[i64; 6], Unsupported> {
        let mut ids = [0i64; 6];
        for (id, w) in ids.iter_mut().zip(WIDTHS) {
            let at = self.keywords.iter().position(|k| k == w);
            *id = at.map(|i| i as i64).ok_or_else(|| {
                Unsupported(format!("macro module: the keyword table has no :{w}"))
            })?;
        }
        Ok(ids)
    }

    fn tag_of(name: &str) -> i32 {
        VARIANTS.iter().position(|v| *v == name).unwrap_or(7) as i32
    }

    fn str_object(&self, s: &str) -> P {
        (self.str_new)(s.as_ptr(), s.len() as i64)
    }

    fn width_id(&self, suffix: &str) -> i64 {
        WIDTHS
            .iter()
            .position(|w| *w == suffix)
            .map_or(-1, |i| self.widths[i])
    }

    /// A new form as a `Form` object of the module (a `gensym` or a
    /// reflection answer): read back from a result it takes the call's
    /// position, as nothing records it.
    pub fn to_object(&self, f: &Form) -> P {
        self.build(f, None)
    }

    /// An argument of a macro as a `Form` object of the module, every
    /// node of it recorded in `inputs`.
    pub fn input_object<'a>(&self, f: &'a Form, inputs: &mut Inputs<'a>) -> P {
        self.build(f, Some(inputs))
    }

    /// A `(Vec Form)` object of the rest arguments of a macro, each form
    /// recorded in `inputs`.
    pub fn input_items<'a>(&self, items: &'a [Form], inputs: &mut Inputs<'a>) -> P {
        self.build_items(items, Some(inputs))
    }

    /// The object of `f`, and of its tree, recorded in `rec` if there is one.
    fn build<'a>(&self, f: &'a Form, mut rec: Option<&mut Inputs<'a>>) -> P {
        let null = std::ptr::null();
        let o = match &f.kind {
            FormKind::Sym(s) => (self.form)(Self::tag_of("Sym"), 0, 0, self.str_object(s)),
            FormKind::Kw(s) => (self.form)(Self::tag_of("Kw"), 0, 0, self.str_object(s)),
            FormKind::Str(s) => (self.form)(Self::tag_of("Str"), 0, 0, self.str_object(s)),
            FormKind::Int { v, width } => {
                (self.form)(Self::tag_of("Int"), *v, self.width_id(width.suffix()), null)
            }
            FormKind::Flt { v, width } => {
                let suffix = if *width == FltWidth::F32 {
                    "f32"
                } else {
                    "f64"
                };
                (self.form)(
                    Self::tag_of("Flt"),
                    v.to_bits() as i64,
                    self.width_id(suffix),
                    null,
                )
            }
            FormKind::Chr(c) => (self.form)(Self::tag_of("Chr"), i64::from(u32::from(*c)), 0, null),
            FormKind::Bool(b) => (self.form)(Self::tag_of("Bool"), i64::from(*b), 0, null),
            FormKind::Nil => (self.form)(Self::tag_of("Nil"), 0, 0, null),
            FormKind::List(items) => {
                let v = self.build_items(items, rec.as_deref_mut());
                (self.form)(Self::tag_of("List"), 0, 0, v)
            }
            FormKind::Vec(items) => {
                let v = self.build_items(items, rec.as_deref_mut());
                (self.form)(Self::tag_of("Vec"), 0, 0, v)
            }
            FormKind::Map(items) => {
                let v = self.build_items(items, rec.as_deref_mut());
                (self.form)(Self::tag_of("Map"), 0, 0, v)
            }
        };
        if let Some(inputs) = rec {
            inputs.record(o, f);
        }
        o
    }

    /// A `(Vec Form)` object of these forms.
    fn build_items<'a>(&self, items: &'a [Form], mut rec: Option<&mut Inputs<'a>>) -> P {
        let objs: Vec<P> = items
            .iter()
            .map(|f| self.build(f, rec.as_deref_mut()))
            .collect();
        (self.vec)(objs.as_ptr().cast(), objs.len() as i64)
    }

    /// The text of a `str` object.
    pub fn read_str(&self, s: P) -> String {
        let n = (self.str_len)(s) as usize;
        let p = (self.str_ptr)(s);
        // SAFETY: a str object holds n bytes at p (types §8.3).
        let bytes = unsafe { std::slice::from_raw_parts(p, n) };
        String::from_utf8_lossy(bytes).into_owned()
    }

    /// A `Form` object of the module as a form: the input form it was
    /// made for, if `inputs` has it, else a new form at `pos`.
    pub fn to_form(&self, o: P, pos: &Pos, inputs: &Inputs) -> Result<Form, String> {
        if let Some(f) = inputs.get(o) {
            return Ok(f.clone());
        }
        let tag = (self.tag)(o) as usize;
        let name = VARIANTS.get(tag).copied().unwrap_or("Nil");
        let kind = match name {
            "Sym" => FormKind::Sym(self.read_str((self.f0_ptr)(o))),
            "Kw" => FormKind::Kw(self.read_str((self.f0_ptr)(o))),
            "Str" => FormKind::Str(self.read_str((self.f0_ptr)(o))),
            "Int" => {
                let w = self.width_name((self.f1_i64)(o));
                let width = IntWidth::from_suffix(&w)
                    .ok_or_else(|| format!("trap: an Int form of width :{w}"))?;
                FormKind::Int {
                    v: (self.f0_i64)(o),
                    width,
                }
            }
            "Flt" => {
                let w = self.width_name((self.f1_i64)(o));
                let width = FltWidth::from_suffix(&w)
                    .ok_or_else(|| format!("trap: a Flt form of width :{w}"))?;
                FormKind::Flt {
                    v: (self.f0_f64)(o),
                    width,
                }
            }
            "Chr" => FormKind::Chr(char::from_u32((self.f0_i32)(o) as u32).unwrap_or('\0')),
            "Bool" => FormKind::Bool((self.f0_i1)(o) != 0),
            "List" => FormKind::List(self.read_items((self.f0_ptr)(o), pos, inputs)?),
            "Vec" => FormKind::Vec(self.read_items((self.f0_ptr)(o), pos, inputs)?),
            "Map" => FormKind::Map(self.read_items((self.f0_ptr)(o), pos, inputs)?),
            _ => FormKind::Nil,
        };
        Ok(Form::new(kind, pos.clone()))
    }

    /// The name of a keyword id, from the module's table; an id that is
    /// none is reported by its number.
    fn width_name(&self, id: i64) -> String {
        usize::try_from(id)
            .ok()
            .and_then(|i| self.keywords.get(i))
            .cloned()
            .unwrap_or_else(|| format!("#{id}"))
    }

    fn read_items(&self, v: P, pos: &Pos, inputs: &Inputs) -> Result<Vec<Form>, String> {
        let n = (self.vec_len)(v);
        (0..n)
            .map(|i| self.to_form((self.vec_elem)(v, i), pos, inputs))
            .collect()
    }
}
