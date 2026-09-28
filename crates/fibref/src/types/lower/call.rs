//! Lowering calls (syntax §2, §3.13): `&x` arguments, the builtins
//! whose operands are special (`deref` and `set!`, whose operand may be
//! an `&` parameter name; the variadic `concat`), the primitive forms
//! whose operand is a field name, a protocol or a type (§4.3), and the
//! `unsafe`-only names (§3.15).

use crate::syntax::{Form, FormKind};

use crate::types::ast::{Arg, BindingKind, ConvOp, Expr, ExprKind, GlobalRef, IntConv, Place};
use crate::types::builtins::{builtin_index, BUILTINS, CONVERSIONS};
use crate::types::error::{ErrorKind, TResult, TypeError};

use super::scope::Lowerer;
use super::typeform::{proto_ref, type_ann};

/// What a call's head names, when it matters for lowering.
enum Special {
    Deref,
    Set,
    Concat,
    SetField,
    Dyn,
    Convert(ConvOp),
}

/// Whether `(dyn P :send e)`: the keyword right after the protocol.
fn dyn_send(items: &[Form]) -> bool {
    matches!(items.get(2).map(|f| &f.kind), Some(FormKind::Kw(k)) if k == "send")
}

fn conv_op(name: &str) -> Option<ConvOp> {
    Some(match name {
        "trunc" => ConvOp::IntToInt(IntConv::Trunc),
        "zext" => ConvOp::IntToInt(IntConv::Zext),
        "sext" => ConvOp::IntToInt(IntConv::Sext),
        "fptrunc" | "fpext" => ConvOp::FloatToFloat,
        "fptosi" => ConvOp::FloatToInt { signed: true },
        "fptoui" => ConvOp::FloatToInt { signed: false },
        "sitofp" => ConvOp::IntToFloat { signed: true },
        "uitofp" => ConvOp::IntToFloat { signed: false },
        _ => return None,
    })
}

impl Lowerer<'_> {
    /// Whether `name` is a primitive form (§4.3) not shadowed locally.
    pub fn is_primitive_form(&self, name: &str) -> bool {
        (name == "set-field!" || name == "dyn" || CONVERSIONS.contains(&name))
            && !self.is_local(name)
    }

    /// The error for a primitive form used as a value.
    pub fn primitive_value(&self, name: &str, form: &Form) -> TypeError {
        if name == "set-field!" {
            let msg = "function with & parameters is not a value";
            return TypeError::new(ErrorKind::AmpFunctionValue, &form.pos, msg);
        }
        TypeError::resolve(
            &form.pos,
            format!("{name} is a primitive form, not a value"),
        )
    }

    /// Rejects an `unsafe`-only name outside `unsafe` (§3.15).
    pub fn check_unsafe(&self, r: GlobalRef, name: &str, form: &Form) -> TResult<()> {
        let only = match r {
            GlobalRef::Extern(_) => true,
            GlobalRef::Builtin(b) => BUILTINS.get(b.0 as usize).is_some_and(|s| s.unsafe_only),
            _ => false,
        };
        if only && self.unsafe_depth == 0 {
            let msg = format!("{name} may only be used inside unsafe");
            return Err(TypeError::other(&form.pos, msg));
        }
        Ok(())
    }

    fn special(&self, name: &str) -> Option<Special> {
        if self.is_primitive_form(name) {
            return Some(match name {
                "set-field!" => Special::SetField,
                "dyn" => Special::Dyn,
                _ => Special::Convert(conv_op(name)?),
            });
        }
        if self.is_local(name) {
            return None;
        }
        let r = self.g.value(self.m, name)?;
        let builtin = |n: &str| {
            builtin_index(n).map(|i| GlobalRef::Builtin(crate::types::ast::BuiltinId(i as u32)))
        };
        if Some(r) == builtin("set!") {
            return Some(Special::Set);
        }
        if Some(r) == builtin("concat") {
            return Some(Special::Concat);
        }
        match (r, self.g.deref_proto) {
            (GlobalRef::Method(p, 0), Some(d)) if p == d => Some(Special::Deref),
            _ => None,
        }
    }

    /// A list form that is not a core form.
    pub fn call(&mut self, items: &[Form], form: &Form) -> TResult<Expr> {
        if let Some(name) = items[0].as_sym() {
            if let Some(s) = self.special(name) {
                return self.special_call(s, items, form);
            }
        }
        let head = self.expr(&items[0], false)?;
        let mut args = Vec::new();
        for a in &items[1..] {
            args.push(self.arg(a)?);
        }
        Ok(self.mk(&form.pos, ExprKind::Call(Box::new(head), args)))
    }

    /// One argument: `(& x)` or an expression.
    fn arg(&mut self, a: &Form) -> TResult<Arg> {
        match amp_operand(a) {
            Some(x) => Ok(Arg::Amp(self.amp_variable(x, a)?, a.pos.clone())),
            None => Ok(Arg::Expr(self.expr(a, false)?)),
        }
    }

    /// The variable of an `&x` argument: a local binding (§2.14).
    fn amp_variable(&mut self, x: &str, a: &Form) -> TResult<crate::types::ast::BindingId> {
        match self.lookup(x) {
            Some(b) => Ok(b),
            None if self.g.value(self.m, x).is_some() => Err(TypeError::new(
                ErrorKind::AmpArgument,
                &a.pos,
                "& argument must be a cell variable",
            )),
            None => Err(TypeError::resolve(&a.pos, format!("unbound name {x}"))),
        }
    }

    /// The target of `deref`/`set!`: an `&` parameter's name or an
    /// expression.
    fn place(&mut self, f: &Form) -> TResult<Place> {
        if let Some(name) = f.as_sym() {
            if let Some(b) = self.lookup_amp(name) {
                return Ok(Place::Amp(b));
            }
        }
        Ok(Place::Expr(Box::new(self.expr(f, false)?)))
    }

    fn lookup_amp(&mut self, name: &str) -> Option<crate::types::ast::BindingId> {
        if !self.is_local(name) {
            return None;
        }
        let b = self.lookup(name)?;
        (self.kind(b) == BindingKind::AmpParam).then_some(b)
    }

    fn special_call(&mut self, s: Special, items: &[Form], form: &Form) -> TResult<Expr> {
        let pos = &form.pos;
        let name = items[0].as_sym().unwrap_or("");
        let arity = |n: usize| {
            if items.len() == n + 1 {
                Ok(())
            } else {
                Err(TypeError::other(
                    pos,
                    format!("{name} takes {n} operand(s), got {}", items.len() - 1),
                ))
            }
        };
        let kind = match s {
            Special::Deref => {
                arity(1)?;
                let text = items[1].to_string();
                ExprKind::Deref(self.place(&items[1])?, text)
            }
            Special::Set => {
                arity(2)?;
                let target = self.place(&items[1])?;
                ExprKind::Set(target, Box::new(self.expr(&items[2], false)?))
            }
            Special::Concat => ExprKind::Concat(
                items[1..]
                    .iter()
                    .map(|a| self.expr(a, false))
                    .collect::<TResult<_>>()?,
            ),
            Special::SetField => {
                arity(3)?;
                self.set_field(items)?
            }
            Special::Dyn => {
                let send = dyn_send(items);
                arity(if send { 3 } else { 2 })?;
                let (p, dets) = proto_ref(self.g, self.m, &items[1], false)?;
                let e = self.expr(items.last().unwrap_or(&items[0]), false)?;
                ExprKind::Dyn(p, dets, send, Box::new(e))
            }
            Special::Convert(op) => {
                arity(2)?;
                self.convert(op, items)?
            }
        };
        Ok(self.mk(pos, kind))
    }

    /// `(set-field! &x f e)` (§2.13).
    fn set_field(&mut self, items: &[Form]) -> TResult<ExprKind> {
        let Some(x) = amp_operand(&items[1]) else {
            let msg = "& argument must be a cell variable";
            return Err(TypeError::new(ErrorKind::AmpArgument, &items[1].pos, msg));
        };
        let b = self.amp_variable(x, &items[1])?;
        let Some(field) = items[2].as_sym() else {
            return Err(TypeError::resolve(
                &items[2].pos,
                "a field name is a symbol",
            ));
        };
        let value = self.expr(&items[3], false)?;
        Ok(ExprKind::SetField(b, field.to_string(), Box::new(value)))
    }

    /// A conversion `(op T e)` (§2.12).
    fn convert(&mut self, op: ConvOp, items: &[Form]) -> TResult<ExprKind> {
        let target = match type_ann(self.g, self.m, &items[1], false)? {
            crate::types::ast::TypeAnn::Scalar(s) => s,
            _ => {
                return Err(TypeError::other(
                    &items[1].pos,
                    "a conversion's target is a scalar type",
                ))
            }
        };
        let wants_int = matches!(op, ConvOp::IntToInt(_) | ConvOp::FloatToInt { .. });
        let ok = if wants_int {
            target.is_int()
        } else {
            target.is_float()
        };
        if !ok {
            let name = items[0].as_sym().unwrap_or("");
            let msg = format!("{name} cannot convert to {}", target.name());
            return Err(TypeError::other(&items[1].pos, msg));
        }
        Ok(ExprKind::Convert(
            op,
            target,
            Box::new(self.expr(&items[2], false)?),
        ))
    }
}

/// `x` when `form` is `(& x)`.
pub fn amp_operand(form: &Form) -> Option<&str> {
    match &form.kind {
        FormKind::List(items) => match items.as_slice() {
            [h, x] if h.as_sym() == Some("&") => x.as_sym(),
            _ => None,
        },
        _ => None,
    }
}
