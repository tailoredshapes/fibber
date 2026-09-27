//! Type schemes `σ ::= ∀ā ς̄. C̄ ⇒ T` (spec/types.md §1.8).

use super::ty::{subst_colour, Colour, Pred, Ty};

/// A colour constraint kept in a scheme (§5.4 step 4): between
/// quantified colour variables, or the symbolic `ς ⊒ send-of(T)` of a
/// capture whose type is a quantified variable.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ColourBound {
    /// `κ₁ ⊑ κ₂`.
    Flow(Colour, Colour),
    /// `ς ⊒ Caps{T}`: `ς` is `local` unless `Send T`.
    Caps(Colour, Ty),
}

/// A closed type scheme. Quantified variables are `Ty::Gen(0..n_vars)`
/// and `Colour::Gen(0..n_colours)` in `ty`, `preds` and
/// `colour_bounds`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scheme {
    /// How many quantified type variables.
    pub n_vars: u32,
    /// How many quantified colour variables.
    pub n_colours: u32,
    /// A display name for each quantified type variable.
    pub var_names: Vec<String>,
    /// The context: protocol bounds, `Send` and `Object` bounds. For a
    /// protocol method the first is the dispatch constraint `(P s d..)`.
    pub preds: Vec<Pred>,
    /// The remaining colour constraints.
    pub colour_bounds: Vec<ColourBound>,
    /// The type: a function type for functions, constructors and
    /// methods; the value's type for a field-less variant or a `def`.
    pub ty: Ty,
    /// For a function: which parameters are `&` (then `ty`'s parameter
    /// is the value type `T` of the `(& T)` position, and the function
    /// is not a value). Empty for a value.
    pub amps: Vec<bool>,
    /// For a function: the parameter names, for messages.
    pub params: Vec<String>,
}

impl Scheme {
    /// A monomorphic scheme with no context.
    pub fn mono(ty: Ty) -> Scheme {
        Scheme {
            n_vars: 0,
            n_colours: 0,
            var_names: Vec::new(),
            preds: Vec::new(),
            colour_bounds: Vec::new(),
            ty,
            amps: Vec::new(),
            params: Vec::new(),
        }
    }

    /// Whether some parameter is `&`, so the function is not a value.
    pub fn has_amp(&self) -> bool {
        self.amps.iter().any(|a| *a)
    }

    /// The scheme's parts with the quantified variables replaced by
    /// `tys` and `colours`.
    pub fn instantiate_with(&self, tys: &[Ty], colours: &[Colour]) -> Instance {
        let sub = |t: &Ty| t.subst_gen(tys, colours);
        Instance {
            ty: sub(&self.ty),
            preds: self
                .preds
                .iter()
                .map(|p| p.map_tys(&mut |t| sub(t)))
                .collect(),
            colour_bounds: self
                .colour_bounds
                .iter()
                .map(|b| match b {
                    ColourBound::Flow(a, c) => {
                        ColourBound::Flow(subst_colour(*a, colours), subst_colour(*c, colours))
                    }
                    ColourBound::Caps(k, t) => ColourBound::Caps(subst_colour(*k, colours), sub(t)),
                })
                .collect(),
        }
    }
}

/// A scheme instantiated at particular types.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Instance {
    /// The type.
    pub ty: Ty,
    /// The context to discharge.
    pub preds: Vec<Pred>,
    /// The colour constraints to solve.
    pub colour_bounds: Vec<ColourBound>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ty::ProtoId;

    #[test]
    fn instantiation_substitutes_everywhere() {
        let s = Scheme {
            n_vars: 1,
            n_colours: 1,
            var_names: vec!["a".into()],
            preds: vec![Pred::Proto(ProtoId(0), vec![Ty::Gen(0)])],
            colour_bounds: vec![ColourBound::Caps(Colour::Gen(0), Ty::Gen(0))],
            ty: Ty::Fn(Colour::Gen(0), vec![Ty::Gen(0)], Box::new(Ty::Gen(0))),
            amps: vec![false],
            params: vec!["x".into()],
        };
        let i = s.instantiate_with(&[Ty::i64()], &[Colour::Send]);
        assert_eq!(i.preds, vec![Pred::Proto(ProtoId(0), vec![Ty::i64()])]);
        assert_eq!(
            i.colour_bounds,
            vec![ColourBound::Caps(Colour::Send, Ty::i64())]
        );
        assert!(!s.has_amp());
    }
}
