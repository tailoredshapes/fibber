//! `:private` (syntax §5): the marker after a definition's name, taken
//! off before the definition is lowered and recorded in the module's
//! names, so that name resolution in every other module skips it.

use crate::syntax::{Form, FormKind};

use crate::types::decls::{Globals, ModuleId, Shape, Space};

use super::decl::annotation_name;

pub use crate::expand::marker_index;

/// The forms with their markers taken off, and the private ones among
/// them (without their markers).
pub fn strip_private(forms: &[Form]) -> (Vec<Form>, Vec<Form>) {
    let mut out = Vec::with_capacity(forms.len());
    let mut private = Vec::new();
    for f in forms {
        match (marker_index(f), &f.kind) {
            (Some(at), FormKind::List(items)) => {
                let mut items = items.clone();
                items.remove(at);
                let stripped = Form::new(FormKind::List(items), f.pos.clone());
                private.push(stripped.clone());
                out.push(stripped);
            }
            _ => out.push(f.clone()),
        }
    }
    (out, private)
}

/// The name a definition head gives: `Name` or `(Name a..)`.
fn head_name(f: &Form) -> Option<&str> {
    f.as_sym()
        .or_else(|| f.as_list().and_then(|l| l.first()).and_then(Form::as_sym))
}

/// Records the definitions of `private` (declared in `m`) as private:
/// their names, a struct's constructor, an enum's variants and a
/// protocol's methods (syntax §5).
pub fn mark_private(g: &mut Globals, m: ModuleId, private: &[Form]) {
    for f in private {
        let items = f.as_list().unwrap_or(&[]);
        let kind = items.first().and_then(Form::as_sym).unwrap_or("");
        let Some(name) = items
            .get(1)
            .and_then(|n| annotation_name(n).or_else(|| head_name(n)))
        else {
            continue;
        };
        let name = name.to_string();
        let mut values = Vec::new();
        match kind {
            "defstruct" | "defenum" => {
                g.names_mut(m).make_private(Space::Type, &name);
                values.extend(ctor_names(g, m, &name));
            }
            "defprotocol" => {
                g.names_mut(m).make_private(Space::Proto, &name);
                if let Some(p) = g.names(m).protos.get(&name) {
                    values.extend(g.proto(*p).methods.iter().map(|md| md.name.clone()));
                }
            }
            "defmacro" => {}
            _ => values.push(name),
        }
        for v in values {
            g.names_mut(m).make_private(Space::Value, &v);
        }
    }
}

/// The constructor names of the struct or enum `name` of module `m`.
fn ctor_names(g: &Globals, m: ModuleId, name: &str) -> Vec<String> {
    let Some(id) = g.names(m).types.get(name) else {
        return Vec::new();
    };
    match &g.ty(*id).shape {
        Shape::Struct(_) => vec![name.to_string()],
        Shape::Enum(vs) => vs.iter().map(|v| v.name.clone()).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::read_all;

    fn forms(src: &str) -> Vec<Form> {
        read_all(src, "t").expect("reads")
    }

    #[test]
    fn the_marker_follows_the_name_or_a_defs_annotation() {
        let fs = forms(
            "(defun f :private (x) x) (def g: i64 :private 1) (def h :private 2)
             (defstruct P :private (x: i64)) (defun k (x) :private)",
        );
        let at: Vec<Option<usize>> = fs.iter().map(marker_index).collect();
        assert_eq!(at, vec![Some(2), Some(3), Some(2), Some(2), None]);
    }

    #[test]
    fn stripping_takes_the_marker_off_and_lists_the_private_forms() {
        let (out, private) = strip_private(&forms("(defun f :private (x) x) (defun g (x) x)"));
        assert_eq!(out[0].to_string(), "(defun f (x) x)");
        assert_eq!(out[1].to_string(), "(defun g (x) x)");
        assert_eq!(private.len(), 1);
        assert_eq!(private[0].to_string(), "(defun f (x) x)");
    }
}
