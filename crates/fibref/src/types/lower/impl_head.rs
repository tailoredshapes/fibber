//! The head of an `impl` (syntax §3.10; types §1.3, §2.7): a type
//! constructor applied to distinct variables, where a colour parameter
//! of a struct or enum takes a colour variable or a written colour
//! (**Decided**, owner, 2026-09-28). A colour variable of the head is
//! `Gen(i)` in the instance's head, determined arguments and context,
//! like a type variable; the method bodies see it as the rigid colour
//! `Colour::Rigid(i)` ([`body_args`]).

use crate::syntax::{Form, Pos};

use crate::types::annot::AnnEnv;
use crate::types::ast::{ColourAnn, TypeAnn};
use crate::types::decls::{Globals, InstanceDef, ModuleId};
use crate::types::error::{TResult, TypeError};
use crate::types::ty::{Colour, Con, Ty};

use super::typeform::type_ann;

/// An `impl` head, resolved.
pub struct ImplHead {
    /// The head constructor.
    pub con: Con,
    /// The head's variables, `Gen(i)` in order.
    pub vars: Vec<String>,
    /// Per variable, whether it is a colour variable.
    pub colours: Vec<bool>,
    /// `(K ā)`: `Gen(i)` for a variable, `Ty::colour_arg(k)` for a
    /// written colour.
    pub head: Ty,
}

const DISTINCT: &str = "an instance head is a type constructor applied to distinct variables (or, at a colour parameter, a colour)";

/// Resolves the head form of an `impl`.
pub fn impl_head(g: &Globals, m: ModuleId, form: &Form) -> TResult<ImplHead> {
    let bad = |msg: &str| Err(TypeError::other(&form.pos, msg.to_string()));
    let (con, args) = match type_ann(g, m, form, false)? {
        TypeAnn::Var(_) => return bad("an instance head must not be a type variable"),
        TypeAnn::Scalar(s) => (Con::Scalar(s), Vec::new()),
        TypeAnn::Str => (Con::Str, Vec::new()),
        TypeAnn::Builtin(c, a) => (c, vec![*a]),
        TypeAnn::Nominal(id, args) => (Con::Nominal(id), args),
        _ => return bad(DISTINCT),
    };
    let mut out = ImplHead {
        con,
        vars: Vec::new(),
        colours: Vec::new(),
        head: Ty::unit(),
    };
    let mut head_args = Vec::new();
    for a in args {
        let (name, colour) = match a {
            TypeAnn::Var(v) => (v, false),
            TypeAnn::ColourArg(ColourAnn::Named(v)) => (v, true),
            TypeAnn::ColourArg(ColourAnn::Fixed(k)) => {
                head_args.push(Ty::colour_arg(k));
                continue;
            }
            _ => return bad(DISTINCT),
        };
        if out.vars.contains(&name) {
            return bad(DISTINCT);
        }
        head_args.push(Ty::Gen(out.vars.len() as u32));
        out.vars.push(name);
        out.colours.push(colour);
    }
    out.head = Ty::Con(con, head_args);
    Ok(out)
}

/// The variables of an impl head in a determined argument or the
/// `:where` context: a type variable is `Gen(i)`, and so is a colour
/// variable at a colour parameter; a colour variable anywhere else is
/// an error.
pub struct HeadEnv<'a> {
    /// The head's variables.
    pub vars: &'a [String],
    /// Which are colour variables.
    pub colours: &'a [bool],
}

impl AnnEnv for HeadEnv<'_> {
    fn var(&mut self, name: &str, pos: &Pos) -> TResult<Ty> {
        match self.vars.iter().position(|p| p == name) {
            Some(i) if self.colours[i] => Err(TypeError::resolve(
                pos,
                format!("{name} is a colour parameter of the impl head, not a type"),
            )),
            Some(i) => Ok(Ty::Gen(i as u32)),
            None => Err(TypeError::resolve(
                pos,
                format!("type variable {name} is not a parameter of the impl head"),
            )),
        }
    }

    fn colour(&mut self) -> Colour {
        Colour::Local
    }

    fn named_colour(&mut self, name: &str, pos: &Pos) -> TResult<Colour> {
        Err(TypeError::resolve(
            pos,
            format!("{name} may be used here only as the argument of a colour parameter"),
        ))
    }

    fn colour_param_arg(&mut self, name: &str, pos: &Pos) -> Option<TResult<Ty>> {
        let i = self.vars.iter().position(|p| p == name);
        Some(match i {
            Some(i) if self.colours[i] => Ok(Ty::Gen(i as u32)),
            _ => Err(TypeError::resolve(
                pos,
                format!("{name} is not a colour variable of the impl head"),
            )),
        })
    }
}

/// Per variable of `inst`, whether it is a colour variable: it stands
/// at a colour parameter of the head's struct or enum (§1.3).
pub fn colour_vars(g: &Globals, inst: &InstanceDef) -> Vec<bool> {
    let mut out = vec![false; inst.var_names.len()];
    if let Ty::Con(Con::Nominal(id), args) = &inst.head {
        for (i, a) in args.iter().enumerate() {
            if let (true, Ty::Gen(j)) = (g.ty(*id).is_colour(i), a) {
                if let Some(slot) = out.get_mut(*j as usize) {
                    *slot = true;
                }
            }
        }
    }
    out
}

/// What the variables of `inst` are in its method bodies: `Rigid(i)`
/// for a type variable, the rigid colour `Colour::Rigid(i)` as a colour
/// argument for a colour variable (§1.3).
pub fn body_args(g: &Globals, inst: &InstanceDef) -> Vec<Ty> {
    colour_vars(g, inst)
        .into_iter()
        .enumerate()
        .map(|(i, c)| match c {
            true => Ty::colour_arg(Colour::Rigid(i as u32)),
            false => Ty::Rigid(i as u32),
        })
        .collect()
}

/// The colours a head gives its colour parameters, by argument
/// position: `Some(k)` for a written colour, `None` for a variable.
pub fn fixed_colours(g: &Globals, head: &Ty) -> Vec<Option<Colour>> {
    let Ty::Con(Con::Nominal(id), args) = head else {
        return Vec::new();
    };
    args.iter()
        .enumerate()
        .map(|(i, a)| match (g.ty(*id).is_colour(i), a) {
            (true, Ty::Fn(k @ (Colour::Send | Colour::Local), _, _)) => Some(*k),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn head_env_reads_a_colour_variable_only_at_a_colour_parameter() {
        let pos = crate::syntax::Pos {
            file: std::sync::Arc::from("t"),
            line: 1,
            col: 1,
            start: 0,
            end: 0,
        };
        let vars = vec!["a".to_string(), "k".to_string()];
        let mut env = HeadEnv {
            vars: &vars,
            colours: &[false, true],
        };
        assert_eq!(env.var("a", &pos).ok(), Some(Ty::Gen(0)));
        assert!(env.var("k", &pos).is_err());
        assert!(matches!(
            env.colour_param_arg("k", &pos),
            Some(Ok(Ty::Gen(1)))
        ));
        assert!(matches!(env.colour_param_arg("a", &pos), Some(Err(_))));
        assert!(env.named_colour("k", &pos).is_err());
    }
}
