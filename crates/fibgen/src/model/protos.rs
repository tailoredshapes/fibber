//! Protocol method calls as the model evaluates them: dispatch on the
//! receiver's constructor (a `dyn` value is the value itself, types
//! §4.5), the `impl`'s own body when it gives the method, else the
//! protocol's default written here independently of the printed one:
//! `bonus` is `(+ (score self) k)` and `tier` is `(+ (rank self)
//! (score self))` (types §4.1).

use std::collections::HashMap;

use crate::ast::{Method, Program};
use crate::ty::Ty;

use super::eval::{unsupported, Env, Machine, Res};
use super::value::V;

/// The head of an implementing type, as its values' constructors name it.
pub fn ty_key(t: &Ty) -> &'static str {
    match t {
        Ty::Pt => "Pt",
        Ty::Wrap => "Wrap",
        Ty::Holder => "Holder",
        Ty::Shape => "Shape",
        Ty::Hook(_) => "Hook",
        _ => "?",
    }
}

/// The head of the type of `v`.
fn value_key(v: &V) -> Option<&'static str> {
    let V::Data(name, _) = v else { return None };
    Some(match &**name {
        "Circle" | "Rect" | "Named" => "Shape",
        "Pt" => "Pt",
        "Wrap" => "Wrap",
        "Holder" => "Holder",
        "Hook" => "Hook",
        _ => return None,
    })
}

/// Every method an `impl` of `p` gives, by (method, type head).
pub fn impl_table(p: &Program) -> HashMap<(&str, &'static str), &Method> {
    let mut t = HashMap::new();
    for i in &p.impls {
        for m in &i.methods {
            t.insert((m.name.as_str(), ty_key(&i.target)), m);
        }
    }
    t
}

impl Machine<'_> {
    /// The method `h` called with `args`, the receiver first.
    pub(super) fn method(&mut self, h: &str, args: Vec<V>) -> Res {
        let key = args.first().and_then(value_key);
        let (Some(key), true) = (key, ["score", "bonus", "rank", "tier"].contains(&h)) else {
            return Err(unsupported(format!("call {h} on {args:?}")));
        };
        if let Some(m) = self.impls.get(&(h, key)).copied() {
            if matches!(h, "bonus" | "tier") {
                self.trace.insert("run: overriding method ran");
            }
            let mut env = Env::default().bind("self", args[0].clone());
            for ((p, _), v) in m.params.iter().zip(args[1..].iter()) {
                env = env.bind(p, v.clone());
            }
            return Ok(self.eval(&m.body, &env)?);
        }
        self.default_method(h, args)
    }

    /// The protocol's default for `h`, or unsupported for a method
    /// without one that the `impl` did not give.
    fn default_method(&mut self, h: &str, args: Vec<V>) -> Res {
        let recv = args[0].clone();
        let (first, second) = match (h, args.get(1)) {
            ("bonus", Some(k)) => (self.method("score", vec![recv])?, k.clone()),
            ("tier", None) => {
                let r = self.method("rank", vec![recv.clone()])?;
                (r, self.method("score", vec![recv])?)
            }
            _ => return Err(unsupported(format!("no method {h} on {recv:?}"))),
        };
        self.trace.insert("run: default method ran");
        self.builtin("+", vec![first, second])
    }
}
