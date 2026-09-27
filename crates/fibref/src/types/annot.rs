//! Turning a resolved annotation ([`TypeAnn`]) into a [`Ty`].
//!
//! What a type variable and an omitted colour mean depends on where the
//! annotation is (§1.4, §3.1): the caller supplies that through an
//! [`AnnEnv`].

use std::collections::HashMap;

use crate::syntax::Pos;

use super::ast::TypeAnn;
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
        TypeAnn::Dyn(p, args) => Ty::Con(Con::Dyn(*p), anns(args, env, pos)?),
        TypeAnn::Fn(k, ps, r) => {
            let k = match k {
                Some(k) => *k,
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
}

/// The parameters of a struct or enum: only they may occur, as
/// `Gen(i)`; an omitted colour is `local` (§1.4: on a field).
pub struct ParamEnv<'a> {
    /// The definition's parameters.
    pub params: &'a [String],
    /// The definition's name, for messages.
    pub owner: &'a str,
}

impl AnnEnv for ParamEnv<'_> {
    fn var(&mut self, name: &str, pos: &Pos) -> TResult<Ty> {
        match self.params.iter().position(|p| p == name) {
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
            owner: "S",
        };
        let bad = ann_to_ty(&TypeAnn::Var("b".into()), &mut env, &pos());
        assert!(bad.is_err());
        let f = TypeAnn::Fn(None, vec![], Box::new(TypeAnn::Scalar(Scalar::I64)));
        let t = ann_to_ty(&f, &mut env, &pos()).expect("converts");
        assert_eq!(t, Ty::Fn(Colour::Local, vec![], Box::new(Ty::i64())));
    }
}
