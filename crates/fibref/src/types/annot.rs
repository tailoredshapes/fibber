//! Turning a resolved annotation ([`TypeAnn`]) into a [`Ty`].
//!
//! What a type variable and an omitted colour mean depends on where the
//! annotation is (§1.4, §3.1): the caller supplies that through an
//! [`AnnEnv`].

use std::collections::HashMap;

use crate::syntax::Pos;

use super::ast::{ColourAnn, TypeAnn};
use super::error::{TResult, TypeError};
use super::ty::{Colour, Con, Ty};

/// What variables, `Self` and omitted colours mean in one annotation.
pub trait AnnEnv {
    /// The type of the variable `name`.
    fn var(&mut self, name: &str, pos: &Pos) -> TResult<Ty>;
    /// The type `Self` stands for.
    fn self_ty(&mut self, pos: &Pos) -> TResult<Ty> {
        Err(TypeError::resolve(
            pos,
            "Self is only allowed in a protocol",
        ))
    }
    /// The colour of a function type whose colour is omitted.
    fn colour(&mut self) -> Colour;
    /// The colour a named colour variable or parameter stands for
    /// (§1.3).
    fn named_colour(&mut self, name: &str, pos: &Pos) -> TResult<Colour> {
        Err(TypeError::resolve(
            pos,
            format!("colour variable {name} is not allowed here"),
        ))
    }
    /// The argument of a nominal type at a colour parameter written as
    /// the name `name`, when this environment gives it as a type (an
    /// `impl` head's colour variable is `Gen(i)` there, §1.3); `None`
    /// for the colour of [`AnnEnv::named_colour`].
    fn colour_param_arg(&mut self, _name: &str, _pos: &Pos) -> Option<TResult<Ty>> {
        None
    }
}

/// A written colour under `env`.
fn colour_of(c: &ColourAnn, env: &mut dyn AnnEnv, pos: &Pos) -> TResult<Colour> {
    match c {
        ColourAnn::Fixed(k) => Ok(*k),
        ColourAnn::Named(n) => env.named_colour(n, pos),
    }
}

/// Converts `ann` under `env`.
pub fn ann_to_ty(ann: &TypeAnn, env: &mut dyn AnnEnv, pos: &Pos) -> TResult<Ty> {
    Ok(match ann {
        TypeAnn::Var(name) => env.var(name, pos)?,
        TypeAnn::SelfTy => env.self_ty(pos)?,
        TypeAnn::Scalar(s) => Ty::scalar(*s),
        TypeAnn::Str => Ty::str(),
        TypeAnn::Builtin(c, arg) => Ty::Con(*c, vec![ann_to_ty(arg, env, pos)?]),
        TypeAnn::Nominal(id, args) => Ty::Con(Con::Nominal(*id), anns(args, env, pos)?),
        TypeAnn::Dyn(p, args, send) => Ty::Con(Con::Dyn(*p, *send), anns(args, env, pos)?),
        TypeAnn::ColourArg(ColourAnn::Named(n)) => match env.colour_param_arg(n, pos) {
            Some(t) => t?,
            None => Ty::colour_arg(env.named_colour(n, pos)?),
        },
        TypeAnn::ColourArg(c) => Ty::colour_arg(colour_of(c, env, pos)?),
        TypeAnn::Fn(k, ps, r) => {
            let k = match k {
                Some(k) => colour_of(k, env, pos)?,
                None => env.colour(),
            };
            let ps = anns(ps, env, pos)?;
            Ty::Fn(k, ps, Box::new(ann_to_ty(r, env, pos)?))
        }
    })
}

fn anns(args: &[TypeAnn], env: &mut dyn AnnEnv, pos: &Pos) -> TResult<Vec<Ty>> {
    args.iter().map(|a| ann_to_ty(a, env, pos)).collect()
}

/// Variables are quantified: each new name gets the next `Gen`, and each
/// omitted colour the next `Colour::Gen` (protocol signatures and
/// builtin signatures). `Self` is `Gen(0)` when `self_gen` is set.
#[derive(Debug, Default)]
pub struct GenEnv {
    /// The variables named so far, in order of `Gen` index.
    pub names: Vec<String>,
    /// How many colour variables were made.
    pub colours: u32,
    /// Whether `Self` is allowed (as `Gen(0)`).
    pub self_gen: bool,
    /// Whether omitted colours are quantified variables (else `local`).
    pub local_colours: bool,
    /// Named colour variables and their `Colour::Gen` index.
    pub colour_names: Vec<(String, u32)>,
}

impl GenEnv {
    /// An environment whose first variables are `names`.
    pub fn with_names(names: Vec<String>) -> Self {
        GenEnv {
            names,
            ..GenEnv::default()
        }
    }
}

impl AnnEnv for GenEnv {
    fn var(&mut self, name: &str, _: &Pos) -> TResult<Ty> {
        if let Some(i) = self.names.iter().position(|n| n == name) {
            return Ok(Ty::Gen(i as u32));
        }
        self.names.push(name.to_string());
        Ok(Ty::Gen(self.names.len() as u32 - 1))
    }

    fn self_ty(&mut self, pos: &Pos) -> TResult<Ty> {
        if self.self_gen {
            Ok(Ty::Gen(0))
        } else {
            Err(TypeError::resolve(
                pos,
                "Self is only allowed in a protocol",
            ))
        }
    }

    fn colour(&mut self) -> Colour {
        if self.local_colours {
            return Colour::Local;
        }
        self.colours += 1;
        Colour::Gen(self.colours - 1)
    }

    fn named_colour(&mut self, name: &str, _: &Pos) -> TResult<Colour> {
        if let Some((_, i)) = self.colour_names.iter().find(|(n, _)| n == name) {
            return Ok(Colour::Gen(*i));
        }
        self.colours += 1;
        self.colour_names.push((name.to_string(), self.colours - 1));
        Ok(Colour::Gen(self.colours - 1))
    }
}

/// The parameters of a struct or enum: only they may occur, as
/// `Gen(i)`; an omitted colour is `local` (§1.4: on a field).
pub struct ParamEnv<'a> {
    /// The definition's parameters.
    pub params: &'a [String],
    /// Which of them are colour parameters (§1.3); may be empty.
    pub colours: &'a [bool],
    /// The definition's name, for messages.
    pub owner: &'a str,
}

impl AnnEnv for ParamEnv<'_> {
    fn var(&mut self, name: &str, pos: &Pos) -> TResult<Ty> {
        match self.params.iter().position(|p| p == name) {
            Some(i) if self.colours.get(i) == Some(&true) => Err(TypeError::resolve(
                pos,
                format!("{name} is a colour parameter of {}, not a type", self.owner),
            )),
            Some(i) => Ok(Ty::Gen(i as u32)),
            None => Err(TypeError::resolve(
                pos,
                format!("type variable {name} is not a parameter of {}", self.owner),
            )),
        }
    }

    fn colour(&mut self) -> Colour {
        Colour::Local
    }

    fn named_colour(&mut self, name: &str, pos: &Pos) -> TResult<Colour> {
        match self.params.iter().position(|p| p == name) {
            Some(i) if self.colours.get(i) == Some(&true) => Ok(Colour::Gen(i as u32)),
            _ => Err(TypeError::resolve(
                pos,
                format!("{name} is not a colour parameter of {}", self.owner),
            )),
        }
    }
}

/// Named variables map to rigid variables (an annotated `defun`, an
/// impl body): `map` gives each name its rigid index, `names` the name
/// of each index; new names are added. Omitted colours come from
/// `colour`.
pub struct RigidEnv<'a> {
    /// Name to rigid index.
    pub map: &'a mut HashMap<String, u32>,
    /// Rigid index to name.
    pub names: &'a mut Vec<String>,
    /// Makes the colour of an omitted annotation.
    pub colour: &'a mut dyn FnMut() -> Colour,
    /// Named colour variables: each name one colour variable of the
    /// definition (§1.3).
    pub colour_names: &'a mut HashMap<String, Colour>,
}

impl AnnEnv for RigidEnv<'_> {
    fn var(&mut self, name: &str, _: &Pos) -> TResult<Ty> {
        if let Some(i) = self.map.get(name) {
            return Ok(Ty::Rigid(*i));
        }
        let i = self.names.len() as u32;
        self.names.push(name.to_string());
        self.map.insert(name.to_string(), i);
        Ok(Ty::Rigid(i))
    }

    fn colour(&mut self) -> Colour {
        (self.colour)()
    }

    fn named_colour(&mut self, name: &str, _: &Pos) -> TResult<Colour> {
        if let Some(k) = self.colour_names.get(name) {
            return Ok(*k);
        }
        let k = (self.colour)();
        self.colour_names.insert(name.to_string(), k);
        Ok(k)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ty::Scalar;
    use std::sync::Arc;

    fn pos() -> Pos {
        Pos {
            file: Arc::from("t"),
            line: 1,
            col: 1,
            start: 0,
            end: 0,
        }
    }

    #[test]
    fn gen_env_numbers_variables_and_colours() {
        let ann = TypeAnn::Fn(
            None,
            vec![TypeAnn::Var("a".into()), TypeAnn::Var("b".into())],
            Box::new(TypeAnn::Var("a".into())),
        );
        let mut env = GenEnv::default();
        let t = ann_to_ty(&ann, &mut env, &pos()).expect("converts");
        assert_eq!(
            t,
            Ty::Fn(
                Colour::Gen(0),
                vec![Ty::Gen(0), Ty::Gen(1)],
                Box::new(Ty::Gen(0))
            )
        );
        assert_eq!(env.names, vec!["a", "b"]);
    }

    #[test]
    fn param_env_rejects_foreign_variables_and_defaults_local() {
        let params = vec!["a".to_string()];
        let mut env = ParamEnv {
            params: &params,
            colours: &[],
            owner: "S",
        };
        let bad = ann_to_ty(&TypeAnn::Var("b".into()), &mut env, &pos());
        assert!(bad.is_err());
        let f = TypeAnn::Fn(None, vec![], Box::new(TypeAnn::Scalar(Scalar::I64)));
        let t = ann_to_ty(&f, &mut env, &pos()).expect("converts");
        assert_eq!(t, Ty::Fn(Colour::Local, vec![], Box::new(Ty::i64())));
    }
}
