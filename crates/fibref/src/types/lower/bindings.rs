//! The bindings of `let` and `loop` (syntax §3.3, §3.18): `(pat expr)`,
//! or `(x: T expr)`, whose annotation types `x` as annotations do
//! everywhere (§1.5, types §2.4).

use crate::syntax::{Form, FormKind};

use crate::types::ast::{BindingId, PatKind, Pattern};
use crate::types::error::{TResult, TypeError};

use super::decl::annotation_name;
use super::scope::Lowerer;
use super::typeform::type_ann;

/// One binding as written.
pub struct RawBinding<'f> {
    /// The pattern (for `x: T`, the symbol `x`).
    pub pat: Form,
    /// The annotation `T`, if any.
    pub ann: Option<&'f Form>,
    /// The initialiser.
    pub init: &'f Form,
}

/// The `((pat expr)..)` or `((x: T expr)..)` of a `let` or `loop`.
pub fn bindings_of<'f>(items: &'f [Form], form: &Form) -> TResult<Vec<RawBinding<'f>>> {
    let head = items[0].as_sym().unwrap_or("let");
    let (Some(pairs), true) = (items.get(1).and_then(Form::as_list), items.len() >= 3) else {
        return Err(TypeError::resolve(
            &form.pos,
            format!("{head} needs bindings and a body"),
        ));
    };
    pairs.iter().map(binding).collect()
}

fn binding(p: &Form) -> TResult<RawBinding<'_>> {
    match p.as_list() {
        Some([name, t, init]) => match annotation_name(name) {
            Some(n) => Ok(RawBinding {
                pat: Form::new(FormKind::Sym(n.to_string()), name.pos.clone()),
                ann: Some(t),
                init,
            }),
            None => Err(malformed(p)),
        },
        Some([pat, init]) => Ok(RawBinding {
            pat: pat.clone(),
            ann: None,
            init,
        }),
        _ => Err(malformed(p)),
    }
}

fn malformed(p: &Form) -> TypeError {
    TypeError::resolve(
        &p.pos,
        "a binding is (pattern expression) or (name: type expression)",
    )
}

impl Lowerer<'_> {
    /// Records the annotation `ann` of the `let` binding `pat`, which is
    /// a variable when annotated (`_: T` is a pattern, not a name).
    pub fn annotate(&mut self, pat: &Pattern, ann: Option<&Form>) -> TResult<()> {
        let Some(t) = ann else {
            return Ok(());
        };
        match pat.kind {
            PatKind::Bind(b) => self.annotate_binding(b, Some(t)),
            _ => Err(TypeError::resolve(
                &pat.pos,
                "only a variable can be annotated in a binding",
            )),
        }
    }

    /// Records the annotation `ann` of the binding `b`.
    pub fn annotate_binding(&mut self, b: BindingId, ann: Option<&Form>) -> TResult<()> {
        if let Some(t) = ann {
            let a = type_ann(self.g, self.m, t, false)?;
            self.g.bindings[b.0 as usize].ann = Some(a);
        }
        Ok(())
    }
}
