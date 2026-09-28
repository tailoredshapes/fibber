//! One compiled macro-time module in the JIT: its C entry points
//! (`abi.rs`) as function pointers, and the conversions between the
//! expander's forms and the module's `Form` objects.

use fibref::syntax::{FltWidth, Form, FormKind, IntWidth, Pos};
use lair::Jit;

use super::abi::{VARIANTS, WIDTHS};
use crate::compile::Unsupported;

type P = *const u8;

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
    /// The keyword ids of the width suffixes: i8 i16 i32 i64 f32 f64.
    pub widths: [i64; 6],
    /// Every keyword the module interned, by id.
    pub keywords: Vec<String>,
}

impl Fns {
    /// Looks the module's functions up in the JIT.
    pub fn lookup(jit: &mut Jit, widths: [i64; 6], k: usize) -> Result<Fns, Unsupported> {
        let n = |s: &str| format!("fibm.{s}.{k}");
        // SAFETY: each name is defined by `abi.rs` with exactly the
        // signature transmuted to here; the JIT outlives the runner.
        unsafe {
            let e = |m: lair::Error| Unsupported(format!("macro module: {m}"));
            Ok(Fns {
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
                widths,
                keywords: Vec::new(),
            })
        }
    }

    fn tag_of(name: &str) -> i32 {
        VARIANTS.iter().position(|v| *v == name).unwrap_or(7) as i32
    }

    fn str_object(&self, s: &str) -> P {
        (self.str_new)(s.as_ptr(), s.len() as i64)
    }

    fn width_id(&self, suffix: &str) -> i64 {
        WIDTHS.iter().position(|w| *w == suffix).map_or_else(
            || {
                self.keywords
                    .iter()
                    .position(|k| k == suffix)
                    .map_or(-1, |i| i as i64)
            },
            |i| self.widths[i],
        )
    }

    /// A form as a `Form` object of the module.
    pub fn to_object(&self, f: &Form) -> P {
        let null = std::ptr::null();
        match &f.kind {
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
            FormKind::List(items) => (self.form)(Self::tag_of("List"), 0, 0, self.items(items)),
            FormKind::Vec(items) => (self.form)(Self::tag_of("Vec"), 0, 0, self.items(items)),
            FormKind::Map(items) => (self.form)(Self::tag_of("Map"), 0, 0, self.items(items)),
        }
    }

    /// A `(Vec Form)` object of these forms.
    pub fn items(&self, items: &[Form]) -> P {
        let objs: Vec<P> = items.iter().map(|f| self.to_object(f)).collect();
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

    /// A `Form` object of the module as a form.
    pub fn to_form(&self, o: P, pos: &Pos) -> Result<Form, String> {
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
            "List" => FormKind::List(self.read_items((self.f0_ptr)(o), pos)?),
            "Vec" => FormKind::Vec(self.read_items((self.f0_ptr)(o), pos)?),
            "Map" => FormKind::Map(self.read_items((self.f0_ptr)(o), pos)?),
            _ => FormKind::Nil,
        };
        Ok(Form::new(kind, pos.clone()))
    }

    /// The suffix a keyword id names; an id the module did not intern
    /// for a width is reported by its number, as the interpreter names
    /// an unknown keyword.
    fn width_name(&self, id: i64) -> String {
        match self.widths.iter().position(|w| *w == id) {
            Some(i) => WIDTHS[i].to_string(),
            None => self
                .keywords
                .get(id as usize)
                .cloned()
                .unwrap_or_else(|| format!("#{id}")),
        }
    }

    fn read_items(&self, v: P, pos: &Pos) -> Result<Vec<Form>, String> {
        let n = (self.vec_len)(v);
        (0..n)
            .map(|i| self.to_form((self.vec_elem)(v, i), pos))
            .collect()
    }
}
