//! `#(..)`, `#{..}` and `7/2` (spec/syntax.md §1.2), and the positions
//! of the forms they synthesise.

use super::{err, one, show};
use crate::syntax::{read_all, Form, ReadErrorKind};

fn span(src: &str, f: &Form) -> String {
    src[f.pos.start..f.pos.end].to_string()
}

#[test]
fn fn_shorthand_reads_as_fn() {
    assert_eq!(show("#(* % 2)"), "(fn (%1) (* %1 2))");
    assert_eq!(show("#(+ %1 %2)"), "(fn (%1 %2) (+ %1 %2))");
    assert_eq!(show("#(f %3)"), "(fn (%1 %2 %3) (f %3))");
    assert_eq!(show("#()"), "(fn () ())");
    assert_eq!(show("#(f 1)"), "(fn () (f 1))");
    assert_eq!(show("#(f [% {:k %2}])"), "(fn (%1 %2) (f [%1 {:k %2}]))");
    assert_eq!(show("(map #(* % 2) xs)"), "(map (fn (%1) (* %1 2)) xs)");
    assert_eq!(show("#(f %1x %0a %)"), "(fn (%1) (f %1x %0a %1))");
    // Outside a `#(`, `%` is an ordinary symbol.
    assert_eq!(show("(f % %1 %&)"), "(f % %1 %&)");
    // The parameters of one `#(` do not reach the next.
    assert_eq!(
        show("#(f %2) #(g %)"),
        "(fn (%1 %2) (f %2)) (fn (%1) (g %1))"
    );
}

#[test]
fn fn_shorthand_errors() {
    assert_eq!(err("#(f #(g %))"), ReadErrorKind::NestedFn);
    assert_eq!(err("#(f %&)"), ReadErrorKind::UnsupportedRest);
    for bad in ["%0", "%01", "%256", "%99999999999999999999"] {
        let src = format!("#(f {bad})");
        assert!(
            matches!(err(&src), ReadErrorKind::BadFnParam { .. }),
            "{src}"
        );
    }
    assert_eq!(show("#(f %255)").matches("%255").count(), 2);
    assert_eq!(err("#(f x"), ReadErrorKind::Unclosed { open: '(' });
    assert!(matches!(
        err("#(f x]"),
        ReadErrorKind::MismatchedClose { .. }
    ));
}

#[test]
fn fn_shorthand_positions() {
    let src = "  #(f % %3 %)";
    let f = &read_all(src, "t").expect("reads")[0];
    assert_eq!(span(src, f), "#(f % %3 %)");
    let items = f.as_list().expect("list");
    assert_eq!(span(src, &items[0]), "#(");
    assert_eq!(items[0].as_sym(), Some("fn"));
    assert_eq!(span(src, &items[1]), "#(");
    let params = items[1].as_list().expect("params");
    assert_eq!(params.len(), 3);
    assert_eq!(span(src, &params[0]), "%");
    assert_eq!(params[0].pos.col, 7);
    assert_eq!(span(src, &params[1]), "#(", "unused: the token");
    assert_eq!(span(src, &params[2]), "%3");
    let body = items[2].as_list().expect("body");
    assert_eq!(body[1].as_sym(), Some("%1"));
    assert_eq!(span(src, &body[1]), "%");
    assert_eq!(span(src, &body[2]), "%3");
}

#[test]
fn set_shorthand() {
    assert_eq!(show("#{1 2}"), "(hash-set 1 2)");
    assert_eq!(show("#{}"), "(hash-set)");
    assert_eq!(show("#{1 #{2}}"), "(hash-set 1 (hash-set 2))");
    assert_eq!(err("#{1 2"), ReadErrorKind::Unclosed { open: '{' });
    let src = " #{a b}";
    let f = &read_all(src, "t").expect("reads")[0];
    assert_eq!(span(src, f), "#{a b}");
    let items = f.as_list().expect("list");
    assert_eq!(span(src, &items[0]), "#{");
    assert_eq!(items[0].as_sym(), Some("hash-set"));
}

#[test]
fn ratio_shorthand() {
    assert_eq!(show("7/2"), "(/ 7 2)");
    assert_eq!(show("-7/2"), "(/ -7 2)");
    for bad in [
        "7/-2", "7/2i32", "7i32/2", "0x7/2", "1_0/2", "7/2.5", "7/", "1/2/3",
    ] {
        assert!(
            matches!(err(bad), ReadErrorKind::InvalidNumber { .. }),
            "{bad}"
        );
    }
    let src = "(f -17/203)";
    let f = &read_all(src, "t").expect("reads")[0];
    let r = &f.as_list().expect("list")[1];
    assert_eq!(span(src, r), "-17/203");
    let parts = r.as_list().expect("ratio");
    assert_eq!(span(src, &parts[0]), "/");
    assert_eq!(span(src, &parts[1]), "-17");
    assert_eq!(span(src, &parts[2]), "203");
    assert_eq!(parts[2].pos.col, 8);
}

#[test]
fn unquote_is_tilde_and_comma_is_whitespace() {
    assert_eq!(show("~x ~@x"), "(unquote x) (unquote-splicing x)");
    assert_eq!(show("[1,2]"), "[1 2]");
    assert_eq!(show(",,,"), "");
    assert_eq!(show("a,b"), "a b");
    assert_eq!(show("\\,"), "\\,");
    assert_eq!(err("~"), ReadErrorKind::PrefixWithoutForm("~"));
    assert_eq!(err("~,x"), ReadErrorKind::PrefixWithoutForm("~"));
    assert_eq!(err("~@ x"), ReadErrorKind::PrefixWithoutForm("~@"));
    let src = "~@xs";
    let f = one(src);
    assert_eq!(span(src, &f.as_list().expect("list")[0]), "~@");
}
