//! The builtins, constructors and prelude functions the generator calls,
//! as the model evaluates them.

use std::cell::RefCell;
use std::rc::Rc;

use super::eval::{deref, trap, unsupported, Machine, Res};
use super::value::{TaskState, V};

impl Machine<'_> {
    /// Calls the builtin, constructor or prelude function `h`.
    pub(super) fn builtin(&mut self, h: &str, a: Vec<V>) -> Res {
        if let Some(r) = super::nums::op(h, &a).or_else(|| super::arrays::op(h, &a)) {
            return r;
        }
        match (h, a.as_slice()) {
            ("+" | "-" | "*" | "rem", [V::Int(x), V::Int(y)]) => arith(h, *x, *y),
            ("=" | "<" | "<=" | ">" | ">=" | "!=", [x, y]) => Ok(V::Bool(compare(h, x, y)?)),
            ("not", [V::Bool(b)]) => Ok(V::Bool(!b)),
            ("str-len", [V::Str(s)]) => Ok(V::Int(s.len() as i64)),
            ("str-concat", [V::Str(x), V::Str(y)]) => Ok(V::str(&format!("{x}{y}"))),
            ("inc1", [V::Int(x)]) => arith("+", *x, 1),
            ("some", [x]) => Ok(V::Opt(Some(Rc::new(x.clone())))),
            ("cons", [x, V::List(t)]) => Ok(V::List(Rc::new(prepend(x, t)))),
            ("list", items) => Ok(V::List(Rc::new(items.to_vec()))),
            (
                "Pt" | "Wrap" | "Holder" | "Box" | "Circle" | "Rect" | "Named" | "Hook" | "Ver"
                | "Mid" | "High",
                _,
            ) => Ok(V::data(h, a)),
            ("box", [x]) => Ok(V::data("Box", vec![x.clone()])),
            ("unbox", [V::Data(_, f)]) => Ok(f[0].clone()),
            ("cell", [x]) => Ok(V::Cell(Rc::new(RefCell::new(x.clone())))),
            ("atom", [x]) => Ok(V::Atom(Rc::new(RefCell::new(x.clone())))),
            ("weak", [x]) => Ok(V::Weak(Some(Rc::new(x.clone())))),
            ("reset!", [V::Atom(c), x]) => {
                *c.borrow_mut() = x.clone();
                Ok(V::Unit)
            }
            ("swap!", [V::Atom(c), f]) => {
                let old = c.borrow().clone();
                let new = self.apply(f, vec![old])?;
                *c.borrow_mut() = new.clone();
                Ok(new)
            }
            ("spawn", [f]) => Ok(task(TaskState::Thunk(f.clone()))),
            ("yield", []) => Ok(task(TaskState::Done(V::Unit))),
            ("join" | "block-on", [t]) => self.force(t),
            ("deref", [x]) => deref(x),
            _ => self.collection(h, a),
        }
    }

    fn collection(&mut self, h: &str, a: Vec<V>) -> Res {
        match (h, a.as_slice()) {
            ("count", [V::Vector(xs) | V::List(xs)]) => Ok(V::Int(xs.len() as i64)),
            ("count", [V::Str(s)]) => Ok(V::Int(s.len() as i64)),
            ("nth", [V::Vector(xs) | V::List(xs), V::Int(i)]) => usize::try_from(*i)
                .ok()
                .and_then(|i| xs.get(i).cloned())
                .ok_or_else(|| trap("nth: index out of range")),
            ("conj", [V::Vector(xs), x]) => {
                let mut ys = (**xs).clone();
                ys.push(x.clone());
                Ok(V::Vector(Rc::new(ys)))
            }
            ("conj", [V::List(xs), x]) => Ok(V::List(Rc::new(prepend(x, xs)))),
            ("push!", [V::Cell(c), x]) => {
                let v = self.collection("conj", vec![c.borrow().clone(), x.clone()])?;
                *c.borrow_mut() = v;
                Ok(V::Unit)
            }
            ("range", [V::Int(n)]) => Ok(V::Vector(Rc::new((0..*n).map(V::Int).collect()))),
            ("sum-vec", [V::Vector(xs)]) => xs
                .iter()
                .try_fold(0i64, |s, x| match x {
                    V::Int(n) => Ok(s.wrapping_add(*n)),
                    v => Err(unsupported(format!("sum-vec over {v:?}"))),
                })
                .map(V::Int),
            ("map" | "pmap", [f, V::Vector(xs)]) => {
                let mut out = Vec::new();
                for x in xs.iter() {
                    out.push(self.apply(f, vec![x.clone()])?);
                }
                Ok(V::Vector(Rc::new(out)))
            }
            ("for-each", [V::Vector(xs), f]) => {
                for x in xs.iter() {
                    self.apply(f, vec![x.clone()])?;
                }
                Ok(V::Unit)
            }
            ("nil?", [V::Opt(o)]) => Ok(V::Bool(o.is_none())),
            ("some?", [V::Opt(o)]) => Ok(V::Bool(o.is_some())),
            ("unwrap-or", [V::Opt(o), d]) => Ok(o.as_deref().cloned().unwrap_or_else(|| d.clone())),
            _ => self.method(h, a),
        }
    }
}

fn task(s: TaskState) -> V {
    V::Task(Rc::new(RefCell::new(s)))
}

fn prepend(x: &V, xs: &[V]) -> Vec<V> {
    let mut ys = Vec::with_capacity(xs.len() + 1);
    ys.push(x.clone());
    ys.extend(xs.iter().cloned());
    ys
}

/// Integer arithmetic as Rust's (types §2.12, Decided 2026-09-27): a
/// result that does not fit `i64` traps, as does `rem` by zero and
/// `rem` of the minimum by -1. The messages are the interpreter's.
fn arith(h: &str, x: i64, y: i64) -> Res {
    let r = match h {
        "+" => x.checked_add(y),
        "-" => x.checked_sub(y),
        "*" => x.checked_mul(y),
        _ if y == 0 => return Err(trap("integer rem by zero")),
        _ => x.checked_rem(y),
    };
    match r {
        Some(n) => Ok(V::Int(n)),
        None => Err(trap(&format!("integer overflow in {h} at i64"))),
    }
}

fn compare(h: &str, x: &V, y: &V) -> Result<bool, super::eval::Stop> {
    let ord = match (x, y) {
        (V::Int(a), V::Int(b)) => a.cmp(b),
        (V::Bool(a), V::Bool(b)) => a.cmp(b),
        (V::Str(a), V::Str(b)) => a.cmp(b),
        (V::Data(..), V::Data(..)) => super::arrays::derived_cmp(x, y)
            .ok_or_else(|| unsupported(format!("compare {x:?} {y:?}")))?,
        _ => return Err(unsupported(format!("compare {x:?} {y:?}"))),
    };
    Ok(match h {
        "=" => ord.is_eq(),
        "!=" => ord.is_ne(),
        "<" => ord.is_lt(),
        "<=" => ord.is_le(),
        ">" => ord.is_gt(),
        _ => ord.is_ge(),
    })
}
