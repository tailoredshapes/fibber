//! Default methods (types §4.1, owner's decision of 2026-09-28): the
//! `impl` method a protocol's default stands for in an `impl` that
//! omits it.

use crate::syntax::{Form, FormKind};

use crate::types::decls::MethodDef;

/// `(m (self x..) body..)` from the default `(m (self q.. x: T q..) ->
/// R body..)`: the parameters by name, the body as written. The result
/// annotation is dropped; the impl body is checked against the
/// signature anyway (§2.7).
pub fn method_form(md: &MethodDef, default: &Form) -> Form {
    let items = default.as_list().unwrap_or(&[]);
    let pos = &default.pos;
    let sym = |s: &str| Form::new(FormKind::Sym(s.to_string()), pos.clone());
    let params = md.params.iter().map(|p| sym(&p.name)).collect();
    let mut out = vec![
        sym(&md.name),
        Form::new(FormKind::List(params), pos.clone()),
    ];
    out.extend(items.iter().skip(4).cloned());
    Form::new(FormKind::List(out), pos.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::read_all;
    use crate::types::decls::MethodParam;
    use crate::types::scheme::Scheme;
    use crate::types::ty::Ty;

    #[test]
    fn the_default_becomes_a_method_body_with_the_parameter_names() {
        let f = &read_all("(!= (self y: Self :borrow) -> bool (not (= self y)))", "t")
            .expect("reads")[0];
        let p = |n: &str| MethodParam {
            name: n.into(),
            borrow: false,
            owned: false,
        };
        let md = MethodDef {
            name: "!=".into(),
            params: vec![p("self"), p("y")],
            scheme: Scheme::mono(Ty::unit()),
            self_elsewhere: true,
            default: Some(f.clone()),
            pos: f.pos.clone(),
        };
        assert_eq!(
            method_form(&md, f).to_string(),
            "(!= (self y) (not (= self y)))"
        );
    }
}
