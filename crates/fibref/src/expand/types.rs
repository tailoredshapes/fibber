//! The expander's table of the structs and enums seen so far (§3.16:
//! "a `defstruct` is registered with the expander before the next form
//! is expanded"), which reflection and `derive` read.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::syntax::{read_all, Form, FormKind, Pos};

use super::build::{list, malformed, sym};
use super::error::ExpandError;

/// A struct as `defstruct` declared it (§3.7).
#[derive(Clone, Debug, PartialEq)]
pub struct StructInfo {
    /// The struct's name.
    pub name: String,
    /// The type parameters in order, those synthesised from unannotated
    /// fields appended in field order (§3.7).
    pub params: Vec<Form>,
    /// Field names and type forms in order; an unannotated field's type
    /// is the `Sym` of its synthesised parameter.
    pub fields: Vec<(Form, Form)>,
}

/// One variant of an enum (§3.9).
#[derive(Clone, Debug, PartialEq)]
pub struct VariantInfo {
    /// The variant's name (`nil` for `Option`'s empty variant).
    pub name: String,
    /// Each field's name, if written, and type form, in order.
    pub fields: Vec<(Option<String>, Form)>,
}

/// An enum as `defenum` declared it (§3.9).
#[derive(Clone, Debug, PartialEq)]
pub struct EnumInfo {
    /// The enum's name.
    pub name: String,
    /// The type parameters in order.
    pub params: Vec<Form>,
    /// The variants in declaration order.
    pub variants: Vec<VariantInfo>,
}

/// Structs and enums by name.
#[derive(Clone, Debug, Default)]
pub struct TypeTable {
    pub(crate) structs: HashMap<String, StructInfo>,
    pub(crate) enums: HashMap<String, EnumInfo>,
    /// The `:private` structs and enums of the module being expanded.
    pub(crate) private: HashSet<String>,
    /// The private structs and enums of modules expanded before it,
    /// which reflection and `derive` do not see (syntax §5).
    pub(crate) hidden: HashSet<String>,
}

/// §3.16's built-in `Form` enum, in the spelling §3.16 gives it.
const FORM_ENUM: &str = "(defenum Form
  (Sym name: str) (Kw name: str)
  (Int v: i64 width: keyword) (Flt v: f64 width: keyword)
  (Str v: str) (Chr v: char) (Bool v: bool) (Nil)
  (List items: (Vec Form)) (Vec items: (Vec Form)) (Map items: (Vec Form)))";

impl TypeTable {
    /// The table holding the built-in enums `Option` and `Form` (§3.9,
    /// §3.16: reflection works "for the built-in `Option` and `Form`").
    pub fn with_builtins() -> Self {
        let mut table = TypeTable::default();
        let pos = builtin_pos();
        let a = sym("a", &pos);
        let some = VariantInfo {
            name: "some".to_string(),
            fields: vec![(Some("v".to_string()), a.clone())],
        };
        let none = VariantInfo {
            name: "nil".to_string(),
            fields: Vec::new(),
        };
        table.enums.insert(
            "Option".to_string(),
            EnumInfo {
                name: "Option".to_string(),
                params: vec![a],
                variants: vec![none, some],
            },
        );
        // FORM_ENUM is a constant checked by the unit test
        // `builtin_form_enum_parses`, so neither step can fail.
        let forms = read_all(FORM_ENUM, "<builtin>").expect("FORM_ENUM reads");
        let info = parse_defenum(&forms[0]).expect("FORM_ENUM parses");
        table.enums.insert(info.name.clone(), info);
        table
    }

    /// Records a `defstruct` form, replacing any earlier one of the name.
    pub(crate) fn add_struct(&mut self, form: &Form) -> Result<(), ExpandError> {
        let info = parse_defstruct(form)?;
        self.hidden.remove(&info.name);
        self.structs.insert(info.name.clone(), info);
        Ok(())
    }

    /// Records a `defenum` form, replacing any earlier one of the name.
    pub(crate) fn add_enum(&mut self, form: &Form) -> Result<(), ExpandError> {
        let info = parse_defenum(form)?;
        self.hidden.remove(&info.name);
        self.enums.insert(info.name.clone(), info);
        Ok(())
    }

    /// Ends a module: its private types are hidden from the next.
    pub(crate) fn end_module(&mut self) {
        let private = std::mem::take(&mut self.private);
        self.hidden.extend(private);
    }
}

/// The position of built-in definitions.
fn builtin_pos() -> Pos {
    Pos {
        file: Arc::from("<builtin>"),
        line: 0,
        col: 0,
        start: 0,
        end: 0,
    }
}

/// The name and declared parameters of a `Name` or `(Name tvar+)` head.
fn parse_head(
    head: &str,
    form: Option<&Form>,
    pos: &Pos,
) -> Result<(String, Vec<Form>), ExpandError> {
    let Some(form) = form else {
        return Err(malformed(head, "missing name", pos));
    };
    if let Some(name) = form.as_sym() {
        return Ok((name.to_string(), Vec::new()));
    }
    let items = form.as_list().unwrap_or(&[]);
    let all_syms = items.iter().all(|f| f.as_sym().is_some());
    match items.first().and_then(Form::as_sym) {
        Some(name) if items.len() > 1 && all_syms => Ok((name.to_string(), items[1..].to_vec())),
        _ => Err(malformed(
            head,
            "name must be a symbol or (Name tvar+)",
            &form.pos,
        )),
    }
}

/// `x:` as `Some("x")`.
fn annotation_name(form: &Form) -> Option<&str> {
    let name = form.as_sym()?.strip_suffix(':')?;
    (!name.is_empty()).then_some(name)
}

/// Parses `(defstruct Name (field+))` or `(defstruct (Name tvar+)
/// (field+))`, `field ::= sym: type | sym` (§3.7).
pub(crate) fn parse_defstruct(form: &Form) -> Result<StructInfo, ExpandError> {
    let items = form.as_list().unwrap_or(&[]);
    if items.len() != 3 {
        return Err(malformed(
            "defstruct",
            "expected (defstruct Name (field+))",
            &form.pos,
        ));
    }
    let (name, mut params) = parse_head("defstruct", items.get(1), &form.pos)?;
    let fields_form = &items[2];
    let decl = match fields_form.as_list() {
        Some(d) if !d.is_empty() => d,
        _ => {
            return Err(malformed(
                "defstruct",
                "expected a non-empty field list",
                &fields_form.pos,
            ))
        }
    };
    let mut fields = Vec::new();
    let mut i = 0;
    while i < decl.len() {
        let f = &decl[i];
        if let Some(fname) = annotation_name(f) {
            let Some(ty) = decl.get(i + 1) else {
                return Err(malformed("defstruct", "annotation without a type", &f.pos));
            };
            fields.push((sym(fname, &f.pos), ty.clone()));
            i += 2;
        } else if f.as_sym().is_some() {
            params.push(f.clone());
            fields.push((f.clone(), f.clone()));
            i += 1;
        } else {
            return Err(malformed(
                "defstruct",
                "a field is sym: type or sym",
                &f.pos,
            ));
        }
    }
    Ok(StructInfo {
        name,
        params,
        fields,
    })
}

/// Parses `(defenum Name variant+)` or `(defenum (Name tvar+)
/// variant+)` (§3.9).
pub(crate) fn parse_defenum(form: &Form) -> Result<EnumInfo, ExpandError> {
    let items = form.as_list().unwrap_or(&[]);
    if items.len() < 3 {
        return Err(malformed(
            "defenum",
            "expected (defenum Name variant+)",
            &form.pos,
        ));
    }
    let (name, params) = parse_head("defenum", items.get(1), &form.pos)?;
    let variants = items[2..]
        .iter()
        .map(parse_variant)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(EnumInfo {
        name,
        params,
        variants,
    })
}

/// `variant ::= (Variant) | (Variant field+) | Variant`, `field ::= sym:
/// type | type`.
fn parse_variant(form: &Form) -> Result<VariantInfo, ExpandError> {
    if let Some(name) = form.as_sym() {
        let name = name.to_string();
        return Ok(VariantInfo {
            name,
            fields: Vec::new(),
        });
    }
    let items = form.as_list().unwrap_or(&[]);
    let Some(name) = items.first().and_then(Form::as_sym) else {
        return Err(malformed(
            "defenum",
            "a variant is (Variant field*) or Variant",
            &form.pos,
        ));
    };
    let mut fields = Vec::new();
    let mut i = 1;
    while i < items.len() {
        let f = &items[i];
        if let Some(fname) = annotation_name(f) {
            let Some(ty) = items.get(i + 1) else {
                return Err(malformed("defenum", "annotation without a type", &f.pos));
            };
            fields.push((Some(fname.to_string()), ty.clone()));
            i += 2;
        } else {
            fields.push((None, f.clone()));
            i += 1;
        }
    }
    let name = name.to_string();
    Ok(VariantInfo { name, fields })
}

impl VariantInfo {
    /// The variant as §3.16's `enum-variants` returns it: `(List [(Sym
    /// "Variant") type*])`.
    pub fn as_form(&self, pos: &Pos) -> Form {
        let mut items = vec![sym(&self.name, pos)];
        items.extend(self.fields.iter().map(|(_, t)| t.clone()));
        list(items, pos)
    }
}

/// Whether the symbol `name` occurs anywhere in the type form `ty`.
pub(crate) fn mentions(ty: &Form, name: &str) -> bool {
    match &ty.kind {
        FormKind::Sym(s) => s == name,
        FormKind::List(items) | FormKind::Vec(items) | FormKind::Map(items) => {
            items.iter().any(|t| mentions(t, name))
        }
        _ => false,
    }
}
