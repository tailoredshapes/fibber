use super::*;

fn shape(src: &str) -> String {
    fn show(n: &Node, src: &str, out: &mut String) {
        match n.kind {
            Kind::List | Kind::Vector | Kind::Map => {
                out.push(match n.kind {
                    Kind::List => '(',
                    Kind::Vector => '[',
                    _ => '{',
                });
                for (i, k) in n.kids.iter().enumerate() {
                    if i > 0 {
                        out.push(' ');
                    }
                    show(k, src, out);
                }
                out.push(match n.kind {
                    Kind::List => ')',
                    Kind::Vector => ']',
                    _ => '}',
                });
            }
            Kind::Prefix => {
                out.push_str("P<");
                show(&n.kids[0], src, out);
                out.push('>');
            }
            _ => out.push_str(n.text(src)),
        }
    }
    let mut out = String::new();
    for (i, n) in parse(src).unwrap().iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        show(n, src, &mut out);
    }
    out
}

#[test]
fn nesting_and_spans() {
    let src = "(a [b {c d}] e)";
    let forms = parse(src).unwrap();
    assert_eq!(forms.len(), 1);
    let list = &forms[0];
    assert_eq!((list.start, list.end), (0, src.len()));
    assert_eq!(list.kids.len(), 3);
    assert_eq!(list.kids[1].text(src), "[b {c d}]");
    assert_eq!(list.kids[1].kids[1].kind, Kind::Map);
    assert_eq!(list.head(src), Some("a"));
}

#[test]
fn a_string_hides_what_is_inside_it() {
    assert_eq!(shape(r#"(f "a ) ; (\" b" x)"#), r#"(f "a ) ; (\" b" x)"#);
    let src = r#"(f "a ) ; (\" b" x)"#;
    assert_eq!(parse(src).unwrap()[0].kids.len(), 3);
}

#[test]
fn a_comment_hides_what_is_inside_it() {
    let src = "(f ; (not a form\n x) ; and ) this\n y";
    assert_eq!(shape(src), "(f x) y");
}

#[test]
fn character_literals_may_be_delimiters() {
    assert_eq!(shape(r"(f \( \) \; \\ \a)"), r"(f \( \) \; \\ \a)");
    assert_eq!(shape(r"(f \newline \space x)"), r"(f \newline \space x)");
    assert_eq!(shape(r#"(f \" x)"#), r#"(f \" x)"#);
    let src = r"(f \é x)";
    let forms = parse(src).unwrap();
    assert_eq!(forms[0].kids[1].text(src), r"\é");
}

#[test]
fn a_discarded_form_is_not_in_the_tree() {
    assert_eq!(shape("(a #_(b c) d)"), "(a d)");
    assert_eq!(shape("(a #_ #_ x y d)"), "(a d)");
    assert_eq!(shape("(a #_b)"), "(a)");
    assert_eq!(shape("#_(whole) (kept)"), "(kept)");
}

#[test]
fn prefixes_take_the_next_form() {
    assert_eq!(
        shape("'(a b) @x `(c ~d ~@e)"),
        "P<(a b)> P<x> P<(c P<d> P<e>)>"
    );
    assert_eq!(shape("' x"), "P<x>");
}

#[test]
fn a_comma_is_space() {
    assert_eq!(shape("[1, 2 , 3]"), "[1 2 3]");
    // a comma is space even when it touches a form (E14)
    assert_eq!(shape("[1 ,3]"), "[1 3]");
    assert_eq!(shape("(a ~b)"), "(a P<b>)");
    assert_eq!(shape("(a ,~b)"), "(a P<b>)");
    assert_eq!(shape("(a b,)"), "(a b)");
}

#[test]
fn a_symbol_stops_where_the_alphabet_stops() {
    assert_eq!(shape("(a@b c'd)"), "(a P<b> c P<d>)");
    assert_eq!(
        shape("(fib.seq/map x: -1 :k 2.5f32 &x ->)"),
        "(fib.seq/map x: -1 :k 2.5f32 &x ->)"
    );
}

#[test]
fn unbalanced_text_is_an_error_with_an_offset() {
    assert_eq!(parse("(a b").unwrap_err().offset, 0);
    assert_eq!(parse("a )").unwrap_err().offset, 2);
    assert_eq!(parse("(a ]").unwrap_err().offset, 3);
    assert!(parse("\"open").is_err());
    assert!(parse("(a #_)").is_err());
    assert!(parse("\\").is_err());
    assert!(parse("@").is_err());
}

#[test]
fn nesting_is_limited_like_the_reader() {
    let deep = format!("{}{}", "(".repeat(2000), ")".repeat(2000));
    assert_eq!(parse(&deep).unwrap_err().message, "forms nest too deeply");
    let fine = format!("{}{}", "(".repeat(100), ")".repeat(100));
    assert!(parse(&fine).is_ok());
}

#[test]
fn lines_count_from_one() {
    assert_eq!(line_of("a\nb\nc", 0), 1);
    assert_eq!(line_of("a\nb\nc", 2), 2);
    assert_eq!(line_of("a\nb\nc", 4), 3);
    assert_eq!(line_of("a", 99), 1);
}

#[test]
fn a_header_comment_and_a_multi_byte_symbol_are_fine() {
    let src = ";; spec: é\n(ns m) (defun é () 1)";
    let forms = parse(src).unwrap();
    assert_eq!(forms.len(), 2);
    assert_eq!(forms[1].kids[1].text(src), "é");
}
