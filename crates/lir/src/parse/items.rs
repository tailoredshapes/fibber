//! Top-level forms (spec/lir.md §4).

use super::expr::parse_expr;
use super::ty::{parse_cc, parse_params, parse_ret, parse_type};
use super::{arity, atom, label, valid_name};
use crate::ast::{Block, Declare, Function, GlobalDef, Item, Module, StructDef};
use crate::diag::{err, Pos, Result};
use crate::sexp::Sexp;
use crate::types::FnType;

const TOP: &str = "expected a top-level form (define, declare, defstruct, global, constant)";

/// Parse every top-level form.
pub fn module(forms: &[Sexp]) -> Result<Module> {
    let items = forms.iter().map(item).collect::<Result<_>>()?;
    Ok(Module { items })
}

fn item(form: &Sexp) -> Result<Item> {
    let Sexp::List(items, pos) = form else {
        return err(form.pos(), format!("{TOP}, found {}", form.describe()));
    };
    let head = items.first().and_then(Sexp::atom).unwrap_or("");
    let rest = if items.is_empty() { items } else { &items[1..] };
    match head {
        "defstruct" => defstruct(rest, *pos),
        "global" => global(rest, *pos, false),
        "constant" => global(rest, *pos, true),
        "declare" => declare(rest, *pos),
        "define" => define(rest, *pos),
        _ => err(*pos, format!("{TOP}, found {}", form.describe())),
    }
}

fn global_name(s: &Sexp) -> Result<String> {
    valid_name(atom(s, "a name")?, s.pos())
}

fn defstruct(rest: &[Sexp], pos: Pos) -> Result<Item> {
    arity("defstruct", rest, 2, pos)?;
    let name = global_name(&rest[0])?;
    let Sexp::List(fields, _) = &rest[1] else {
        return err(rest[1].pos(), "expected a field type list");
    };
    let fields = fields.iter().map(parse_type).collect::<Result<_>>()?;
    Ok(Item::Struct(StructDef { name, fields, pos }))
}

fn global(rest: &[Sexp], pos: Pos, constant: bool) -> Result<Item> {
    let op = if constant { "constant" } else { "global" };
    arity(op, rest, 3, pos)?;
    Ok(Item::Global(GlobalDef {
        name: global_name(&rest[0])?,
        ty: parse_type(&rest[1])?,
        init: parse_expr(&rest[2])?,
        constant,
        pos,
    }))
}

fn declare(rest: &[Sexp], pos: Pos) -> Result<Item> {
    let (cc, k) = parse_cc(rest);
    let rest = &rest[k..];
    arity("declare", rest, 3, pos)?;
    let name = global_name(&rest[0])?;
    let ret = parse_ret(&rest[1])?;
    let (params, varargs) = parse_params(&rest[2])?;
    let ty = FnType {
        cc,
        ret,
        params,
        varargs,
    };
    Ok(Item::Declare(Declare { name, ty, pos }))
}

fn define(rest: &[Sexp], pos: Pos) -> Result<Item> {
    let (cc, k) = parse_cc(rest);
    let rest = &rest[k..];
    if rest.len() < 2 {
        return err(pos, "define expects (NAME R), parameters and blocks");
    }
    let (name, ret) = match &rest[0] {
        Sexp::List(h, p) if h.len() == 2 => (global_name(&h[0])?, (parse_ret(&h[1])?, *p)),
        other => return err(other.pos(), "expected (NAME R) after define"),
    };
    let (ptypes, params) = params(&rest[1])?;
    let blocks = rest[2..].iter().map(block).collect::<Result<_>>()?;
    let ty = FnType {
        cc,
        ret: ret.0,
        params: ptypes,
        varargs: false,
    };
    Ok(Item::Define(Function {
        name,
        ty,
        params,
        blocks,
        pos,
    }))
}

type Params = (Vec<crate::types::Type>, Vec<(String, Pos)>);

fn params(s: &Sexp) -> Result<Params> {
    let Sexp::List(items, _) = s else {
        return err(s.pos(), "expected a parameter list ((T name)..)");
    };
    let mut tys = Vec::new();
    let mut names = Vec::new();
    for p in items {
        match p {
            Sexp::List(tn, pp) if tn.len() == 2 => {
                tys.push(parse_type(&tn[0])?);
                names.push((valid_name(atom(&tn[1], "a parameter name")?, *pp)?, *pp));
            }
            other => return err(other.pos(), "a parameter must be (T name)"),
        }
    }
    Ok((tys, names))
}

fn block(s: &Sexp) -> Result<Block> {
    match s {
        Sexp::List(items, pos) if items.first().and_then(Sexp::atom) == Some("block") => {
            if items.len() < 2 {
                return err(*pos, "block needs a label");
            }
            let label = label(&items[1])?;
            let body = items[2..].iter().map(parse_expr).collect::<Result<_>>()?;
            Ok(Block {
                label,
                body,
                pos: *pos,
            })
        }
        other => err(
            other.pos(),
            format!("expected (block LABEL instr..), found {}", other.describe()),
        ),
    }
}
