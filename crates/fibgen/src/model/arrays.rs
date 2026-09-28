//! Arrays (types §2.13) and values with derived `Eq` and `Ord` (syntax
//! §3.16), as the model evaluates them. An array is a `V::Vector`; the
//! primitive `array-set!` takes no copy-in (§2.13: it tests and updates
//! the content of its argument's own cell), so it is evaluated here
//! rather than as a call with an `&` argument.

use std::cmp::Ordering;
use std::rc::Rc;

use crate::ast::Arg;

use super::eval::{trap, unsupported, Env, Machine, Res};
use super::value::V;

/// The array operations.
pub fn op(h: &str, a: &[V]) -> Option<Res> {
    let index = |xs: &Rc<Vec<V>>, i: i64| usize::try_from(i).ok().filter(|i| *i < xs.len());
    let r = match (h, a) {
        ("array", [V::Int(n), x]) => {
            let n = usize::try_from(*n).map_err(|_| trap("array: negative length"));
            n.map(|n| V::Vector(Rc::new(vec![x.clone(); n])))
        }
        ("array-len", [V::Vector(xs)]) => Ok(V::Int(xs.len() as i64)),
        ("array-get", [V::Vector(xs), V::Int(i)]) => index(xs, *i)
            .map(|i| xs[i].clone())
            .ok_or_else(|| trap("array-get: index out of range")),
        ("array-with", [V::Vector(xs), V::Int(i), x]) => match index(xs, *i) {
            Some(i) => {
                let mut ys = (**xs).clone();
                ys[i] = x.clone();
                Ok(V::Vector(Rc::new(ys)))
            }
            None => Err(trap("array-with: index out of range")),
        },
        ("array-copy", [V::Vector(xs), V::Int(i), V::Int(j)]) => {
            match (usize::try_from(*i), usize::try_from(*j)) {
                (Ok(i), Ok(j)) if i <= j && j <= xs.len() => {
                    Ok(V::Vector(Rc::new(xs[i..j].to_vec())))
                }
                _ => Err(trap("array-copy: range out of bounds")),
            }
        }
        _ => return None,
    };
    Some(r)
}

/// The declaration order of the derived enum's variants.
fn variant_index(name: &str) -> usize {
    ["Low", "Mid", "High"]
        .iter()
        .position(|v| *v == name)
        .unwrap_or(0)
}

/// Derived `Ord` of two values: variants in declaration order, then the
/// fields lexicographically.
pub fn derived_cmp(x: &V, y: &V) -> Option<Ordering> {
    match (x, y) {
        (V::Int(a), V::Int(b)) => Some(a.cmp(b)),
        (V::Str(a), V::Str(b)) => Some(a.cmp(b)),
        (V::Data(n, fs), V::Data(m, gs)) => {
            let by_variant = variant_index(n).cmp(&variant_index(m));
            if by_variant != Ordering::Equal {
                return Some(by_variant);
            }
            for (f, g) in fs.iter().zip(gs.iter()) {
                let o = derived_cmp(f, g)?;
                if o != Ordering::Equal {
                    return Some(o);
                }
            }
            Some(Ordering::Equal)
        }
        _ => None,
    }
}

impl Machine<'_> {
    /// `(array-set! &c i x)`: `i` and `x` evaluated, then the array in
    /// `c` replaced by one with slot `i` set.
    pub(super) fn ev_array_set(&mut self, args: &[Arg], env: &Env) -> Res {
        let [Arg::InOut(c), Arg::Val(i), Arg::Val(x)] = args else {
            return Err(unsupported(format!("array-set! {args:?}")));
        };
        let (i, x) = (self.ev(i, env)?, self.ev(x, env)?);
        let Some(V::Cell(cell)) = env.get(c).cloned() else {
            return Err(unsupported(format!("array-set! on {c}")));
        };
        let old = cell.borrow().clone();
        let new = op("array-with", &[old, i, x])
            .unwrap_or_else(|| Err(unsupported("array-set! on a non-array".into())))?;
        *cell.borrow_mut() = new;
        Ok(V::Unit)
    }
}
