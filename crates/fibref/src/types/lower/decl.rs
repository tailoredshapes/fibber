//! `defstruct` and `defenum` (syntax §3.7, §3.9): declaring the names,
//! then resolving the field types once every type name of the module is
//! known (types §3.5 step 3).

use crate::syntax::{Form, Pos};

use crate::types::annot::{ann_to_ty, ParamEnv};
use crate::types::ast::GlobalRef;
use crate::types::decls::{FieldDef, Globals, ModuleId, Shape, TypeDef, VariantDef};
use crate::types::error::{TResult, TypeError};
use crate::types::ty::TypeId;

use super::typeform::type_ann;

/// A field before its type is resolved: `None` is the synthesised
/// parameter named after the field (§3.7).
pub struct RawField<'f> {
    pub name: String,
    pub ty: Option<&'f Form>,
}

/// A declared struct or enum whose field types are still forms.
pub struct RawType<'f> {
    pub id: TypeId,
    /// `None` for a struct.
    pub variants: Option<Vec<(String, Vec<RawField<'f>>)>>,
    /// A struct's fields.
    pub fields: Vec<RawField<'f>>,
}

/// Binds `name` to `r` in module `m`, unless `m` already binds it.
pub fn define_value(
    g: &mut Globals,
    m: ModuleId,
    name: &str,
    r: GlobalRef,
    pos: &Pos,
) -> TResult<()> {
    let values = &mut g.names_mut(m).values;
    if values.contains_key(name) {
        return Err(TypeError::resolve(
            pos,
            format!("{name} is already defined"),
        ));
    }
    values.insert(name.to_string(), r);
    Ok(())
}

/// `x:` as `Some("x")`.
pub fn annotation_name(form: &Form) -> Option<&str> {
    let name = form.as_sym()?.strip_suffix(':')?;
    (!name.is_empty()).then_some(name)
}

/// The name and declared parameters of `Name` or `(Name tvar+)`.
fn parse_head(form: &Form) -> TResult<(String, Vec<String>)> {
    if let Some(name) = form.as_sym() {
        return Ok((name.to_string(), Vec::new()));
    }
    let items = form.as_list().unwrap_or(&[]);
    let names: Option<Vec<&str>> = items.iter().map(Form::as_sym).collect();
    match names {
        Some(ns) if ns.len() > 1 => Ok((
            ns[0].to_string(),
            ns[1..].iter().map(|s| s.to_string()).collect(),
        )),
        _ => Err(TypeError::resolve(
            &form.pos,
            "expected Name or (Name tvar+)",
        )),
    }
}

fn new_type(
    g: &mut Globals,
    m: ModuleId,
    name: &str,
    params: Vec<String>,
    pos: &Pos,
) -> TResult<TypeId> {
    if g.names(m).types.contains_key(name) {
        return Err(TypeError::resolve(
            pos,
            format!("type {name} is already defined"),
        ));
    }
    let id = TypeId(g.types.len() as u32);
    g.types.push(TypeDef {
        name: name.to_string(),
        module: m,
        params,
        shape: Shape::Struct(Vec::new()),
        pos: pos.clone(),
    });
    g.names_mut(m).types.insert(name.to_string(), id);
    Ok(id)
}

/// Declares `(defstruct Name (field+))`: the type and its constructor.
pub fn declare_struct<'f>(g: &mut Globals, m: ModuleId, form: &'f Form) -> TResult<RawType<'f>> {
    let items = form.as_list().unwrap_or(&[]);
    let [_, head, decl] = items else {
        return Err(TypeError::resolve(
            &form.pos,
            "expected (defstruct Name (field+))",
        ));
    };
    let (name, mut params) = parse_head(head)?;
    let decl = match decl.as_list() {
        Some(d) if !d.is_empty() => d,
        _ => {
            return Err(TypeError::resolve(
                &decl.pos,
                "expected a non-empty field list",
            ))
        }
    };
    let fields = struct_fields(decl, &mut params)?;
    let id = new_type(g, m, &name, params, &form.pos)?;
    define_value(g, m, &name, GlobalRef::Ctor(id, None), &form.pos)?;
    Ok(RawType {
        id,
        variants: None,
        fields,
    })
}

/// `field ::= sym: type | sym`; an unannotated field adds a parameter
/// named after it to `params` (§3.7).
fn struct_fields<'f>(decl: &'f [Form], params: &mut Vec<String>) -> TResult<Vec<RawField<'f>>> {
    let mut fields = Vec::new();
    let mut i = 0;
    while i < decl.len() {
        if let Some(fname) = annotation_name(&decl[i]) {
            let Some(ty) = decl.get(i + 1) else {
                return Err(TypeError::resolve(
                    &decl[i].pos,
                    "annotation without a type",
                ));
            };
            fields.push(RawField {
                name: fname.to_string(),
                ty: Some(ty),
            });
            i += 2;
        } else if let Some(fname) = decl[i].as_sym() {
            params.push(fname.to_string());
            fields.push(RawField {
                name: fname.to_string(),
                ty: None,
            });
            i += 1;
        } else {
            return Err(TypeError::resolve(
                &decl[i].pos,
                "a field is sym: type or sym",
            ));
        }
    }
    Ok(fields)
}

/// Declares `(defenum Name variant+)`: the type and its variants.
pub fn declare_enum<'f>(g: &mut Globals, m: ModuleId, form: &'f Form) -> TResult<RawType<'f>> {
    let items = form.as_list().unwrap_or(&[]);
    if items.len() < 3 {
        return Err(TypeError::resolve(
            &form.pos,
            "expected (defenum Name variant+)",
        ));
    }
    let (name, params) = parse_head(&items[1])?;
    let variants = items[2..]
        .iter()
        .map(parse_variant)
        .collect::<TResult<Vec<_>>>()?;
    let id = new_type(g, m, &name, params, &form.pos)?;
    for (i, (vname, _)) in variants.iter().enumerate() {
        define_value(g, m, vname, GlobalRef::Ctor(id, Some(i)), &form.pos)?;
    }
    Ok(RawType {
        id,
        variants: Some(variants),
        fields: Vec::new(),
    })
}

/// `variant ::= (Variant) | (Variant field+) | Variant`.
fn parse_variant(form: &Form) -> TResult<(String, Vec<RawField<'_>>)> {
    if let Some(name) = form.as_sym() {
        return Ok((name.to_string(), Vec::new()));
    }
    let items = form.as_list().unwrap_or(&[]);
    let Some(name) = items.first().and_then(Form::as_sym) else {
        return Err(TypeError::resolve(
            &form.pos,
            "a variant is (Variant field*) or Variant",
        ));
    };
    let mut fields = Vec::new();
    let mut i = 1;
    while i < items.len() {
        if let Some(fname) = annotation_name(&items[i]) {
            let Some(ty) = items.get(i + 1) else {
                return Err(TypeError::resolve(
                    &items[i].pos,
                    "annotation without a type",
                ));
            };
            fields.push(RawField {
                name: fname.to_string(),
                ty: Some(ty),
            });
            i += 2;
        } else {
            let fname = fields.len().to_string();
            fields.push(RawField {
                name: fname,
                ty: Some(&items[i]),
            });
            i += 1;
        }
    }
    Ok((name.to_string(), fields))
}

/// Resolves the field types of a declared struct or enum.
pub fn resolve_shape(g: &mut Globals, m: ModuleId, raw: &RawType<'_>) -> TResult<()> {
    let shape = match &raw.variants {
        None => Shape::Struct(resolve_fields(g, m, raw.id, &raw.fields)?),
        Some(vs) => {
            let mut out = Vec::new();
            for (name, fields) in vs {
                let fields = resolve_fields(g, m, raw.id, fields)?;
                out.push(VariantDef {
                    name: name.clone(),
                    fields,
                });
            }
            Shape::Enum(out)
        }
    };
    g.types[raw.id.0 as usize].shape = shape;
    Ok(())
}

fn resolve_fields(
    g: &Globals,
    m: ModuleId,
    id: TypeId,
    raw: &[RawField<'_>],
) -> TResult<Vec<FieldDef>> {
    let def = g.ty(id);
    let mut env = ParamEnv {
        params: &def.params,
        owner: &def.name,
    };
    let mut out = Vec::new();
    for f in raw {
        let ty = match f.ty {
            Some(form) => {
                let ann = type_ann(g, m, form, false)?;
                ann_to_ty(&ann, &mut env, &form.pos)?
            }
            None => {
                let i = def.params.iter().position(|p| *p == f.name).unwrap_or(0);
                crate::types::ty::Ty::Gen(i as u32)
            }
        };
        out.push(FieldDef {
            name: f.name.clone(),
            ty,
        });
    }
    Ok(out)
}
