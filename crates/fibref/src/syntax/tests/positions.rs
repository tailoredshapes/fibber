//! Source positions on every form (§1.3; representation on `Pos`).

use crate::syntax::{read_all, Form};

fn span(src: &str, f: &Form) -> String {
    src[f.pos.start..f.pos.end].to_string()
}

fn lc(f: &Form) -> (usize, usize) {
    (f.pos.line, f.pos.col)
}

#[test]
fn line_column_and_byte_span() {
    let src = ";; hi\n(defun f (x)\n  [1 \"é\" x])";
    let forms = read_all(src, "a.fib").expect("reads");
    let top = &forms[0];
    assert_eq!(&*top.pos.file, "a.fib");
    assert_eq!(lc(top), (2, 1));
    assert_eq!(span(src, top), "(defun f (x)\n  [1 \"é\" x])");
    let items = top.as_list().expect("list");
    assert_eq!(
        (lc(&items[1]), span(src, &items[1]).as_str()),
        ((2, 8), "f")
    );
    assert_eq!(
        (lc(&items[2]), span(src, &items[2]).as_str()),
        ((2, 10), "(x)")
    );
    let crate::syntax::FormKind::Vec(v) = &items[3].kind else {
        panic!("vec")
    };
    assert_eq!(lc(&items[3]), (3, 3));
    assert_eq!(lc(&v[1]), (3, 6));
    assert_eq!(span(src, &v[1]), "\"é\"");
    // Columns count characters: é is two bytes, one column.
    assert_eq!(lc(&v[2]), (3, 10));
    assert_eq!(span(src, &v[2]), "x");
}

#[test]
fn prefix_forms_span_prefix_and_operand() {
    let src = "  @(. p c)";
    let f = &read_all(src, "t").expect("reads")[0];
    assert_eq!(span(src, f), "@(. p c)");
    assert_eq!(lc(f), (1, 3));
    let items = f.as_list().expect("list");
    assert_eq!(span(src, &items[0]), "@");
    assert_eq!(items[0].as_sym(), Some("deref"));
    assert_eq!(span(src, &items[1]), "(. p c)");
    let src = ",@xs";
    let f = &read_all(src, "t").expect("reads")[0];
    assert_eq!(span(src, &f.as_list().expect("list")[0]), ",@");
    let src = "' \n x";
    let f = &read_all(src, "t").expect("reads")[0];
    assert_eq!(span(src, f), src);
}

#[test]
fn crlf_and_tabs() {
    let src = "a\r\n\tb\r\n  c";
    let forms = read_all(src, "t").expect("reads");
    let got: Vec<_> = forms.iter().map(lc).collect();
    assert_eq!(got, vec![(1, 1), (2, 2), (3, 3)]);
    // A lone CR is whitespace but not a line end.
    let forms = read_all("a\rb", "t").expect("reads");
    assert_eq!(lc(&forms[1]), (1, 3));
}

#[test]
fn bom_is_skipped_but_counted_in_bytes() {
    let src = "\u{FEFF}(a)";
    let forms = read_all(src, "t").expect("reads");
    assert_eq!(lc(&forms[0]), (1, 1));
    assert_eq!(forms[0].pos.start, 3);
    assert_eq!(span(src, &forms[0]), "(a)");
}
