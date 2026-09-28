//! `:private` after a definition's name (syntax §5). The expander takes
//! the marker off while it parses and registers the definition, puts it
//! back for name resolution, and hides a private struct or enum from
//! the reflection of every later module (§3.16).

use crate::syntax::{Form, FormKind};

/// The index of the `:private` marker a definition form may carry: right
/// after the name, or after a `def`'s `name: type`.
pub fn marker_index(form: &Form) -> Option<usize> {
    let items = form.as_list()?;
    let annotated = |f: &Form| f.as_sym().is_some_and(|s| s.len() > 1 && s.ends_with(':'));
    let at = match items.first().and_then(Form::as_sym)? {
        "def" if items.get(1).is_some_and(annotated) => 3,
        "def" | "defun" | "defmacro" | "defstruct" | "defenum" | "defprotocol" | "extern" => 2,
        _ => return None,
    };
    matches!(&items.get(at)?.kind, FormKind::Kw(k) if k == "private").then_some(at)
}

/// `form` without the item at `at`, and that item.
pub(crate) fn take_marker(form: Form, at: usize) -> (Form, Option<Form>) {
    match form.kind {
        FormKind::List(mut items) if at < items.len() => {
            let marker = items.remove(at);
            (Form::new(FormKind::List(items), form.pos), Some(marker))
        }
        kind => (Form::new(kind, form.pos), None),
    }
}

/// `form` with `marker` put back at `at`.
pub(crate) fn put_marker(form: Form, at: usize, marker: Form) -> Form {
    match form.kind {
        FormKind::List(mut items) if at <= items.len() => {
            items.insert(at, marker);
            Form::new(FormKind::List(items), form.pos)
        }
        kind => Form::new(kind, form.pos),
    }
}

/// The name of a `defstruct`/`defenum` form: `Name` or `(Name a..)`.
pub(crate) fn type_name(form: &Form) -> Option<&str> {
    let n = form.as_list()?.get(1)?;
    n.as_sym()
        .or_else(|| n.as_list().and_then(|l| l.first()).and_then(Form::as_sym))
}
